//! Deterministic, transport-agnostic transfer coordination.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};

pub const AEAD_TAG_BYTES: u64 = 16;
pub const MAX_CIPHERTEXT_BYTES: u64 = 25_000_000;
pub const MAX_CHUNKS: u64 = 65_535;

const MAX_CONCURRENCY: u32 = 8;
const SAMPLE_WINDOW_MS: u64 = 2_000;
const PROBE_MIN_RATE: f64 = 256.0 * 1024.0 / 1_000.0;
const PROBE_COOLDOWN_MS: u64 = 2_000;
const CONGESTION_COOLDOWN_MS: u64 = 8_000;
const MAX_RETRY_DELAY_MS: u64 = 60_000;
const MAX_STAGE_RETRIES: u32 = 8;
const MAX_STAGE_RESETS: u32 = 2;
const DEFAULT_FIXED_MEMORY_BYTES: u64 = 8 * 1024 * 1024;
const RETIRED_REQUEST_IDS: usize = 1_024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdaptiveConcurrency {
    current: u32,
    maximum: u32,
    requests: HashMap<String, u64>,
    retired_requests: HashSet<String>,
    retired_order: VecDeque<String>,
    window_start: Option<u64>,
    window_bytes: u64,
    events: u32,
    measured_rate: f64,
    probe_rate: Option<f64>,
    cooldown_until: u64,
    successes: u32,
}

impl AdaptiveConcurrency {
    pub fn new(maximum: u32) -> Self {
        Self {
            current: 1,
            maximum: maximum.clamp(1, MAX_CONCURRENCY),
            requests: HashMap::new(),
            retired_requests: HashSet::new(),
            retired_order: VecDeque::new(),
            window_start: None,
            window_bytes: 0,
            events: 0,
            measured_rate: 0.0,
            probe_rate: None,
            cooldown_until: 0,
            successes: 0,
        }
    }

    pub fn limit(&self) -> u32 {
        self.current
    }
    pub fn rate(&self) -> f64 {
        self.measured_rate
    }

    pub fn sample(&mut self, key: &str, loaded: u64, now_ms: u64) {
        if self.retired_requests.contains(key) {
            if loaded != 0 {
                return;
            }
            self.retired_requests.remove(key);
            self.retired_order.retain(|retired| retired != key);
        }
        let previous = *self.requests.get(key).unwrap_or(&0);
        if loaded < previous {
            // A retry has a new byte counter; it must never subtract bytes.
            self.requests.insert(key.to_owned(), loaded);
            return;
        }
        self.requests.insert(key.to_owned(), loaded);
        let delta = loaded - previous;
        if delta == 0 {
            return;
        }
        let start = *self.window_start.get_or_insert(now_ms);
        self.window_bytes = self.window_bytes.saturating_add(delta);
        self.events = self.events.saturating_add(1);
        let elapsed = now_ms.saturating_sub(start);
        if elapsed >= SAMPLE_WINDOW_MS && self.events >= 3 {
            let rate = self.window_bytes as f64 / elapsed as f64;
            self.learn_aggregate(rate, now_ms);
            self.window_start = Some(now_ms);
            self.window_bytes = 0;
            self.events = 0;
        }
    }

    pub fn forget(&mut self, key: &str) {
        if self.requests.remove(key).is_some() && self.retired_requests.insert(key.to_owned()) {
            self.retired_order.push_back(key.to_owned());
            if self.retired_order.len() > RETIRED_REQUEST_IDS
                && let Some(oldest) = self.retired_order.pop_front()
            {
                self.retired_requests.remove(&oldest);
            }
        }
        if self.requests.is_empty() {
            self.window_start = None;
            self.window_bytes = 0;
            self.events = 0;
        }
    }

    pub fn observe(&mut self, bytes: u64, elapsed_ms: u64, now_ms: u64) {
        if bytes == 0 || elapsed_ms == 0 {
            return;
        }
        self.record_rate(bytes as f64 / elapsed_ms as f64);
        self.successes = self.successes.saturating_add(1);
        if now_ms >= self.cooldown_until
            && self.successes >= self.current.saturating_mul(2)
            && self.measured_rate >= PROBE_MIN_RATE
            && self.maximum > 1
            && self.current < self.maximum
        {
            self.probe_rate = Some(self.measured_rate);
            self.cooldown_until = now_ms.saturating_add(PROBE_COOLDOWN_MS);
            self.current += 1;
            self.successes = 0;
        }
    }

    pub fn congested(&mut self, now_ms: u64) {
        self.probe_rate = None;
        self.successes = 0;
        self.cooldown_until = now_ms.saturating_add(CONGESTION_COOLDOWN_MS);
        self.current = (self.current / 2).max(1);
    }

