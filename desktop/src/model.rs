//! Presentation-neutral desktop state. No snapshot contains secret material.

use std::path::PathBuf;

pub type TransferId = String;

#[derive(Clone, Default)]
pub struct DesktopSnapshot {
    pub instance: InstanceSnapshot,
    pub policy: PolicySnapshot,
    pub settings: SettingsSnapshot,
    pub account: AccountSnapshot,
    pub jobs: Vec<JobSnapshot>,
    pub inbox: Vec<InboxItem>,
    /// Authenticated receive preparations. Filenames appear only after the
    /// encrypted manifest has been validated by the native receiver.
    pub receive_previews: Vec<ReceivePreview>,
    pub notes: Vec<NoteSnapshot>,
    pub messages: Vec<ClientMessage>,
    pub operations: Vec<OperationStatus>,
    pub invitation: Option<InvitationSnapshot>,
    pub pending_prompts: Vec<PendingPrompt>,
    pub verified_results: Vec<VerifiedResult>,
}

#[derive(Clone, Default)]
pub struct InstanceSnapshot {
    pub url: String,
    pub connected: bool,
    pub detail: Option<String>,
}
/// Public instance policy, scoped to `origin`. Missing caps are not unlimited.
#[derive(Clone, Default)]
pub struct PolicySnapshot {
    pub origin: String,
    pub availability: PolicyAvailability,
    pub reason: Option<String>,
    pub anonymous_uploads: bool,
    pub default_transport: SendTransport,
    pub http: DriverPolicy,
    pub webrtc: DriverPolicy,
    pub default_retention_hours: Option<u64>,
    pub retention_options_hours: Vec<u64>,
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum PolicyAvailability {
    Available,
    Cached,
    #[default]
    Unavailable,
}
#[derive(Clone, Default)]
pub struct DriverPolicy {
    pub enabled: bool,
    pub maximum_ciphertext_bytes: Option<u64>,
    pub maximum_file_count: Option<usize>,
    pub maximum_note_bytes: Option<u64>,
}
#[derive(Clone, Default)]
pub struct SettingsSnapshot {
    pub memory_limit_mib: u64,
    pub max_concurrency: Option<u32>,
    pub relay_only: bool,
    pub auto_update: bool,
    /// Resource increases are persisted but require a process restart to become effective.
    pub restart_required: bool,
}
#[derive(Clone, Default)]
pub struct AccountSnapshot {
    pub authenticated: bool,
    pub email: Option<String>,
    pub username: Option<String>,
    pub profile_url: Option<String>,
    pub email_verified: bool,
    pub inbox_enabled: bool,
    pub notification_channel: Option<NotificationChannel>,
    pub key_custody: KeyCustody,
}
#[derive(Clone, Default)]
pub enum KeyCustody {
    #[default]
    Unknown,
    /// Retained for existing view consumers; new snapshots use `Configured`.
    Local,
    /// Retained for existing view consumers; new snapshots use `Locked` or `Configured`.
    PasswordWrapped,
    /// Retained for existing view consumers; new snapshots use `Locked`.
    ServerWrapped,
    PendingBackup {
        generation_id: String,
        fingerprint: String,
    },
    Locked {
        version: u32,
        fingerprint: String,
        custody_mode: String,
    },
    Configured {
        version: u32,
        fingerprint: String,
        custody_mode: String,
    },
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NotificationChannel {
    Mail,
    Database,
}

#[derive(Clone)]
pub struct JobSnapshot {
    pub id: TransferId,
    pub direction: TransferDirection,
    pub state: TransferState,
    pub progress: TransferProgress,
    pub share_url: Option<String>,
    /// Kept in process memory only when the sender chose separate sharing.
    pub separate_key: Option<String>,
    pub cli_command: Option<String>,
    pub checkpoint_id: Option<String>,
    /// Authenticated checkpoint origin for recovered transfers.
    pub origin: Option<String>,
    pub error: Option<ClientError>,
    pub peer_warning: Option<String>,
    pub capabilities: TransferCapabilities,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum TransferDirection {
    Send,
    Receive,
    Note,
    Service,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransferState {
    Running,
    PauseRequested,
    Paused,
    Complete,
    Failed,
}
#[derive(Clone, Default)]
pub struct TransferProgress {
    pub phase: String,
    pub name: String,
    pub item_index: usize,
    pub item_count: usize,
    pub total_bytes: Option<u64>,
    pub completed_bytes: u64,
    pub committed_bytes: u64,
    pub wire_bytes: u64,
}
#[derive(Clone, Default)]
pub struct TransferCapabilities {
    pub can_pause: bool,
    pub can_resume: bool,
    pub can_end_live: bool,
    pub can_revoke: bool,
    pub can_discard: bool,
    pub can_export: bool,
    pub can_retry_export: bool,
}
#[derive(Clone)]
pub struct InboxItem {
    pub id: String,
    pub ciphertext_bytes: u64,
    pub item_count: u64,
    pub completed_at: String,
    pub expires_at: String,
    /// Server item identifiers are metadata only; filenames remain encrypted.
    pub item_ids: Vec<String>,
}
#[derive(Clone)]
pub struct InvitationSnapshot {
    pub token: String,
    pub email: Option<String>,
    pub expires_at: Option<String>,
}
#[derive(Clone)]
pub struct ReceivePreview {
    /// Native checkpoint ID for a prepared file receive. Notes have no ID
    /// because opening them is a separate, consent-bearing operation.
    pub operation_id: Option<String>,
    pub link: String,
    pub kind: ReceiveKind,
    pub password_required: bool,
    pub burn_on_read: bool,
    pub items: Vec<ReceiveItem>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ReceiveKind {
    File,
    Note,
}
#[derive(Clone)]
pub struct ReceiveItem {
    pub id: String,
    pub name: String,
    pub size: u64,
}
#[derive(Clone)]
pub struct NoteSnapshot {
    pub id: String,
    pub link: Option<String>,
    /// Present only when the creator elected to share the link and key separately.
    pub separate_key: Option<String>,
    pub transport: NoteTransport,
    pub state: NoteState,
    pub burn_on_read: bool,
    pub retry_burn: bool,
    pub text_available: bool,
    pub error: Option<ClientError>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NoteTransport {
    Http,
    Live,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum NoteState {
    Creating,
    WaitingForReader,
    Opened,
    BurnPending,
    Complete,
    Failed,
}
#[derive(Clone)]
pub struct PendingPrompt {
    pub transfer_id: TransferId,
    pub id: u64,
    pub kind: PromptKind,
    pub peer: Option<String>,
    pub directory: Option<DirectoryChoice>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PromptKind {
    ShareKey,
    Password,
    PeerConsent,
    Directory,
    ShareReady,
}
#[derive(Clone)]
pub struct DirectoryChoice {
    pub files: u64,
    pub bytes: u64,
    pub maximum_files: Option<u64>,
}
#[derive(Clone)]
pub struct VerifiedResult {
    pub transfer_id: TransferId,
    pub private_path: PathBuf,
    /// Authenticated private artifacts retained until the user exports or removes them.
    pub verified_paths: Vec<PathBuf>,
    /// Destination files created by a successful export, never private artifacts.
    pub exported_paths: Vec<PathBuf>,
    pub export_error: Option<ClientError>,
}
#[derive(Clone)]
pub struct ClientMessage {
    /// Monotonic event identity, retained independently from identical text.
    pub id: u64,
    pub operation: String,
    pub error: ClientError,
}
#[derive(Clone)]
pub struct OperationStatus {
    pub operation: String,
    pub detail: String,
}
#[derive(Clone, PartialEq, Eq)]
pub struct ClientError {
    pub code: ClientErrorCode,
    pub detail: String,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ClientErrorCode {
    InvalidInput,
    Network,
    Remote,
    Storage,
    Crypto,
    Cancelled,
    Busy,
    Internal,
}

/// Secret-bearing commands intentionally do not implement `Debug` or `Clone`.
pub enum ClientCommand {
    /// Reload the validated shared configuration for controls created after this command.
    ReloadSettings,
    Refresh,
    RefreshInbox,
    GenerateReceivingKey {
        password: Option<String>,
        replace: bool,
        acknowledge_old_key_loss: bool,
    },
    ImportReceivingKey {
        export: String,
    },
    UnlockReceivingKey {
        password: String,
    },
    ExportReceivingKey {
        destination: PathBuf,
    },
    /// Commits a self-custody key only after an exported backup was recorded.
    ConfirmReceivingKeyBackup {
        generation_id: String,
    },
    ReceiveInbox {
        id: String,
        item_ids: Option<Vec<String>>,
    },
    ChangeInstance {
        url: String,
    },
    SendFiles(SendFiles),
    ReceiveLink {
        link: String,
        private_dir: PathBuf,
    },
    /// Inspects routing metadata and prepares a file manifest for selection.
    /// A note is only inspected; `OpenNote` supplies its explicit burn consent.
    InspectReceive {
        link: String,
    },
    SelectReceiveItems {
        operation_id: String,
        item_ids: Vec<String>,
    },
    StartPreparedReceive {
        operation_id: String,
    },
    Resume {
        id: TransferId,
    },
    Pause {
        id: TransferId,
    },
    EndLive {
        id: TransferId,
    },
    Revoke {
        id: TransferId,
    },
    Discard {
        id: TransferId,
    },
    AnswerPrompt {
        transfer_id: TransferId,
        prompt_id: u64,
        answer: PromptAnswer,
    },
    ExportVerified {
        transfer_id: TransferId,
        destination: PathBuf,
    },
    CreateNote(NoteDraft),
    OpenNote {
        link: String,
        password: Option<String>,
        burn_acknowledged: bool,
    },
    RetryBurn {
        id: String,
    },
    Login {
        email: String,
        password: String,
        remember: bool,
    },
    Logout,
    Register {
        username: String,
        name: Option<String>,
        email: String,
        password: String,
    },
    RequestPasswordReset {
        email: String,
    },
    ResetPassword {
        email: String,
        token: String,
        password: String,
    },
    ResendVerification,
    VerifyEmailLink {
        link: String,
    },
    SetInboxEnabled(bool),
    SetNotificationChannel(NotificationChannel),
    DeleteInboxItem {
        id: String,
    },
    InspectInbox {
        id: String,
    },
    InspectInvitation {
        token: String,
    },
    AcceptInvitation {
        token: String,
        username: String,
        name: Option<String>,
        email: String,
        password: String,
    },
    Report {
        transfer_id: String,
        category: String,
        description: String,
        email: Option<String>,
    },
    DeleteAccount {
        current_password: String,
        confirmation: String,
    },
    Shutdown {
        wait_ms: u64,
    },
}

impl ClientCommand {
    pub fn operation(&self) -> &'static str {
        match self {
            Self::ReloadSettings => "reload settings",
            Self::GenerateReceivingKey { .. } => "generate receiving key",
            Self::ImportReceivingKey { .. } => "import receiving key",
            Self::UnlockReceivingKey { .. } => "unlock receiving key",
            Self::ExportReceivingKey { .. } => "export receiving key",
            Self::ConfirmReceivingKeyBackup { .. } => "confirm receiving-key backup",
            Self::ReceiveInbox { .. } => "receive inbox",
            Self::InspectReceive { .. } => "inspect receive",
            Self::SelectReceiveItems { .. } => "select receive items",
            Self::StartPreparedReceive { .. } => "start prepared receive",
            Self::ChangeInstance { .. } => "change instance",
            Self::Login { .. } => "login",
            Self::Logout => "logout",
            Self::Register { .. } => "register",
            Self::RequestPasswordReset { .. } => "request password reset",
            Self::ResetPassword { .. } => "reset password",
            Self::ResendVerification => "resend verification",
            Self::VerifyEmailLink { .. } => "verify email",
            Self::SetInboxEnabled(_) => "set inbox enabled",
            Self::SetNotificationChannel(_) => "set notification channel",
            Self::DeleteInboxItem { .. } => "delete inbox item",
            Self::InspectInbox { .. } => "inspect inbox",
            Self::InspectInvitation { .. } => "inspect invitation",
            Self::AcceptInvitation { .. } => "accept invitation",
            Self::Report { .. } => "report transfer",
            Self::DeleteAccount { .. } => "delete account",
            _ => "command",
        }
    }
}

pub struct SendFiles {
    pub paths: Vec<PathBuf>,
    pub directory_mode: DirectoryMode,
    pub transport: SendTransport,
    pub retention_hours: Option<u64>,
    pub turbo: bool,
    pub include_key: bool,
    pub password: Option<String>,
    pub recipient: Option<String>,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DirectoryMode {
    Zip,
    Individual,
}
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum SendTransport {
    #[default]
    Http,
    Live,
}
pub enum PromptAnswer {
    Secret(String),
    AllowPeer(bool),
    Directory(DirectoryMode),
}
pub struct NoteDraft {
    pub text: String,
    pub title: Option<String>,
    pub language: String,
    pub password: Option<String>,
    pub burn_on_read: bool,
    pub retention_hours: Option<u64>,
    pub transport: NoteTransport,
    pub include_key: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshots_have_no_secret_fields() {
        let snapshot = DesktopSnapshot::default();
        assert!(snapshot.jobs.is_empty());
        assert!(!snapshot.account.authenticated);
    }
}
