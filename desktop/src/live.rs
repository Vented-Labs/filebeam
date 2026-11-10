//! One snapshot feed for the window.
//!
//! The service worker publishes into a shared snapshot without signalling. This
//! entity polls it once, compares a fingerprint of everything a view can show, and
//! notifies observers only when that changes, so an idle window draws no frames.

use std::{
    hash::{DefaultHasher, Hash, Hasher},
    mem::discriminant,
    sync::Arc,
    time::Duration,
};

use gpui::{Context, Task};

use crate::{
    client::DesktopClient,
    model::{ClientCommand, DesktopSnapshot, KeyCustody},
};

const POLL: Duration = Duration::from_millis(100);

pub struct Live {
    client: Arc<DesktopClient>,
    snapshot: Arc<DesktopSnapshot>,
    fingerprint: u64,
    revision: u64,
    _poll: Task<()>,
}

impl Live {
    pub fn new(client: Arc<DesktopClient>, cx: &mut Context<Self>) -> Self {
        let poll = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                if this.update(cx, |live, cx| live.refresh(cx)).is_err() {
                    return;
                }
            }
        });
        let snapshot = client.snapshot();
        Self {
            fingerprint: fingerprint(&snapshot),
            snapshot: Arc::new(snapshot),
            client,
            revision: 0,
            _poll: poll,
        }
    }

    /// The most recent state. Cheap to clone and safe to hold across a render.
    pub fn snapshot(&self) -> Arc<DesktopSnapshot> {
        self.snapshot.clone()
    }

    pub fn client(&self) -> &Arc<DesktopClient> {
        &self.client
    }

    /// Increases whenever the visible state changes.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Queues work for the service worker.
    pub fn dispatch(&self, command: ClientCommand) -> anyhow::Result<()> {
        self.client.dispatch(command)
    }

    /// Reads the shared snapshot now instead of waiting for the next poll.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        let snapshot = self.client.snapshot();
        let next = fingerprint(&snapshot);
        if next != self.fingerprint {
            self.fingerprint = next;
            self.snapshot = Arc::new(snapshot);
            self.revision = self.revision.wrapping_add(1);
            cx.notify();
        }
    }
}