    fn record_rate(&mut self, rate: f64) {
        if !rate.is_finite() || rate <= 0.0 {
            return;
        }
        self.measured_rate = if self.measured_rate == 0.0 {
            rate
        } else {
            self.measured_rate * 0.5 + rate * 0.5
        };
    }

    // Completion timing is useful for conservative startup, but only an
    // aggregate sampled body rate can accept or reject an added slot.
    fn learn_aggregate(&mut self, rate: f64, now_ms: u64) {
        self.record_rate(rate);
        if !rate.is_finite() || rate <= 0.0 || now_ms < self.cooldown_until {
            return;
        }
        if let Some(probe) = self.probe_rate.take() {
            if rate < probe * 1.1 {
                self.current = self.current.saturating_sub(1).max(1);
                self.cooldown_until = now_ms.saturating_add(CONGESTION_COOLDOWN_MS);
            } else {
                self.cooldown_until = now_ms.saturating_add(PROBE_COOLDOWN_MS);
            }
            self.successes = 0;
        }
    }
}

/// Explicit transfer-buffer accounting supplied by the adapter that owns buffers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferMemoryBudget {
    /// Total bytes available to all live transfer buffers.
    pub total_bytes: u64,
    /// Shared crypto, scheduler, and work-buffer reservation.
    pub fixed_overhead_bytes: u64,
    /// Simultaneously retained plaintext copies for each in-flight request.
    pub plaintext_copies: u32,
    /// Simultaneously retained ciphertext copies for each in-flight request.
    pub ciphertext_copies: u32,
}

impl TransferMemoryBudget {
    pub const fn new(
        total_bytes: u64,
        fixed_overhead_bytes: u64,
        plaintext_copies: u32,
        ciphertext_copies: u32,
    ) -> Self {
        Self {
            total_bytes,
            fixed_overhead_bytes,
            plaintext_copies,
            ciphertext_copies,
        }
    }

    pub const fn standard(total_bytes: u64) -> Self {
        Self::new(total_bytes, DEFAULT_FIXED_MEMORY_BYTES, 1, 1)
    }

    pub fn slot_bytes(&self, plaintext_chunk_bytes: u64) -> Option<u64> {
        let ciphertext = plaintext_chunk_bytes.checked_add(AEAD_TAG_BYTES)?;
        plaintext_chunk_bytes
            .checked_mul(u64::from(self.plaintext_copies))?
            .checked_add(ciphertext.checked_mul(u64::from(self.ciphertext_copies))?)
    }
}

pub fn concurrency_limit_with_memory(
    configured: u32,
    chunk_bytes: u64,
    memory: TransferMemoryBudget,
    platform_limit: u32,
) -> u32 {
    let requested = configured
        .clamp(1, MAX_CONCURRENCY)
        .min(platform_limit.clamp(1, MAX_CONCURRENCY));
    let available = memory
        .total_bytes
        .saturating_sub(memory.fixed_overhead_bytes);
    let memory_slots = memory
        .slot_bytes(chunk_bytes)
        .map(|slot| (available / slot.max(1)).clamp(1, u64::from(MAX_CONCURRENCY)) as u32)
        .unwrap_or(1);
    requested.min(memory_slots).max(1)
}

