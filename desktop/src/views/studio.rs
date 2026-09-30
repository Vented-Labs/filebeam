//! Studio layout constants intentionally mirror the approved native design tokens.

pub const SIDEBAR_WIDTH: f32 = 216.0;
pub const INSPECTOR_WIDTH: f32 = 306.0;
pub const INSPECTOR_MIN_WIDTH: f32 = 260.0;
pub const INSPECTOR_MAX_WIDTH: f32 = 420.0;

pub fn clamp_inspector_width(width: f32) -> f32 {
    width.clamp(INSPECTOR_MIN_WIDTH, INSPECTOR_MAX_WIDTH)
}

use crate::theme::Palette;
use gpui::{IntoElement, ParentElement, Styled, div, px};

pub fn unavailable(inbox: bool, palette: Palette) -> impl IntoElement {
    let title = if inbox { "Inbox" } else { "Account" };
    div()
        .flex_1()
        .flex()
        .flex_col()
        .gap(px(20.))
        .child(div().text_size(px(28.)).child(title))
        .child(
            div()
                .p(px(24.))
                .rounded(px(18.))
                .bg(palette.surface)
                .border_1()
                .border_color(palette.border)
                .text_color(palette.muted)
                .child("This workspace is unavailable."),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn studio_inspector_width_is_clamped_to_the_design_range() {
        assert_eq!(clamp_inspector_width(100.0), INSPECTOR_MIN_WIDTH);
        assert_eq!(clamp_inspector_width(306.0), 306.0);
        assert_eq!(clamp_inspector_width(900.0), INSPECTOR_MAX_WIDTH);
    }
}
