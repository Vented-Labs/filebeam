//! The page scaffold: a full-height scroll viewport with consistent gutters, a
//! reading-width column and a heading that enters with the page.

use gpui::{
    App, Div, InteractiveElement, IntoElement, ParentElement, ScrollHandle, SharedString,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};
use gpui_component::scroll::Scrollbar;

use crate::{
    theme::{Palette, tokens::geometry},
    ui::{
        motion::{self, MotionExt},
        text,
    },
};

/// Whether the window is narrow enough for the sidebar to collapse.
pub fn is_compact(window: &Window) -> bool {
    window.viewport_size().width < px(geometry::COMPACT_BELOW)
}

/// Horizontal page margin for the current window width.
pub fn gutter(window: &Window) -> f32 {
    if is_compact(window) {
        geometry::PAGE_GUTTER_COMPACT
    } else {
        geometry::PAGE_GUTTER
    }
}

/// Width left for page content once the sidebar and gutters are taken out.
pub fn content_width(window: &Window) -> f32 {
    let sidebar = if is_compact(window) {
        geometry::SIDEBAR_RAIL
    } else {
        geometry::SIDEBAR
    };
    (window.viewport_size().width.as_f32() - sidebar - gutter(window) * 2.).max(0.)
}

/// A page title with a supporting line.
pub fn header(
    p: Palette,
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    window: &mut Window,
    cx: &mut App,
) -> Div {
    let title: SharedString = title.into();
    let entrance = motion::entrance(("page-header", title.clone()), 0, window, cx);
    div()
        .flex()
        .flex_col()
        .gap(px(4.))
        .entering(entrance)
        .child(text::headline(title).text_color(p.text))
        .child(text::body(detail).text_color(p.text_muted))
}

/// A header with trailing actions aligned to its baseline.
pub fn header_with(
    p: Palette,
    title: impl Into<SharedString>,
    detail: impl Into<SharedString>,
    trailing: impl IntoElement,
    window: &mut Window,
    cx: &mut App,
) -> Div {
    div()
        .w_full()
        .flex()
        .items_end()
        .justify_between()
        .gap(px(16.))
        .child(header(p, title, detail, window, cx).flex_1().min_w_0())
        .child(div().flex_none().child(trailing))
}

/// A full-height page viewport with content gutters and an app-edge scrollbar.
/// `max_width` caps the content column; wider windows centre it.
pub fn scroller(
    id: &'static str,
    scroll_handle: &ScrollHandle,
    max_width: f32,
    centered: bool,
    window: &Window,
    content: impl IntoElement,
) -> impl IntoElement {
    let horizontal = gutter(window);
    let vertical = if is_compact(window) { 18. } else { 24. };
    div()
        .id(id)
        .size_full()
        .min_h_0()
        .relative()
        .child(
            div()
                .id((id, 0_u64))
                .size_full()
                .min_h_0()
                .flex()
                .flex_col()
                .overflow_y_scroll()
                .track_scroll(scroll_handle)
                .child(
                    div()
                        .w_full()
                        .when(centered, |this| this.flex_1().flex().flex_col())
                        .when(!centered, |this| this.flex_none())
                        .px(px(horizontal))
                        .pt(px(vertical))
                        .pb(px(vertical + 4.))
                        .child(
                            div()
                                .w_full()
                                .max_w(px(max_width))
                                .mx_auto()
                                .when(centered, |this| this.my_auto())
                                .child(content),
                        ),
                ),
        )
        // The scrollbar's viewport is this narrow app-edge overlay rather than
        // the padded scroll content.
        .child(
            div()
                .absolute()
                .top_0()
                .bottom_0()
                .right(px(2.))
                .w(px(12.))
                .child(Scrollbar::vertical(scroll_handle).viewport_from_layout()),
        )
}

/// A page whose content fills the viewport and manages its own scrolling regions.
pub fn fixed(window: &Window, max_width: f32, content: impl IntoElement) -> Div {
    let horizontal = gutter(window);
    let vertical = if is_compact(window) { 18. } else { 24. };
    div()
        .size_full()
        .min_h_0()
        .flex()
        .flex_col()
        .px(px(horizontal))
        .pt(px(vertical))
        .pb(px(vertical))
        .child(
            div()
                .size_full()
                .min_h_0()
                .max_w(px(max_width))
                .mx_auto()
                .flex()
                .flex_col()
                .child(content),
        )
}
