#[derive(Clone, uniffi::Record)]
pub struct ClientConfig {
    pub state_directory: String,
    pub memory_budget_mib: u32,
    pub max_concurrency: u32,
    pub relay_only: bool,
    /// Development builds may connect to local HTTP instances.
    pub allow_http: bool,
}

#[derive(Clone, Copy, uniffi::Enum)]
pub enum Transport {
    Http,
    WebRtc,
}

/// Additive upload configuration. `start_upload` retains its historic defaults.
#[derive(Clone, uniffi::Record)]
pub struct UploadOptions {
    pub transport: Transport,
    pub turbo: bool,
    pub archive: bool,
    pub password: bool,
    pub retention_hours: Option<u64>,
    pub authentication: UploadAuthentication,
    pub recipient: Option<UploadRecipient>,
}

#[derive(Clone, uniffi::Record)]
pub struct UploadAuthentication {
    pub bearer_token: Option<String>,
    pub session_cookie: Option<String>,
}

#[derive(Clone, uniffi::Record)]
pub struct UploadRecipient {
    pub username: String,
    pub user_id: u64,
    pub account_key_bundle_id: u64,
    pub public_key: String,
}

#[derive(Clone, Copy, uniffi::Enum)]
pub enum SourceKind {
    Path,
    Provider,
}

/// A bounded source. Provider identities are opaque to Rust and require a
/// `SourceCallback` installed on the owning `TransferClient`.
#[derive(Clone, uniffi::Record)]
pub struct UploadSource {
    pub kind: SourceKind,
    pub name: String,
    pub path_or_identity: String,
    pub offset: u64,
    pub length: u64,
    pub mutation_token: Option<String>,
}

#[derive(Clone, uniffi::Record)]
pub struct InstanceInfo {
    pub name: String,
    pub anonymous_uploads: bool,
    pub enabled_transports: Vec<String>,
    pub maximum_transfer_bytes: Option<u64>,
    pub maximum_file_count: Option<u64>,
    pub retention_hours: u64,
    pub webrtc_maximum_transfer_bytes: Option<u64>,
    pub webrtc_maximum_file_count: Option<u64>,
    pub retention_options_hours: Vec<u64>,
    pub drivers: Vec<DriverLimit>,
}

#[derive(Clone, uniffi::Record)]
pub struct DriverLimit {
    pub driver: String,
    pub maximum_transfer_bytes: Option<u64>,
    pub maximum_file_count: Option<u64>,
}

#[derive(Clone, uniffi::Record)]
pub struct ShareLinkPresentation {
    pub link: String,
    pub separate_key: String,
}

#[derive(Clone, uniffi::Record)]
pub struct SavedTransfer {
    pub id: String,
    pub direction: String,
    pub state: String,
    pub done: u64,
    pub total: u64,
}

#[derive(Clone, uniffi::Record)]
pub struct SavedTransferDetails {
    pub id: String,
    pub direction: String,
    pub kind: String,
    pub transport: String,
    pub state: String,
    pub done: u64,
    pub total: u64,
    pub verified_privately: bool,
    pub exported: bool,
    pub expires_at: Option<String>,
    pub can_resume: bool,
    pub can_retry_save: bool,
    pub can_end_live: bool,
    pub can_revoke_remote: bool,
    pub can_remove_local: bool,
}

#[derive(Clone, Copy, uniffi::Enum)]
pub enum JobState {
    Running,
    Pausing,
    Paused,
    Complete,
    Failed,
}

#[derive(Clone, Copy, uniffi::Enum)]
pub enum ErrorCategory {
    Cancelled,
    ResourceExhausted,
    InvalidInput,
    Network,
    Remote,
    Storage,
    Crypto,
    Internal,
}

#[derive(Clone, Copy, uniffi::Enum)]
pub enum PromptType {
    ShareKey,
    Password,
    PeerConsent,
    Directory,
    ShareReady,
}

#[derive(Clone, uniffi::Record)]
pub struct PendingPrompt {
    pub id: u64,
    pub kind: PromptType,
    pub peer: Option<String>,
    pub directory: Option<DirectoryPrompt>,
}

#[derive(Clone, uniffi::Record)]
pub struct DirectoryPrompt {
    pub files: u64,
    pub bytes: u64,
    pub maximum_files: Option<u64>,
}

#[derive(Clone, Copy, uniffi::Enum)]
pub enum DirectoryChoice {
    Zip,
    IndividualFiles,
}

#[derive(Clone, Copy, uniffi::Enum)]
pub enum SecretRetryKind {
    ShareKey,
    Password,
    Generic,
}

#[derive(Clone, uniffi::Record)]
pub struct TransferSnapshot {
    pub state: JobState,
    pub checkpoint_id: Option<String>,
    pub phase: String,
    pub file_name: String,
    pub file_index: u64,
    pub file_count: u64,
    pub total: Option<u64>,
    pub done: u64,
    pub committed: u64,
    pub wire_bytes: u64,
    pub prompt: Option<PendingPrompt>,
    pub share_url: Option<String>,
    pub results: Vec<String>,
    pub error: Option<String>,
    pub error_category: Option<ErrorCategory>,
    pub peer_warning: Option<String>,
    pub secret_retry: Option<SecretRetryKind>,
}
