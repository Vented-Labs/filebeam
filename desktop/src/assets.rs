//! Embedded product assets: approved icons, bundled fonts and their notices.
//! Iconsax Free artwork and the font licences remain under `assets/NOTICE`.

use std::borrow::Cow;

use gpui::{AssetSource, Result, SharedString};
use gpui_component::Icon;

include!("../assets/native_assets.rs");

/// Checked-in SVG assets exposed to GPUI; no filesystem or network lookup is involved.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(native_asset(path).map(Cow::Borrowed))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(NATIVE_ASSET_PATHS
            .iter()
            .filter(|entry| entry.starts_with(path))
            .map(|entry| (*entry).into())
            .collect())
    }
}

/// The bundled UI and code typefaces. Static instances, because the Linux text
/// system renders a variable font at its default weight only.
pub fn fonts() -> Vec<Cow<'static, [u8]>> {
    macro_rules! font {
        ($name:literal) => {
            Cow::Borrowed(include_bytes!(concat!("../assets/fonts/", $name)) as &'static [u8])
        };
    }
    vec![
        font!("Inter-Regular.ttf"),
        font!("Inter-Medium.ttf"),
        font!("Inter-SemiBold.ttf"),
        font!("Inter-Bold.ttf"),
        font!("InterDisplay-SemiBold.ttf"),
        font!("InterDisplay-Bold.ttf"),
        font!("JetBrainsMono-Regular.ttf"),
        font!("JetBrainsMono-Medium.ttf"),
    ]
}

pub type IconName = &'static str;

pub fn icon(name: &str) -> Icon {
    Icon::default().path(icon_path(name).expect("approved native icon"))
}

/// Aliases required by GPUI Component controls, mapped to approved Iconsax assets.
pub fn component_icon(name: &str) -> Option<Icon> {
    let name = match name {
        "add" => "plus",
        "close" => "x",
        "chevron_down" => "chevron-down",
        "chevron_up" => "chevron-up",
        "chevron_left" => "arrow-left",
        "chevron_right" => "arrow-right",
        "search" | "check" | "trash" => name,
        _ => return None,
    };
    Some(icon(name))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reviewed_icons_are_embedded() {
        let assets = Assets;
        for name in ICON_NAMES {
            let path = icon_path(name).unwrap();
            assert!(assets.load(path).unwrap().is_some(), "missing {name}");
        }
    }

    #[test]
    fn component_controls_use_approved_assets() {
        for name in [
            "add",
            "close",
            "chevron_down",
            "chevron_up",
            "chevron_left",
            "chevron_right",
            "search",
            "check",
            "trash",
        ] {
            assert!(component_icon(name).is_some());
        }
    }
}