pub fn concurrency_limit(
    configured: u32,
    chunk_bytes: u64,
    memory_budget: u64,
    platform_limit: u32,
) -> u32 {
    concurrency_limit_with_memory(
        configured,
        chunk_bytes,
        TransferMemoryBudget::standard(memory_budget),
        platform_limit,
    )
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct UploadTransport {
    pub version: u8,
    pub part_min_bytes: u64,
    pub part_max_bytes: u64,
    pub request_target_ms: u64,
    pub request_budget_ms: u64,
    #[serde(default)]
    pub part_max_count: Option<u32>,
}

impl UploadTransport {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 {
            return Err("unsupported upload transport version".into());
        }
        if self.part_min_bytes == 0
            || self.part_max_bytes < self.part_min_bytes
            || self.part_max_bytes > MAX_CIPHERTEXT_BYTES
        {
            return Err("invalid upload part bounds".into());
        }
        if self.request_target_ms == 0 || self.request_budget_ms < self.request_target_ms {
            return Err("invalid upload request timing".into());
        }
        if self.part_max_count == Some(0) {
            return Err("invalid upload part count".into());
        }
        Ok(())
    }

    pub fn part_bytes(&self, bytes_per_ms: f64) -> u64 {
        if self.validate().is_err() {
            return 0;
        }
        let estimate = if bytes_per_ms.is_finite() && bytes_per_ms > 0.0 {
            (bytes_per_ms * self.request_target_ms as f64 * 0.75).floor()
        } else {
            self.part_min_bytes as f64
        };
        estimate.clamp(self.part_min_bytes as f64, self.part_max_bytes as f64) as u64
    }
    pub fn initial_part_bytes(&self, bytes_per_ms: f64) -> u64 {
        self.part_bytes(bytes_per_ms)
    }
    pub fn grow_part(&self, previous: u64, bytes: u64, elapsed_ms: u64) -> u64 {
        if self.validate().is_err() {
            return 0;
        }
        let target = if elapsed_ms == 0 {
            previous
        } else {
            self.part_bytes(bytes as f64 / elapsed_ms as f64)
        };
        target
            .min(previous.saturating_mul(3) / 2)
            .clamp(self.part_min_bytes, self.part_max_bytes)
    }
    pub fn shrink_part(&self, previous: u64) -> u64 {
        if self.validate().is_err() {
            return 0;
        }
        (previous / 2).clamp(self.part_min_bytes, self.part_max_bytes)
    }
    pub fn should_stage(&self, bytes: u64, bytes_per_ms: f64) -> bool {
        self.validate().is_ok()
            && bytes_per_ms.is_finite()
            && bytes_per_ms > 0.0
            && (bytes as f64 / bytes_per_ms) > self.request_budget_ms as f64 * 0.75
    }
    pub fn should_abandon_direct(&self, bytes: u64, loaded: u64, elapsed_ms: u64) -> bool {
        self.validate().is_ok()
            && elapsed_ms >= 3_000
            && loaded > 0
            && loaded < bytes
            && (bytes as f64 / (loaded as f64 / elapsed_ms as f64))
                > self.request_budget_ms as f64 * 0.8
    }

    /// Reject a staged layout before issuing any part requests.
    pub fn validate_part_count(
        &self,
        ciphertext_bytes: u64,
        part_bytes: u64,
    ) -> Result<(), String> {
        self.validate()?;
        if ciphertext_bytes == 0 || ciphertext_bytes > MAX_CIPHERTEXT_BYTES {
            return Err("invalid staged ciphertext size".into());
        }
        if part_bytes < self.part_min_bytes || part_bytes > self.part_max_bytes {
            return Err("invalid staged part size".into());
        }
        let count = ciphertext_bytes.div_ceil(part_bytes);
        if self
            .part_max_count
            .is_some_and(|maximum| count > u64::from(maximum))
        {
            return Err("staged part count exceeds transport policy".into());
        }
        Ok(())
    }

    /// Returns remaining requests after a recovered staged offset.
    pub fn remaining_part_count(
        &self,
        ciphertext_bytes: u64,
        offset: u64,
        part_bytes: u64,
    ) -> Result<u32, String> {
        self.validate_part_count(ciphertext_bytes, part_bytes)?;
        if offset > ciphertext_bytes {
            return Err("staged offset exceeds ciphertext size".into());
        }
        u32::try_from((ciphertext_bytes - offset).div_ceil(part_bytes))
            .map_err(|_| "staged part count exceeds u32".into())
    }
}

pub fn retryable_status(status: u16, staging: bool) -> bool {
    status == 408 || status == 429 || status >= 500 || (staging && status == 423)
}

pub fn retry_delay_ms(attempt: u32, retry_after_ms: Option<u64>, jitter_ms: u64) -> u64 {
    if let Some(delay) = retry_after_ms {
        return delay.min(MAX_RETRY_DELAY_MS);
    }
    let exponent = attempt.min(16);
    250u64
        .saturating_mul(1u64 << exponent)
        .saturating_add(jitter_ms)
        .min(MAX_RETRY_DELAY_MS)
}

pub fn chunk_count(plaintext_bytes: u64, chunk_bytes: u64) -> Result<u64, String> {
    if chunk_bytes == 0 || chunk_bytes > MAX_CIPHERTEXT_BYTES - AEAD_TAG_BYTES {
        return Err("invalid plaintext chunk size".into());
    }
    let count = plaintext_bytes.div_ceil(chunk_bytes);
    if count > MAX_CHUNKS {
        Err("chunk count exceeds limit".into())
    } else {
        Ok(count)
    }
}

