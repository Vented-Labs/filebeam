use gpui::{
    Div, InteractiveElement, IntoElement, ParentElement, ScrollHandle, StatefulInteractiveElement,
    Styled, Window, div, px,
};
use gpui_component::scroll::{Scrollbar, ScrollbarMode};

use crate::theme::Palette;

pub const HEADER_GAP: f32 = 20.;

pub fn heading(p: Palette, title: &str, detail: &str) -> Div {
    div()
        .child(div().text_size(px(28.)).child(title.to_owned()))
        .child(
            div()
                .text_size(px(12.))
                .text_color(p.muted)
                .child(detail.to_owned()),
        )
}

/// A full-height page viewport with content insets and an app-edge native scrollbar.
pub fn scroller(
    id: &'static str,
    scroll_handle: &ScrollHandle,
    window: &Window,
    content: Div,
) -> impl IntoElement {
    let compact = window.viewport_size().width < px(1100.);
    let horizontal = if compact { 21. } else { 36. };
    let vertical = if compact { 21. } else { 31. };

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
                        .flex_none()
                        .px(px(horizontal))
                        .pt(px(vertical))
                        .pb(px(vertical))
                        .child(
                            div()
                                .w_full()
                                .max_w(px(crate::theme::tokens::geometry::CONTENT_MAX))
                                .mx_auto()
                                .child(content),
                        ),
                ),
        )
        // The native scrollbar's viewport is this narrow app-edge overlay,
        // rather than the padded scroll content.
        .child(
            div()
                .absolute()
                .top(px(0.))
                .bottom(px(0.))
                .right(px(2.))
                .w(px(14.))
                .child(
                    Scrollbar::vertical(scroll_handle)
                        .mode(ScrollbarMode::Always)
                        .viewport_from_layout(),
                ),
        )
}

/// A full-height page viewport that centers content when its natural height fits.
/// Oversized content keeps its top edge and remains scrollable.
pub fn centered_scroller(
    id: &'static str,
    scroll_handle: &ScrollHandle,
    window: &Window,
    content: Div,
) -> impl IntoElement {
    let compact = window.viewport_size().width < px(1100.);
    let horizontal = if compact { 21. } else { 36. };
    let vertical = if compact { 21. } else { 31. };

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
                        .flex_1()
                        .flex()
                        .flex_col()
                        .px(px(horizontal))
                        .pt(px(vertical))
                        .pb(px(vertical))
                        .child(
                            div()
                                .w_full()
                                .max_w(px(crate::theme::tokens::geometry::CONTENT_MAX))
                                .mx_auto()
                                .mt_auto()
                                .mb_auto()
                                .child(content),
                        ),
                ),
        )
        .child(
            div()
                .absolute()
                .top(px(0.))
                .bottom(px(0.))
                .right(px(2.))
                .w(px(14.))
                .child(
                    Scrollbar::vertical(scroll_handle)
                        .mode(ScrollbarMode::Always)
                        .viewport_from_layout(),
                ),
        )
}
