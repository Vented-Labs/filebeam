use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

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
