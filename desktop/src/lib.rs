#[cfg(not(feature = "headless-acceptance"))]
pub mod app;
#[cfg(not(feature = "headless-acceptance"))]
pub mod assets;
pub mod client;
#[cfg(any(target_os = "linux", target_os = "macos"))]
pub mod daemon_install;
#[cfg(not(feature = "headless-acceptance"))]
pub mod live;
pub mod model;
#[cfg(not(feature = "headless-acceptance"))]
pub mod platform;
#[cfg(not(feature = "headless-acceptance"))]
pub mod shell;
#[cfg(all(test, not(feature = "headless-acceptance")))]
mod testing;
#[cfg(not(feature = "headless-acceptance"))]
pub mod theme;
#[cfg(not(feature = "headless-acceptance"))]
pub mod toast;
#[cfg(not(feature = "headless-acceptance"))]
pub mod ui;
#[cfg(not(feature = "headless-acceptance"))]
pub mod ui_state;
#[cfg(not(feature = "headless-acceptance"))]
pub mod views;
#[cfg(feature = "visual-test")]
pub mod visual;

pub const APP_NAME: &str = "Filebeam";
pub const VERSION: &str = env!("FILEBEAM_VERSION");
pub const RELEASE_TAG: &str = env!("FILEBEAM_RELEASE_TAG");
pub const RELEASE_SHA: &str = env!("FILEBEAM_RELEASE_SHA");
