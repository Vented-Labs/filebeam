//! Private desktop-only layout state. This intentionally never contains drafts,
//! transfer preferences, passwords, or other user content.

use serde::{Deserialize, Serialize};

pub const VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct UiState {
    pub version: u32,
    pub inspector_width: f32,
    pub window: Option<WindowGeometry>,
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
            inspector_width: 306.0,
            window: None,
        }
    }
}

impl UiState {
    pub fn normalized(mut self) -> Self {
        if self.version != VERSION {
            return Self::default();
        }
        self.inspector_width = self.inspector_width.clamp(260.0, 420.0);
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
    fn geometry_is_clamped_and_unknown_versions_are_discarded() {
        assert_eq!(
            UiState {
                version: VERSION,
                inspector_width: 999.0,
                window: None
            }
            .normalized()
            .inspector_width,
            420.0
        );
        assert_eq!(
            UiState {
                version: 2,
                inspector_width: 300.0,
                window: None
            }
            .normalized(),
            UiState::default()
        );
    }
}
