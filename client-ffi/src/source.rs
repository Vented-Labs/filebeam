//! Android descriptor callback. `open` returns a detached dup-fd owned by Rust.

use crate::{ClientError, operation};
use filebeam_client_core::control::TransferSettings;
use filebeam_transfer_native::source::SourceResolver;
use std::{fs::File, sync::Arc};

#[uniffi::export(foreign)]
pub trait SourceCallback: Send + Sync {
    /// Return an owned duplicated Unix FD, or a typed host error. Kotlin must
    /// use `ParcelFileDescriptor.dup(...).detachFd()` for this value.
    fn open(&self, identity: String) -> std::result::Result<i64, ClientError>;
    fn mutation_token(&self, identity: String) -> std::result::Result<String, ClientError>;
}

struct HostSource(Arc<dyn SourceCallback>);

impl SourceResolver for HostSource {
    fn open(&self, identity: &str) -> anyhow::Result<File> {
        let fd = self.0.open(identity.into()).map_err(operation)?;
        #[cfg(unix)]
        {
            use std::os::fd::FromRawFd;
            let fd = i32::try_from(fd)
                .map_err(|_| anyhow::anyhow!("host returned an invalid descriptor"))?;
            if fd < 0 {
                return Err(anyhow::anyhow!("host returned an invalid descriptor"));
            }
            // The callback contract transfers ownership of exactly this FD.
            Ok(unsafe { File::from_raw_fd(fd) })
        }
        #[cfg(not(unix))]
        {
            let _ = fd;
            Err(anyhow::anyhow!(
                "provider descriptors are unsupported on this platform"
            ))
        }
    }

    fn mutation_token(&self, identity: &str) -> anyhow::Result<String> {
        Ok(self.0.mutation_token(identity.into()).map_err(operation)?)
    }
}

pub fn install(settings: &mut TransferSettings, callback: Arc<dyn SourceCallback>) {
    settings.source_resolver = Some(Arc::new(HostSource(callback)));
}
