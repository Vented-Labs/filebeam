//! Private desktop-only layout state. This intentionally never contains drafts,
//! transfer preferences, passwords, or other user content.

use serde::{Deserialize, Serialize};

pub const VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct UiState {
    pub version: u32,
    pub window: Option<WindowGeometry>,
    /// The encoded colour source (`preset:<id>` or `custom:<rrggbb>`). Appearance is
    /// a per-device choice and is deliberately not part of the shared configuration.
    #[serde(default)]
    pub accent: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct WindowGeometry {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub maximized: bool,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            version: VERSION,
            window: None,
            accent: None,
        }
    }
}

impl UiState {
    pub fn normalized(mut self) -> Self {
        if self.version != VERSION {
            return Self::default();
        }
        self.window = self.window.filter(|window| {
            window.width.is_finite()
                && window.height.is_finite()
                && window.width >= 960.0
                && window.height >= 640.0
                && window.x.is_finite()
                && window.y.is_finite()
        });
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undersized_geometry_and_unknown_versions_are_discarded() {
        let small = UiState {
            version: VERSION,
            window: Some(WindowGeometry {
                x: 0.0,
                y: 0.0,
                width: 400.0,
                height: 300.0,
                maximized: false,
            }),
            accent: Some("preset:teal".into()),
        }
        .normalized();
        assert_eq!(small.window, None);
        assert_eq!(small.accent.as_deref(), Some("preset:teal"));
        assert_eq!(
            UiState {
                version: 2,
                window: None,
                accent: Some("preset:teal".into()),
            }
            .normalized(),
            UiState::default()
        );
    }

    #[test]
    fn state_written_by_earlier_versions_still_loads() {
        let state: UiState =
            serde_json::from_str(r#"{"version":1,"inspector_width":306.0,"window":null}"#).unwrap();
        assert_eq!(state.normalized(), UiState::default());
    }
}
