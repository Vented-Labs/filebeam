//! Values copied exactly from the immutable Studio token contract.

pub mod geometry {
    pub const TITLEBAR_HEIGHT: f32 = 42.0;
    pub const STUDIO_SIDEBAR: f32 = 216.0;
    pub const WORKBENCH_RAIL: f32 = 86.0;
    pub const STUDIO_INSPECTOR: f32 = 306.0;
    pub const WORKBENCH_INSPECTOR: f32 = 330.0;
    pub const INSPECTOR_MIN: f32 = 260.0;
    pub const INSPECTOR_MAX: f32 = 420.0;
    pub const DIVIDER_HIT: f32 = 7.0;
    pub const CONTENT_MAX: f32 = 1536.0;
    pub const STUDIO_RADIUS: f32 = 18.0;
    pub const WORKBENCH_RADIUS: f32 = 11.0;
    pub const CONTROL_RADIUS: f32 = 9.0;
    pub const CONTROL_HEIGHT: f32 = 36.0;
    pub const LARGE_CONTROL_HEIGHT: f32 = 43.0;
    pub const WORKSPACE_PADDING: f32 = 36.0;
    /// Page-owned scroll content uses this on both ends; the shell owns no vertical inset.
    pub const WORKSPACE_VERTICAL_PADDING: f32 = 31.0;
    pub const STATUSBAR_HEIGHT: f32 = 29.0;
}

pub mod motion {
    use std::time::Duration;

    pub const CONTROLS_MS: u64 = 180;
    pub const SWITCH_MS: u64 = 280;
    pub const SELECTION_MS: u64 = 340;
    pub const FILE_ROW_MS: u64 = 200;
    pub const PANEL_MS: u64 = 330;
    pub const MODAL_ENTER_MS: u64 = 340;
    pub const MODAL_EXIT_MS: u64 = 200;
    pub const MODAL_CONTENT_MS: u64 = 320;
    pub const TOAST_ENTER_MS: u64 = 230;
    pub const TOAST_EXIT_MS: u64 = 180;
    pub const EASING: [f32; 4] = [0.22, 1.0, 0.36, 1.0];

    /// Platform adapters pass the OS setting explicitly so it always overrides configuration.
    pub fn duration(
        milliseconds: u64,
        os_reduced_motion: bool,
        configured_reduced_motion: bool,
    ) -> Duration {
        if os_reduced_motion || configured_reduced_motion {
            Duration::ZERO
        } else {
            Duration::from_millis(milliseconds)
        }
    }
}
