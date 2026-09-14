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
}

#[derive(Clone, uniffi::Record)]
pub struct SavedTransfer {
    pub id: String,
    pub direction: String,
    pub state: String,
    pub done: u64,
    pub total: u64,
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
}
