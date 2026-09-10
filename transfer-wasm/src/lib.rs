use filebeam_transfer::{
    AdaptiveConcurrency, StageSession, StageStatus, UploadTransport, chunk_count, ciphertext_bytes,
    concurrency_limit, retry_delay_ms, retryable_status,
};
use wasm_bindgen::prelude::*;

fn transport(value: JsValue) -> Result<UploadTransport, JsError> {
    serde_wasm_bindgen::from_value(value).map_err(|error| JsError::new(&error.to_string()))
}

fn result<T>(value: Result<T, String>) -> Result<T, JsError> {
    value.map_err(|error| JsError::new(&error))
}

// wasm-bindgen exposes Rust u64 as BigInt. Browser byte counters and monotonic clocks are
// safe JavaScript numbers, so validate them at the boundary and retain u64 internally.
fn integer(value: f64) -> Result<u64, JsError> {
    if !value.is_finite() || value < 0.0 || value > u64::MAX as f64 || value.fract() != 0.0 {
        return Err(JsError::new("expected a non-negative integer"));
    }
    Ok(value as u64)
}

#[wasm_bindgen]
pub struct AdaptiveConcurrencyController {
    inner: AdaptiveConcurrency,
}

#[wasm_bindgen]
impl AdaptiveConcurrencyController {
    #[wasm_bindgen(constructor)]
    pub fn new(maximum: u32) -> Self {
        Self {
            inner: AdaptiveConcurrency::new(maximum),
        }
    }

    #[wasm_bindgen(getter)]
    pub fn limit(&self) -> u32 {
        self.inner.limit()
    }

    #[wasm_bindgen(getter)]
    pub fn rate(&self) -> f64 {
        self.inner.rate()
    }

    pub fn sample(&mut self, key: &str, loaded: f64, now_ms: f64) -> Result<(), JsError> {
        self.inner.sample(key, integer(loaded)?, integer(now_ms)?);
        Ok(())
    }

    pub fn forget(&mut self, key: &str) {
        self.inner.forget(key);
    }

    pub fn observe(&mut self, bytes: f64, elapsed_ms: f64, now_ms: f64) -> Result<(), JsError> {
        self.inner
            .observe(integer(bytes)?, integer(elapsed_ms)?, integer(now_ms)?);
        Ok(())
    }

    pub fn congested(&mut self, now_ms: f64) -> Result<(), JsError> {
        self.inner.congested(integer(now_ms)?);
        Ok(())
    }
}

/// Validates untrusted stage responses without exposing the ciphertext itself to WASM.
#[wasm_bindgen]
pub struct StageSessionController {
    inner: StageSession,
}

#[wasm_bindgen]
impl StageSessionController {
    #[wasm_bindgen(constructor)]
    pub fn new(id: String, ciphertext_bytes: f64, checksum: String) -> Result<Self, JsError> {
        Ok(Self {
            inner: result(StageSession::new(id, integer(ciphertext_bytes)?, checksum))?,
        })
    }

    #[wasm_bindgen(js_name = reconcile)]
    pub fn reconcile_status(&mut self, status: JsValue) -> Result<JsValue, JsError> {
        let status: StageStatus = serde_wasm_bindgen::from_value(status)
            .map_err(|error| JsError::new(&error.to_string()))?;
        result(self.inner.reconcile(&status))?;
        serde_wasm_bindgen::to_value(&self.inner).map_err(|error| JsError::new(&error.to_string()))
    }

    #[wasm_bindgen(js_name = acknowledgePart)]
    pub fn acknowledge_part(
        &mut self,
        start: f64,
        bytes: f64,
        status: JsValue,
    ) -> Result<JsValue, JsError> {
        let status: StageStatus = serde_wasm_bindgen::from_value(status)
            .map_err(|error| JsError::new(&error.to_string()))?;
        result(
            self.inner
                .acknowledge_part(integer(start)?, integer(bytes)?, &status),
        )?;
        serde_wasm_bindgen::to_value(&self.inner).map_err(|error| JsError::new(&error.to_string()))
    }

    #[wasm_bindgen(js_name = recordRetry)]
    pub fn record_retry(&mut self) -> Result<(), JsError> {
        result(self.inner.record_retry())
    }
    pub fn reset(&mut self, id: String) -> Result<(), JsError> {
        result(self.inner.reset(id))
    }
    #[wasm_bindgen(js_name = reprobeAfterRecovery)]
    pub fn reprobe_after_recovery(&mut self) -> bool {
        self.inner.reprobe_after_recovery()
    }
    #[wasm_bindgen(getter)]
    pub fn retries(&self) -> u32 {
        self.inner.retries
    }
}

