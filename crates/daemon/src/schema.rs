use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Local IPC protocol version. The signed release catalog advertises the
/// supported range for each daemon component.
pub const IPC_VERSION: u32 = 1;
/// Version of the daemon's persisted profile state. A component is installed
/// only when it matches the schema the requesting client expects.
pub const STORAGE_SCHEMA: u32 = 1;

/// Reply error for requests that need this profile's device registration.
pub const PROVISIONING_REQUIRED: &str = "device provisioning is required";

/// The profile's daemon endpoint does not exist or refuses connections.
/// Clients match it with [`is_not_running`] to offer setup guidance.
#[derive(Debug)]
pub struct DaemonNotRunning {
    pub endpoint: PathBuf,
}

impl std::fmt::Display for DaemonNotRunning {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "the Filebeam daemon is not running (no endpoint at {})",
            self.endpoint.display()
        )
    }
}

impl std::error::Error for DaemonNotRunning {}

pub fn is_not_running(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| cause.is::<DaemonNotRunning>())
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalMode {
    Ask,
    Auto,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case", tag = "platform")]
pub enum RootIdentity {
    Unix {
        device: u64,
        inode: u64,
    },
    Windows {
        volume_serial: u64,
        file_id: [u8; 16],
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DestinationGrant {
    pub id: Uuid,
    pub alias: String,
    pub revision: u64,
    pub root_identity: RootIdentity,
    pub approval: ApprovalMode,
    pub enabled: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeviceJobState {
    AwaitingPeerConfiguration,
    Preparing,
    Connecting,
    Transferring,
    Delivered,
    Failed,
    Paused,
    Cancelled,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceJob {
    pub id: Uuid,
    pub target: String,
    pub grant_id: String,
    pub sources: Vec<PathBuf>,
    pub state: DeviceJobState,
    pub prepared: bool,
    #[serde(default)]
    pub expires_at_unix_seconds: u64,
    /// Why the job failed, or the last error while it waits to be retried.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeviceProvision {
    pub instance: String,
    pub deployment: String,
    pub account_id: u64,
    pub generation: u64,
    pub device_id: String,
    pub key_version: u32,
    pub public_key: String,
    pub signing_key: String,
    pub credential: String,
}

/// Enrollment data printed by a device that is waiting for approval. Approval
/// signs it with this daemon's device key after the user compares the
/// candidate's fingerprint out of band.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceApproval {
    pub instance_origin: String,
    pub deployment_id: String,
    pub account_id: u64,
    pub generation: u64,
    pub candidate_device_id: String,
    pub candidate_key_version: u32,
    pub candidate_public_key: String,
    pub challenge_sha256: String,
    pub expires_at_unix: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DaemonDevice {
    pub id: String,
    pub display_name: String,
    pub key_version: u32,
    pub status: String,
    pub fingerprint: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DaemonState {
    pub receive_enabled: bool,
    #[serde(default)]
    pub lan_enabled: bool,
    pub runtime: RuntimeAvailability,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeAvailability {
    AwaitingPeerConfiguration,
    Provisioned,
}

impl Default for DaemonState {
    fn default() -> Self {
        Self {
            receive_enabled: false,
            lan_enabled: false,
            runtime: RuntimeAvailability::AwaitingPeerConfiguration,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub enum Request {
    Status {
        version: u32,
    },
    SetReceiveEnabled {
        version: u32,
        enabled: bool,
    },
    SetLanEnabled {
        version: u32,
        enabled: bool,
    },
    ListDestinations {
        version: u32,
    },
    AddDestination {
        version: u32,
        alias: String,
        path: PathBuf,
        approval: ApprovalMode,
    },
    RevokeDestination {
        version: u32,
        alias_or_id: String,
    },
    ListJobs {
        version: u32,
    },
    EnqueueJob {
        version: u32,
        target: String,
        grant_id: String,
        sources: Vec<PathBuf>,
    },
    ProvisionDevice {
        version: u32,
        provision: DeviceProvision,
    },
    ListDevices {
        version: u32,
    },
    ConfirmPeerVerification {
        version: u32,
        device_id: String,
        key_version: u32,
        fingerprint: String,
        confirmation_nonce: String,
    },
    ControlJob {
        version: u32,
        id: Uuid,
        action: DeviceJobState,
    },
    ApproveDevice {
        version: u32,
        approval: DeviceApproval,
        fingerprint: String,
    },
    Shutdown {
        version: u32,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Reply {
    pub version: u32,
    pub state: DaemonState,
    pub error: Option<String>,
    pub destinations: Vec<DestinationGrant>,
    pub jobs: Vec<DeviceJob>,
    pub devices: Vec<DaemonDevice>,
}

#[cfg(test)]
mod tests {
    #[test]
    fn release_catalog_advertises_this_daemon_contract() {
        let writer = include_str!("../../../scripts/release/cli-write-release.php");
        assert!(writer.contains(&format!(
            "const DAEMON_IPC_VERSION = {};",
            super::IPC_VERSION
        )));
        assert!(writer.contains(&format!(
            "const DAEMON_STORAGE_SCHEMA = {};",
            super::STORAGE_SCHEMA
        )));
    }
}