/// Hashes every field a view presents. A field missing here would leave the screen
/// stale until something else changed, so additions to the model belong here too.
pub fn fingerprint(snapshot: &DesktopSnapshot) -> u64 {
    let h = &mut DefaultHasher::new();

    snapshot.instance.url.hash(h);
    snapshot.instance.connected.hash(h);
    snapshot.instance.detail.hash(h);

    let policy = &snapshot.policy;
    policy.origin.hash(h);
    discriminant(&policy.availability).hash(h);
    policy.reason.hash(h);
    policy.anonymous_uploads.hash(h);
    discriminant(&policy.default_transport).hash(h);
    for driver in [&policy.http, &policy.webrtc] {
        driver.enabled.hash(h);
        driver.maximum_ciphertext_bytes.hash(h);
        driver.maximum_file_count.hash(h);
        driver.maximum_note_bytes.hash(h);
    }
    policy.default_retention_hours.hash(h);
    policy.retention_options_hours.hash(h);

    let settings = &snapshot.settings;
    settings.memory_limit_mib.hash(h);
    settings.max_concurrency.hash(h);
    settings.relay_only.hash(h);
    settings.auto_update.hash(h);
    settings.restart_required.hash(h);

    let account = &snapshot.account;
    account.authenticated.hash(h);
    account.email.hash(h);
    account.username.hash(h);
    account.profile_url.hash(h);
    account.email_verified.hash(h);
    account.inbox_enabled.hash(h);
    account
        .notification_channel
        .as_ref()
        .map(discriminant)
        .hash(h);
    discriminant(&account.key_custody).hash(h);
    match &account.key_custody {
        KeyCustody::PendingBackup {
            generation_id,
            fingerprint,
        } => {
            generation_id.hash(h);
            fingerprint.hash(h);
        }
        KeyCustody::Locked {
            version,
            fingerprint,
            custody_mode,
        }
        | KeyCustody::Configured {
            version,
            fingerprint,
            custody_mode,
        } => {
            version.hash(h);
            fingerprint.hash(h);
            custody_mode.hash(h);
        }
        KeyCustody::Unknown
        | KeyCustody::Local
        | KeyCustody::PasswordWrapped
        | KeyCustody::ServerWrapped => {}
    }

    snapshot.jobs.len().hash(h);
    for job in &snapshot.jobs {
        job.id.hash(h);
        discriminant(&job.direction).hash(h);
        discriminant(&job.state).hash(h);
        let progress = &job.progress;
        progress.phase.hash(h);
        progress.name.hash(h);
        progress.item_index.hash(h);
        progress.item_count.hash(h);
        progress.total_bytes.hash(h);
        progress.completed_bytes.hash(h);
        progress.committed_bytes.hash(h);
        progress.wire_bytes.hash(h);
        job.share_url.hash(h);
        job.separate_key.hash(h);
        job.cli_command.hash(h);
        job.checkpoint_id.hash(h);
        job.origin.hash(h);
        if let Some(error) = &job.error {
            discriminant(&error.code).hash(h);
            error.detail.hash(h);
        }
        job.error.is_some().hash(h);
        job.peer_warning.hash(h);
        let can = &job.capabilities;
        [
            can.can_pause,
            can.can_resume,
            can.can_end_live,
            can.can_revoke,
            can.can_discard,
            can.can_export,
            can.can_retry_export,
        ]
        .hash(h);
    }

    snapshot.inbox.len().hash(h);
    for item in &snapshot.inbox {
        item.id.hash(h);
        item.ciphertext_bytes.hash(h);
        item.item_count.hash(h);
        item.completed_at.hash(h);
        item.expires_at.hash(h);
        item.item_ids.hash(h);
    }

    snapshot.history.revision.hash(h);
    snapshot.history.error.hash(h);
    snapshot.history.page.data.len().hash(h);
    snapshot.history.page.next_cursor.hash(h);

    // The contact and staging records are serialisable service types; their encoded
    // form covers every field without mirroring them here.
    serde_json::to_vec(&snapshot.contacts)
        .unwrap_or_default()
        .hash(h);
    serde_json::to_vec(&snapshot.staged_inbox)
        .unwrap_or_default()
        .hash(h);
    snapshot.auto_receiving.hash(h);

    snapshot.receive_previews.len().hash(h);
    for preview in &snapshot.receive_previews {
        if let Some(note) = &preview.attached_note {
            note.title.hash(h);
            note.language.hash(h);
            note.text.hash(h);
        }
        preview.attached_note.is_some().hash(h);
        preview.operation_id.hash(h);
        preview.link.hash(h);
        discriminant(&preview.kind).hash(h);
        preview.password_required.hash(h);
        preview.burn_on_read.hash(h);
        for item in &preview.items {
            item.id.hash(h);
            item.name.hash(h);
            item.size.hash(h);
        }
    }

    snapshot.notes.len().hash(h);
    for note in &snapshot.notes {
        note.id.hash(h);
        note.link.hash(h);
        note.separate_key.hash(h);
        discriminant(&note.transport).hash(h);
        discriminant(&note.state).hash(h);
        note.burn_on_read.hash(h);
        note.retry_burn.hash(h);
        note.text_available.hash(h);
        if let Some(error) = &note.error {
            error.detail.hash(h);
        }
        note.error.is_some().hash(h);
    }

    let devices = &snapshot.device_directory;
    discriminant(&devices.availability).hash(h);
    devices.detail.hash(h);
    devices.enrollment_payload.hash(h);
    devices.pending_fingerprint.hash(h);
    for device in &devices.devices {
        device.id.hash(h);
        device.name.hash(h);
        device.platform.hash(h);
        discriminant(&device.trust).hash(h);
        discriminant(&device.availability).hash(h);
        device.detail.hash(h);
        device.fingerprint.hash(h);
    }
    let daemon = &devices.daemon;
    daemon.connected.hash(h);
    daemon.receive_enabled.hash(h);
    daemon.runtime.hash(h);
    daemon.detail.hash(h);
    for destination in &daemon.destinations {
        destination.id.hash(h);
        destination.alias.hash(h);
        destination.approval.hash(h);
        destination.enabled.hash(h);
    }
    for job in &daemon.jobs {
        job.id.hash(h);
        job.target.hash(h);
        job.state.hash(h);
        job.prepared.hash(h);
        job.error.hash(h);
    }
    for device in &daemon.devices {
        device.id.hash(h);
        device.key_version.hash(h);
        device.status.hash(h);
        device.fingerprint.hash(h);
    }

    // Events are append-only: their count and newest entry identify the list.
    snapshot.messages.len().hash(h);
    if let Some(message) = snapshot.messages.last() {
        message.id.hash(h);
    }
    snapshot.operations.len().hash(h);
    if let Some(operation) = snapshot.operations.last() {
        operation.id.hash(h);
    }

    if let Some(invitation) = &snapshot.invitation {
        invitation.token.hash(h);
        invitation.email.hash(h);
        invitation.expires_at.hash(h);
    }
    snapshot.invitation.is_some().hash(h);

    snapshot.pending_prompts.len().hash(h);
    for prompt in &snapshot.pending_prompts {
        prompt.transfer_id.hash(h);
        prompt.id.hash(h);
        discriminant(&prompt.kind).hash(h);
        prompt.peer.hash(h);
        if let Some(directory) = &prompt.directory {
            directory.files.hash(h);
            directory.bytes.hash(h);
            directory.maximum_files.hash(h);
        }
    }

    snapshot.verified_results.len().hash(h);
    for result in &snapshot.verified_results {
        result.transfer_id.hash(h);
        result.private_path.hash(h);
        result.verified_paths.hash(h);
        result.exported_paths.hash(h);
        if let Some(error) = &result.export_error {
            error.detail.hash(h);
        }
        result.export_error.is_some().hash(h);
    }

    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        ClientError, ClientErrorCode, JobSnapshot, PendingPrompt, PromptKind, TransferCapabilities,
        TransferDirection, TransferProgress, TransferState,
    };

    fn job() -> JobSnapshot {
        JobSnapshot {
            id: "job-1".into(),
            direction: TransferDirection::Send,
            state: TransferState::Running,
            progress: TransferProgress {
                phase: "Uploading".into(),
                name: "report.pdf".into(),
                ..TransferProgress::default()
            },
            share_url: None,
            separate_key: None,
            cli_command: None,
            checkpoint_id: None,
            origin: None,
            error: None,
            peer_warning: None,
            capabilities: TransferCapabilities::default(),
        }
    }

    fn with_job(change: impl FnOnce(&mut JobSnapshot)) -> u64 {
        let mut snapshot = DesktopSnapshot::default();
        let mut job = job();
        change(&mut job);
        snapshot.jobs.push(job);
        fingerprint(&snapshot)
    }

    #[test]
    fn identical_snapshots_share_a_fingerprint() {
        assert_eq!(with_job(|_| {}), with_job(|_| {}));
        assert_eq!(
            fingerprint(&DesktopSnapshot::default()),
            fingerprint(&DesktopSnapshot::default())
        );
    }

    #[test]
    fn every_presented_job_field_is_observed_without_a_byte_change() {
        let base = with_job(|_| {});
        let changes: [(&str, fn(&mut JobSnapshot)); 8] = [
            ("phase", |job| {
                job.progress.phase = "Waiting for recipient".into()
            }),
            ("name", |job| job.progress.name = "renamed.pdf".into()),
            ("share link", |job| {
                job.share_url = Some("https://f.example/1".into())
            }),
            ("separate key", |job| job.separate_key = Some("key".into())),
            ("capability", |job| job.capabilities.can_pause = true),
            ("state", |job| job.state = TransferState::Paused),
            ("error", |job| {
                job.error = Some(ClientError {
                    code: ClientErrorCode::Network,
                    detail: "interrupted".into(),
                });
            }),
            ("item count", |job| job.progress.item_count = 3),
        ];
        for (name, change) in changes {
            assert_ne!(with_job(change), base, "{name} must change the fingerprint");
        }
    }

    #[test]
    fn account_policy_prompts_and_events_are_observed() {
        let base = fingerprint(&DesktopSnapshot::default());
        let mut signed_in = DesktopSnapshot::default();
        signed_in.account.authenticated = true;
        assert_ne!(fingerprint(&signed_in), base);

        let mut policy = DesktopSnapshot::default();
        policy.policy.retention_options_hours = vec![1, 24];
        assert_ne!(fingerprint(&policy), base);

        let mut prompt = DesktopSnapshot::default();
        prompt.pending_prompts.push(PendingPrompt {
            transfer_id: "receive-a".into(),
            id: 1,
            kind: PromptKind::Password,
            peer: None,
            directory: None,
        });
        let first = fingerprint(&prompt);
        assert_ne!(first, base);
        prompt.pending_prompts[0].id = 2;
        assert_ne!(fingerprint(&prompt), first);

        let mut operation = DesktopSnapshot::default();
        operation.operations.push(crate::model::OperationStatus {
            id: 1,
            operation: "login".into(),
        });
        let first = fingerprint(&operation);
        assert_ne!(first, base);
        // The log is trimmed at capacity, so its length alone can stand still.
        operation.operations[0].id = 2;
        assert_ne!(fingerprint(&operation), first);
    }
}