#[wasm_bindgen(js_name = concurrencyLimit)]
pub fn wasm_concurrency_limit(
    configured: u32,
    chunk_bytes: f64,
    memory_budget: f64,
    platform_limit: u32,
) -> u32 {
    match (integer(chunk_bytes), integer(memory_budget)) {
        (Ok(chunk_bytes), Ok(memory_budget)) => {
            concurrency_limit(configured, chunk_bytes, memory_budget, platform_limit)
        }
        _ => 1,
    }
}

#[wasm_bindgen(js_name = retryableStatus)]
pub fn wasm_retryable_status(status: u16, staging: bool) -> bool {
    retryable_status(status, staging)
}

#[wasm_bindgen(js_name = retryDelayMs)]
pub fn wasm_retry_delay_ms(
    attempt: u32,
    retry_after_ms: Option<f64>,
    jitter_ms: f64,
) -> Result<f64, JsError> {
    Ok(retry_delay_ms(
        attempt,
        retry_after_ms.map(integer).transpose()?,
        integer(jitter_ms)?,
    ) as f64)
}

#[wasm_bindgen(js_name = chunkCount)]
pub fn wasm_chunk_count(plaintext_bytes: f64, chunk_bytes: f64) -> Result<f64, JsError> {
    Ok(result(chunk_count(
        integer(plaintext_bytes)?,
        integer(chunk_bytes)?,
    ))? as f64)
}

#[wasm_bindgen(js_name = ciphertextBytes)]
pub fn wasm_ciphertext_bytes(plaintext_bytes: f64, chunk_bytes: f64) -> Result<f64, JsError> {
    Ok(result(ciphertext_bytes(
        integer(plaintext_bytes)?,
        integer(chunk_bytes)?,
    ))? as f64)
}

#[wasm_bindgen(js_name = validateUploadTransport)]
pub fn validate_upload_transport(value: JsValue) -> Result<(), JsError> {
    result(transport(value)?.validate())
}

#[wasm_bindgen(js_name = initialPartBytes)]
pub fn initial_part_bytes(value: JsValue, bytes_per_ms: f64) -> Result<f64, JsError> {
    Ok(transport(value)?.initial_part_bytes(bytes_per_ms) as f64)
}

#[wasm_bindgen(js_name = partBytes)]
pub fn part_bytes(value: JsValue, bytes_per_ms: f64) -> Result<f64, JsError> {
    Ok(transport(value)?.part_bytes(bytes_per_ms) as f64)
}

#[wasm_bindgen(js_name = growPart)]
pub fn grow_part(
    value: JsValue,
    previous: f64,
    bytes: f64,
    elapsed_ms: f64,
) -> Result<f64, JsError> {
    Ok(
        transport(value)?.grow_part(integer(previous)?, integer(bytes)?, integer(elapsed_ms)?)
            as f64,
    )
}

#[wasm_bindgen(js_name = shrinkPart)]
pub fn shrink_part(value: JsValue, previous: f64) -> Result<f64, JsError> {
    Ok(transport(value)?.shrink_part(integer(previous)?) as f64)
}

#[wasm_bindgen(js_name = shouldStage)]
pub fn should_stage(value: JsValue, bytes: f64, bytes_per_ms: f64) -> Result<bool, JsError> {
    Ok(transport(value)?.should_stage(integer(bytes)?, bytes_per_ms))
}

#[wasm_bindgen(js_name = shouldAbandonDirect)]
pub fn should_abandon_direct(
    value: JsValue,
    bytes: f64,
    loaded: f64,
    elapsed_ms: f64,
) -> Result<bool, JsError> {
    Ok(transport(value)?.should_abandon_direct(
        integer(bytes)?,
        integer(loaded)?,
        integer(elapsed_ms)?,
    ))
}

#[wasm_bindgen(js_name = validatePartCount)]
pub fn validate_part_count(
    value: JsValue,
    ciphertext_bytes: f64,
    part_bytes: f64,
) -> Result<(), JsError> {
    result(transport(value)?.validate_part_count(integer(ciphertext_bytes)?, integer(part_bytes)?))
}