pub fn ciphertext_bytes(plaintext_bytes: u64, chunk_bytes: u64) -> Result<u64, String> {
    let count = chunk_count(plaintext_bytes, chunk_bytes)?;
    plaintext_bytes
        .checked_add(
            count
                .checked_mul(AEAD_TAG_BYTES)
                .ok_or("ciphertext size overflow")?,
        )
        .ok_or_else(|| "ciphertext size overflow".into())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StageState {
    Receiving,
    Finalizing,
    Complete,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageStatus {
    pub id: String,
    pub state: StageState,
    pub offset: u64,
    pub ciphertext_bytes: u64,
    pub checksum: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StageAction {
    Continue { offset: u64 },
    Finalizing,
    Complete,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageSession {
    pub id: String,
    pub ciphertext_bytes: u64,
    pub checksum: String,
    pub offset: u64,
    pub state: StageState,
    pub retries: u32,
    pub resets: u32,
    pub reprobe_allowed: bool,
}

impl StageSession {
    pub fn new(id: String, ciphertext_bytes: u64, checksum: String) -> Result<Self, String> {
        if id.is_empty()
            || checksum.is_empty()
            || ciphertext_bytes == 0
            || ciphertext_bytes > MAX_CIPHERTEXT_BYTES
        {
            return Err("invalid stage identity".into());
        }
        Ok(Self {
            id,
            ciphertext_bytes,
            checksum,
            offset: 0,
            state: StageState::Receiving,
            retries: 0,
            resets: 0,
            reprobe_allowed: false,
        })
    }
    pub fn reconcile(&mut self, status: &StageStatus) -> Result<StageAction, String> {
        self.validate_status(status)?;
        self.offset = status.offset;
        self.state = status.state;
        Ok(match status.state {
            StageState::Receiving => StageAction::Continue {
                offset: status.offset,
            },
            StageState::Finalizing => StageAction::Finalizing,
            StageState::Complete => StageAction::Complete,
        })
    }

    fn validate_status(&self, status: &StageStatus) -> Result<(), String> {
        if status.id != self.id
            || status.ciphertext_bytes != self.ciphertext_bytes
            || status.checksum != self.checksum
        {
            return Err("stage identity mismatch".into());
        }
        if status.offset > self.ciphertext_bytes
            || (status.state != StageState::Receiving && status.offset != self.ciphertext_bytes)
        {
            return Err("invalid stage offset".into());
        }
        if status.offset < self.offset {
            return Err("stage offset moved backwards".into());
        }
        if stage_state_rank(status.state) < stage_state_rank(self.state) {
            return Err("stage state moved backwards".into());
        }
        Ok(())
    }
    pub fn acknowledge_part(
        &mut self,
        start: u64,
        bytes: u64,
        status: &StageStatus,
    ) -> Result<StageAction, String> {
        let expected = start.checked_add(bytes).ok_or("part offset overflow")?;
        if start != self.offset || bytes == 0 || expected > self.ciphertext_bytes {
            return Err("invalid staged part range".into());
        }
        self.validate_status(status)?;
        if status.offset != expected {
            return Err("stage did not acknowledge requested part".into());
        }
        self.offset = status.offset;
        self.state = status.state;
        self.retries = 0;
        self.reprobe_allowed = true;
        Ok(match status.state {
            StageState::Receiving => StageAction::Continue {
                offset: status.offset,
            },
            StageState::Finalizing => StageAction::Finalizing,
            StageState::Complete => StageAction::Complete,
        })
    }
    pub fn record_retry(&mut self) -> Result<(), String> {
        if self.retries >= MAX_STAGE_RETRIES {
            Err("staged upload retry limit exceeded".into())
        } else {
            self.retries += 1;
            Ok(())
        }
    }
    pub fn reset(&mut self, id: String) -> Result<(), String> {
        if id.is_empty() {
            return Err("invalid stage identity".into());
        }
        if self.resets >= MAX_STAGE_RESETS {
            return Err("staged upload reset limit exceeded".into());
        }
        self.resets += 1;
        self.id = id;
        self.offset = 0;
        self.state = StageState::Receiving;
        self.retries = 0;
        self.reprobe_allowed = false;
        Ok(())
    }
    pub fn reprobe_after_recovery(&mut self) -> bool {
        std::mem::take(&mut self.reprobe_allowed)
    }
}

fn stage_state_rank(state: StageState) -> u8 {
    match state {
        StageState::Receiving => 0,
        StageState::Finalizing => 1,
        StageState::Complete => 2,
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferProgress {
    pub plaintext_total: u64,
    pub ciphertext_total: u64,
    pub completed_ciphertext: u64,
    pub active_ciphertext: u64,
}

impl TransferProgress {
    pub fn new(plaintext_total: u64, ciphertext_total: u64) -> Result<Self, String> {
        if ciphertext_total < plaintext_total {
            return Err("ciphertext total is smaller than plaintext total".into());
        }
        Ok(Self {
            plaintext_total,
            ciphertext_total,
            completed_ciphertext: 0,
            active_ciphertext: 0,
        })
    }
    pub fn set_active(&mut self, bytes: u64) -> Result<(), String> {
        if bytes
            > self
                .ciphertext_total
                .saturating_sub(self.completed_ciphertext)
        {
            return Err("active progress exceeds remaining bytes".into());
        }
        self.active_ciphertext = bytes;
        Ok(())
    }
    pub fn complete(&mut self, bytes: u64) -> Result<(), String> {
        let total = self
            .completed_ciphertext
            .checked_add(bytes)
            .ok_or("progress overflow")?;
        if total > self.ciphertext_total {
            return Err("completed progress exceeds total".into());
        }
        self.completed_ciphertext = total;
        self.active_ciphertext = 0;
        Ok(())
    }
}
