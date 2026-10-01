//! Atomic persistence for the desktop's non-sensitive layout geometry.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::ui_state::UiState;

fn path(home: &Path) -> PathBuf {
    home.join("private").join("desktop-ui-state.json")
}

pub fn load(home: &Path) -> UiState {
    fs::read(path(home))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<UiState>(&bytes).ok())
        .map(UiState::normalized)
        .unwrap_or_default()
}

pub fn save(home: &Path, state: &UiState) -> io::Result<()> {
    let path = path(home);
    let parent = path.parent().expect("private state has a parent");
    fs::create_dir_all(parent)?;
    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec(&state.clone().normalized()).map_err(io::Error::other)?;
    fs::write(&temporary, bytes)?;
    fs::rename(temporary, path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui_state::{UiState, WindowGeometry};

    #[test]
    fn persistence_round_trip_is_private_and_geometry_only() {
        let temp = tempfile::tempdir().unwrap();
        let state = UiState {
            version: 1,
            inspector_width: 400.0,
            window: Some(WindowGeometry {
                x: 1.0,
                y: 2.0,
                width: 1200.0,
                height: 800.0,
                maximized: false,
            }),
        };
        save(temp.path(), &state).unwrap();
        assert_eq!(load(temp.path()), state);
        assert!(temp.path().join("private/desktop-ui-state.json").is_file());
    }
}
