//! Receiver-facing, handle-only operations for a Windows destination root.

use std::{fs::File, path::Path};

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::{ConfinedRoot, FileIdentity};

/// Durable object identity recorded before publication.  It lets recovery
/// distinguish an idempotent completed rename from a substituted object.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub(crate) struct StagedObjectIdentity {
    pub(crate) volume_serial: u64,
    pub(crate) file_id: [u8; 16],
}

impl From<FileIdentity> for StagedObjectIdentity {
    fn from(value: FileIdentity) -> Self {
        Self {
            volume_serial: value.volume_serial,
            file_id: value.file_id,
        }
    }
}

impl From<StagedObjectIdentity> for FileIdentity {
    fn from(value: StagedObjectIdentity) -> Self {
        Self {
            volume_serial: value.volume_serial,
            file_id: value.file_id,
        }
    }
}

/// The platform-neutral receiver can use this narrow surface rather than
/// reconstructing a destination path from a grant.
pub(crate) struct PlatformRoot(ConfinedRoot);

impl PlatformRoot {
    pub(crate) fn validate_component(name: &str) -> Result<()> {
        super::validate_component_name(name)
    }

    pub(crate) fn open(path: &Path) -> Result<Self> {
        Ok(Self(ConfinedRoot::open(path)?))
    }

    pub(crate) fn identity(&self) -> (u64, [u8; 16]) {
        self.0.identity()
    }

    pub(crate) fn try_clone(&self) -> Result<Self> {
        Ok(Self(self.0.try_clone()?))
    }

    pub(crate) fn create_stage(&self, name: &str) -> Result<Self> {
        Ok(Self(self.0.create_directory_root(name)?))
    }

    pub(crate) fn open_stage(&self, name: &str) -> Result<Self> {
        Ok(Self(self.0.open_directory_root(name)?))
    }

    pub(crate) fn stage_subdirectory(&self, components: &[String]) -> Result<Self> {
        Ok(Self(self.0.open_or_create_subdirectory(components)?))
    }

    pub(crate) fn stage_child(&self, name: &str) -> Result<Self> {
        Ok(Self(self.0.open_or_create_child(name)?))
    }

    /// Safe only while this root and every ancestor guard remain open.
    pub(crate) fn tethered_path(&self) -> Result<std::path::PathBuf> {
        self.0.tethered_path()
    }

    pub(crate) fn sync(&self) -> Result<()> {
        self.0.handle.sync_all().map_err(Into::into)
    }

    pub(crate) fn create_target(&self, relative: &Path) -> Result<File> {
        self.0.create_new(relative)
    }

    pub(crate) fn stage_identity(&self, name: &str) -> Result<Option<StagedObjectIdentity>> {
        self.0
            .child_identity(&self.0.handle, name)
            .map(|identity| identity.map(Into::into))
    }

    pub(crate) fn publish_stage(
        &self,
        stage: &PlatformRoot,
        source_name: &str,
        destination_name: &str,
        identity: &StagedObjectIdentity,
    ) -> Result<()> {
        self.0.rename_from_no_replace(
            &stage.0.handle,
            source_name,
            destination_name,
            identity.clone().into(),
        )
    }

    pub(crate) fn remove_stage(&self, name: &str) -> Result<()> {
        self.0.remove_directory(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_stage_identity_preserves_the_volume_and_file_id() {
        let identity = StagedObjectIdentity {
            volume_serial: 17,
            file_id: [9; 16],
        };
        let encoded = serde_json::to_vec(&identity).unwrap();
        assert_eq!(
            serde_json::from_slice::<StagedObjectIdentity>(&encoded).unwrap(),
            identity
        );
    }
}
