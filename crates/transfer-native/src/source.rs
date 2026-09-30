//! Platform-neutral upload source descriptions and owned descriptor handles.
//!
//! Protocol checkpoints store `SourceSpec`; platform handles stay local and are
//! duplicated for each read so no asynchronous task borrows a provider FD.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceSpec {
    Path {
        path: PathBuf,
        offset: u64,
        length: u64,
    },
    Provider {
        /// Opaque platform identity, never interpreted by the native core.
        identity: String,
        offset: u64,
        length: u64,
        /// Provider-supplied revision/metadata token captured at selection.
        mutation_token: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UploadSource {
    pub name: String,
    pub spec: SourceSpec,
}

impl SourceSpec {
    pub fn bounds(&self) -> (u64, u64) {
        match self {
            Self::Path { offset, length, .. } | Self::Provider { offset, length, .. } => {
                (*offset, *length)
            }
        }
    }

    pub fn checked_end(&self) -> Result<u64> {
        let (offset, length) = self.bounds();
        offset
            .checked_add(length)
            .context("source offset and length overflow")
    }
}

/// An owned, seekable source. `Provider` always owns its descriptor; callers
/// must duplicate a Java `ParcelFileDescriptor` before constructing it.
pub enum SourceHandle {
    Path(PathBuf),
    Provider(ProviderDescriptor),
}

pub struct ProviderDescriptor {
    identity: String,
    file: File,
}

/// Platform seam for reopening a provider after process death. Implementations
/// return an owned duplicate; native closes it after the read completes.
pub trait SourceResolver: Send + Sync {
    fn open(&self, identity: &str) -> Result<File>;
    fn mutation_token(&self, identity: &str) -> Result<String>;
}

impl ProviderDescriptor {
    pub fn from_owned_file(identity: String, file: File) -> Self {
        Self { identity, file }
    }

    #[cfg(unix)]
    /// Takes ownership of `fd`.
    ///
    /// # Safety
    /// `fd` must be a valid, uniquely owned open file descriptor. The caller
    /// must not close or otherwise transfer it after this call.
    pub unsafe fn from_owned_fd(identity: String, fd: std::os::fd::RawFd) -> Self {
        use std::os::fd::FromRawFd;
        Self::from_owned_file(identity, unsafe { File::from_raw_fd(fd) })
    }

    pub fn identity(&self) -> &str {
        &self.identity
    }

    /// Produces a separately owned descriptor for one blocking read operation.
    pub fn duplicate(&self) -> io::Result<File> {
        self.file.try_clone()
    }
}

impl SourceHandle {
    pub fn reopen(&self) -> Result<File> {
        match self {
            Self::Path(path) => {
                File::open(path).with_context(|| format!("open {}", path.display()))
            }
            Self::Provider(provider) => provider
                .duplicate()
                .context("duplicate provider descriptor"),
        }
    }

    pub fn read_range(
        &self,
        spec: &SourceSpec,
        relative_offset: u64,
        maximum: usize,
    ) -> Result<Vec<u8>> {
        spec.checked_end()?;
        let (_, length) = spec.bounds();
        if relative_offset > length {
            bail!("source read starts beyond its declared length");
        }
        let absolute = spec
            .bounds()
            .0
            .checked_add(relative_offset)
            .context("source read offset overflow")?;
        let wanted = (length - relative_offset).min(maximum as u64) as usize;
        let mut file = self.reopen()?;
        file.seek(SeekFrom::Start(absolute))
            .context("seek source descriptor")?;
        let mut result = vec![0; wanted];
        file.read_exact(&mut result)
            .context("source was truncated or revoked")?;
        Ok(result)
    }

    pub fn check_mutation(&self, spec: &SourceSpec, current_token: Option<&str>) -> Result<()> {
        if let SourceSpec::Provider {
            identity,
            mutation_token,
            ..
        } = spec
        {
            let provider = match self {
                Self::Provider(provider) => provider,
                Self::Path(_) => bail!("provider source requires an owned provider descriptor"),
            };
            if provider.identity() != identity || current_token != Some(mutation_token) {
                bail!("provider source changed or its grant was revoked; start a new upload");
            }
        }
        Ok(())
    }
}

pub fn open(spec: &SourceSpec, resolver: Option<&dyn SourceResolver>) -> Result<File> {
    match spec {
        SourceSpec::Path { path, .. } => {
            File::open(path).with_context(|| format!("open {}", path.display()))
        }
        SourceSpec::Provider {
            identity,
            mutation_token,
            ..
        } => {
            let resolver =
                resolver.context("saved provider source cannot be reopened on this host")?;
            if resolver.mutation_token(identity)? != *mutation_token {
                bail!("provider source changed or its grant was revoked; start a new upload");
            }
            resolver
                .open(identity)
                .with_context(|| format!("reopen provider source {identity}"))
        }
    }
}

/// Copies a transient/non-seekable source into `directory` without retaining a
/// partial file. `maximum_bytes` is enforced before every write.
pub fn snapshot_non_seekable(
    mut input: impl Read,
    directory: &Path,
    name: &str,
    maximum_bytes: u64,
) -> Result<(SourceSpec, SourceHandle)> {
    fs::create_dir_all(directory).context("create source snapshot directory")?;
    let safe_name = Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("source");
    let final_path = directory.join(format!("{}-{}", Uuid::new_v4(), safe_name));
    let partial = directory.join(format!(".{}-{}.part", Uuid::new_v4(), safe_name));
    let result = (|| -> Result<u64> {
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&partial)?;
        let mut total = 0u64;
        let mut buffer = [0u8; 64 * 1024];
        loop {
            let read = input.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            total = total
                .checked_add(read as u64)
                .context("snapshot is too large")?;
            if total > maximum_bytes {
                bail!("source exceeds the snapshot byte limit");
            }
            output.write_all(&buffer[..read])?;
        }
        output.sync_all()?;
        Ok(total)
    })();
    match result {
        Ok(length) => {
            fs::rename(&partial, &final_path).context("finish source snapshot")?;
            Ok((
                SourceSpec::Path {
                    path: final_path.clone(),
                    offset: 0,
                    length,
                },
                SourceHandle::Path(final_path),
            ))
        }
        Err(error) => {
            let _ = fs::remove_file(partial);
            Err(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Cursor, sync::Mutex};

    struct TestResolver {
        path: PathBuf,
        token: Mutex<String>,
    }
    impl SourceResolver for TestResolver {
        fn open(&self, identity: &str) -> Result<File> {
            if identity != "content://provider/document/1" {
                bail!("revoked provider grant");
            }
            Ok(File::open(&self.path)?)
        }
        fn mutation_token(&self, _: &str) -> Result<String> {
            Ok(self.token.lock().unwrap().clone())
        }
    }

    #[test]
    fn bounded_large_offsets_do_not_wrap() {
        let spec = SourceSpec::Path {
            path: "/tmp/source".into(),
            offset: u64::MAX - 2,
            length: 3,
        };
        assert!(spec.checked_end().is_err());
    }

    #[test]
    fn non_seekable_snapshot_is_bounded_and_removes_partial_output() {
        let directory = tempfile::tempdir().unwrap();
        assert!(snapshot_non_seekable(Cursor::new(vec![9; 9]), directory.path(), "x", 8).is_err());
        assert!(fs::read_dir(directory.path()).unwrap().next().is_none());
    }

    #[test]
    fn provider_mutation_and_identity_are_checked() {
        let file = tempfile::tempfile().unwrap();
        let handle = SourceHandle::Provider(ProviderDescriptor::from_owned_file(
            "content://one".into(),
            file,
        ));
        let spec = SourceSpec::Provider {
            identity: "content://one".into(),
            offset: 0,
            length: 0,
            mutation_token: "7".into(),
        };
        assert!(handle.check_mutation(&spec, Some("7")).is_ok());
        assert!(handle.check_mutation(&spec, Some("8")).is_err());
    }

    #[test]
    fn provider_can_reopen_after_process_death_but_rejects_revocation_or_mutation() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("provider.bin");
        fs::write(&path, b"provider bytes").unwrap();
        let resolver = TestResolver {
            path,
            token: Mutex::new("v1".into()),
        };
        let spec = SourceSpec::Provider {
            identity: "content://provider/document/1".into(),
            offset: 0,
            length: 14,
            mutation_token: "v1".into(),
        };
        assert_eq!(
            open(&spec, Some(&resolver))
                .unwrap()
                .metadata()
                .unwrap()
                .len(),
            14
        );
        *resolver.token.lock().unwrap() = "v2".into();
        assert!(open(&spec, Some(&resolver)).is_err());
        assert!(open(&spec, None).is_err());
    }
}
