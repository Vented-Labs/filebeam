use crate::{Result, operation};

/// Formats a literal POSIX-shell `beam down` command for a supported share link.
/// Availability remains a transfer-status decision made by the caller.
#[uniffi::export]
pub fn format_download_with_cli(target: String) -> Result<String> {
    filebeam_client_core::link_presentation::format_download_with_cli(&target).map_err(operation)
}
