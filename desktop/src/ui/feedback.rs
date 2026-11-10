//! Status and feedback: tones, pills, inline notices, empty states and the spinner.

use gpui::{
    AnyElement, App, Div, FontWeight, Hsla, IntoElement, ParentElement, SharedString, Styled,
    Window, div, px, relative,
};
use gpui_component::{Sizable, Size, spinner::Spinner};

use crate::{
    assets::icon,
    theme::{
        Palette, Status,
        tokens::{radius, text as size},
    },
    ui::{
        motion::{self, MotionExt},
        surface, text,
    },
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Accent,
    Success,
    Warning,
    Danger,
    Info,
}

impl Tone {
    pub fn colors(self, p: Palette) -> Status {
        match self {
            Self::Neutral => Status {
                ink: p.text_muted,
                surface: p.hover,
                border: p.border,
            },
            Self::Accent => Status {
                ink: p.accent_text,
                surface: p.accent_surface,
                border: p.choice_border,
            },
            Self::Success => p.success,
            Self::Warning => p.warning,
            Self::Danger => p.danger,
            Self::Info => p.info,
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Danger | Self::Warning => "alert",
            Self::Success => "check",
            Self::Neutral | Self::Accent | Self::Info => "info",
        }
    }
}

/// An indeterminate ring. Holds still when motion is reduced.
pub fn spinner(color: Hsla, pixels: f32) -> impl IntoElement {
    Spinner::new()
        .icon(icon("loader"))
        .color(color)
        .with_size(Size::Size(px(pixels)))
}

/// A small rounded status label.
pub fn pill(p: Palette, label: impl Into<SharedString>, tone: Tone) -> Div {
    let colors = tone.colors(p);
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(5.))
        .h(px(22.))
        .px(px(9.))
        .rounded_full()
        .bg(colors.surface)
        .border_1()
        .border_color(colors.border.opacity(0.5))
        .text_size(px(size::CAPTION))
        .font_weight(FontWeight::MEDIUM)
        .line_height(relative(1.))
        .text_color(colors.ink)
        .whitespace_nowrap()
        .child(label.into())
}

/// A status pill led by an approved icon.
pub fn pill_with_icon(
    p: Palette,
    name: &'static str,
    label: impl Into<SharedString>,
    tone: Tone,
) -> Div {
    let colors = tone.colors(p);
    let label: SharedString = label.into();
    div()
        .flex()
        .flex_none()
        .items_center()
        .gap(px(5.))
        .h(px(22.))
        .pl(px(7.))
        .pr(px(9.))
        .rounded_full()
        .bg(colors.surface)
        .border_1()
        .border_color(colors.border.opacity(0.5))
        .text_size(px(size::CAPTION))
        .font_weight(FontWeight::MEDIUM)
        .line_height(relative(1.))
        .text_color(colors.ink)
        .whitespace_nowrap()
        .child(icon(name).size(px(13.)).text_color(colors.ink))
        .child(label)
}

/// A small filled dot that marks live state next to a label.
pub fn dot(color: Hsla) -> Div {
    div().size(px(7.)).flex_none().rounded_full().bg(color)
}

/// A count bubble for navigation and tabs.
pub fn count(p: Palette, value: usize, emphasised: bool) -> Div {
    let (fill, ink) = if emphasised {
        (p.action, p.on_action)
    } else {
        (p.hover, p.text_muted)
    };
    div()
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .min_w(px(20.))
        .h(px(20.))
        .px(px(6.))
        .rounded_full()
        .bg(fill)
        .text_size(px(size::CAPTION))
        .font_weight(FontWeight::SEMIBOLD)
        .line_height(relative(1.))
        .text_color(ink)
        .child(if value > 99 {
            "99+".to_owned()
        } else {
            value.to_string()
        })
}

/// An inline notice: tinted surface, tone icon, message and an optional action.
pub fn notice(p: Palette, message: impl Into<SharedString>, tone: Tone) -> Div {
    let colors = tone.colors(p);
    div()
        .flex()
        .items_start()
        .gap(px(10.))
        .p(px(12.))
        .rounded(px(radius::CONTROL))
        .bg(colors.surface)
        .border_1()
        .border_color(colors.border.opacity(0.55))
        .child(
            div()
                .pt(px(1.))
                .flex_none()
                .child(icon(tone.icon()).size(px(16.)).text_color(colors.ink)),
        )
        .child(
            text::small(message)
                .flex_1()
                .min_w_0()
                .text_color(colors.ink),
        )
}

/// A centred illustration tile, statement and optional action for empty and error states.
pub fn empty_state(
    p: Palette,
    name: &'static str,
    title: impl Into<SharedString>,
    body: impl Into<SharedString>,
    tone: Tone,
    action: Option<AnyElement>,
    window: &mut Window,
    cx: &mut App,
) -> Div {
    let colors = tone.colors(p);
    let title: SharedString = title.into();
    let entrance = motion::entrance_with(
        ("empty-state", title.clone()),
        0,
        16.0,
        motion::reveal(),
        window,
        cx,
    );
    div()
        .w_full()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(10.))
        .px(px(24.))
        .py(px(40.))
        .entering(entrance)
        .child(
            div()
                .size(px(64.))
                .mb(px(6.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(radius::CARD))
                .bg(surface::angled(135., colors.surface, p.surface))
                .border_1()
                .border_color(colors.border.opacity(0.5))
                .child(icon(name).size(px(28.)).text_color(colors.ink)),
        )
        .child(text::title(title).text_color(p.text).text_center())
        .child(
            text::small(body)
                .max_w(px(380.))
                .text_center()
                .text_color(p.text_muted),
        )
        .children(action.map(|action| div().mt(px(8.)).child(action)))
}
