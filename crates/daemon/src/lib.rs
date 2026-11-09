mod control;
mod dispatcher;
#[cfg(unix)]
mod grants;
#[cfg(windows)]
#[path = "windows_grants.rs"]
mod grants;
mod jobs;
#[cfg(unix)]
mod lan_runtime;
#[cfg(unix)]
mod receiver;
#[cfg(unix)]
mod unix;

pub use jobs::DeviceJobs;
#[cfg(unix)]
pub use unix::*;

pub(crate) use control::ControlPlane;
pub(crate) use dispatcher::ActiveJob;

pub(crate) use control::{daemon_store, registry_scope, registry_store, write_json};
#[cfg(windows)]
pub(crate) use control::{read_state, write_state};
#[cfg(unix)]
pub(crate) use unix::{provision_signing_key, secure_directory, unix_now_i64};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
mod windows_main;

#[cfg(windows)]
pub(crate) use grants::DestinationGrants;
#[cfg(windows)]
pub(crate) use windows::{
    DaemonPeerReceiver, PlatformRoot, SharedPeerVerification, StagedObjectIdentity,
    WindowsReceiverRoot,
};

#[cfg(windows)]
pub use windows_main::*;
mod schema;
#[cfg(windows)]
pub use schema::*;
