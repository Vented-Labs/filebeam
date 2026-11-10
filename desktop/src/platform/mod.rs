pub mod ingress;
pub mod lifecycle;
pub mod notifications;
pub mod preferences;
pub mod tray;

/// Where a save dialog starts: the person's home folder, never Filebeam's own state.
pub fn user_directory() -> std::path::PathBuf {
    std::env::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/"))
}
