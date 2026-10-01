//! Embedded product assets. Iconsax Free artwork remains under `assets/NOTICE`.

use std::{
    borrow::Cow,
    sync::{Arc, LazyLock},
};

use gpui::{
    AssetSource, Img, InteractiveElement, RenderImage, Result, SharedString, Stateful, img,
};
use gpui_component::Icon;

include!("../assets/native_assets.rs");

pub const FOLDED_F_MARK: &[u8] = include_bytes!("../assets/filebeam-mark.svg");
static FOLDED_F_MARK_RENDER: LazyLock<Arc<RenderImage>> = LazyLock::new(|| {
    let mut image = image::load_from_memory(include_bytes!("../assets/brand/filebeam-mark.png"))
        .expect("bundled Filebeam mark must decode")
        .to_rgba8();
    // GPUI's image renderer consumes BGRA pixels.
    let pixels: &mut [u8] = image.as_mut();
    for [red, _, blue, _] in pixels.as_chunks_mut::<4>().0 {
        std::mem::swap(red, blue);
    }
    Arc::new(RenderImage::new(vec![image::Frame::new(image)]))
});

/// Checked-in SVG assets exposed to GPUI; no filesystem or network lookup is involved.
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(match path {
            "brand/filebeam-mark.png" => Some(Cow::Borrowed(include_bytes!(
                "../assets/brand/filebeam-mark.png"
            ))),
            _ => native_asset(path).map(Cow::Borrowed),
        })
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(NATIVE_ASSET_PATHS
            .iter()
            .filter(|entry| entry.starts_with(path))
            .map(|entry| (*entry).into())
            .collect())
    }
}

pub type IconName = &'static str;

pub fn icon(name: &str) -> Icon {
    Icon::default().path(icon_path(name).expect("approved native icon"))
}

/// GPUI SVG elements are alpha masks; retain a decoded transparent raster to
/// preserve the approved gradients across native view rebuilds.
pub fn mark() -> Stateful<Img> {
    // Img retains asynchronous decode state only when it has a global element ID.
    img(FOLDED_F_MARK_RENDER.clone()).id("filebeam-mark")
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
