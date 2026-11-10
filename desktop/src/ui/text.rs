//! The type scale. Every label in the app is built from one of these styles so
//! sizes, weights and leading stay consistent.

use gpui::{Div, FontWeight, ParentElement, SharedString, Styled, div, px, relative};

use crate::theme::tokens::text as size;

/// The UI typeface; registered at startup with a system fallback.
pub const SANS: &str = "Inter";
/// The display cut of the UI typeface, for large headings.
pub const DISPLAY: &str = "Inter Display";
pub const MONO: &str = "JetBrains Mono";

fn styled(text: impl Into<SharedString>, pixels: f32, weight: FontWeight, leading: f32) -> Div {
    div()
        .text_size(px(pixels))
        .font_weight(weight)
        .line_height(relative(leading))
        .child(text.into())
}

/// Hero statements such as "Drop your files here".
pub fn display(text: impl Into<SharedString>) -> Div {
    styled(text, size::DISPLAY, FontWeight::BOLD, 1.12).font_family(DISPLAY)
}

/// Page titles.
pub fn headline(text: impl Into<SharedString>) -> Div {
    styled(text, size::HEADLINE, FontWeight::BOLD, 1.18).font_family(DISPLAY)
}

/// Panel and dialog titles.
pub fn title(text: impl Into<SharedString>) -> Div {
    styled(text, size::TITLE, FontWeight::SEMIBOLD, 1.25).font_family(DISPLAY)
}

/// Section and card headings.
pub fn subtitle(text: impl Into<SharedString>) -> Div {
    styled(text, size::SUBTITLE, FontWeight::SEMIBOLD, 1.3)
}

pub fn body(text: impl Into<SharedString>) -> Div {
    styled(text, size::BODY, FontWeight::NORMAL, 1.5)
}

pub fn body_strong(text: impl Into<SharedString>) -> Div {
    styled(text, size::BODY, FontWeight::SEMIBOLD, 1.4)
}

pub fn small(text: impl Into<SharedString>) -> Div {
    styled(text, size::SMALL, FontWeight::NORMAL, 1.45)
}

pub fn small_strong(text: impl Into<SharedString>) -> Div {
    styled(text, size::SMALL, FontWeight::SEMIBOLD, 1.35)
}

pub fn caption(text: impl Into<SharedString>) -> Div {
    styled(text, size::CAPTION, FontWeight::MEDIUM, 1.4)
}

/// Small uppercase section markers, e.g. "ON THIS DEVICE".
pub fn eyebrow(text: impl Into<SharedString>) -> Div {
    let text: SharedString = text.into();
    styled(text.to_uppercase(), size::CAPTION, FontWeight::BOLD, 1.3)
}

/// Links, identifiers and commands.
pub fn code(text: impl Into<SharedString>) -> Div {
    styled(text, size::SMALL, FontWeight::NORMAL, 1.5).font_family(MONO)
}

/// One line that truncates with an ellipsis instead of wrapping.
pub trait TextExt: Styled + Sized {
    fn single_line(self) -> Self {
        self.min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
    }
}

impl<T: Styled> TextExt for T {}
