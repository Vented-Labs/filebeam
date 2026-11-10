//! A row of mutually exclusive options with a selection pill that slides between
//! them. With captions it renders the taller two-line variant used for transports.

use std::rc::Rc;

use gpui::{
    App, Axis, ElementId, FontWeight, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    SharedString, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
    relative,
};
use gpui_base::{Radio, RadioGroup};

use crate::{
    assets::icon,
    theme::{
        self,
        tokens::{radius, text as size},
    },
    ui::motion,
};

#[derive(Clone)]
pub struct Segment {
    label: SharedString,
    icon: Option<&'static str>,
    caption: Option<SharedString>,
    enabled: bool,
}

impl Segment {
    pub fn new(label: impl Into<SharedString>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            caption: None,
            enabled: true,
        }
    }

    pub fn icon(mut self, name: &'static str) -> Self {
        self.icon = Some(name);
        self
    }

    pub fn caption(mut self, caption: impl Into<SharedString>) -> Self {
        self.caption = Some(caption.into());
        self
    }

    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }
}

type SelectHandler = Rc<dyn Fn(usize, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Rail {
    id: ElementId,
    label: SharedString,
    segments: Vec<Segment>,
    selected: usize,
    on_select: Option<SelectHandler>,
    fill: bool,
    segment_width: f32,
}

/// `label` names the group for assistive technology; it is not drawn.
pub fn rail(id: impl Into<ElementId>, label: impl Into<SharedString>, selected: usize) -> Rail {
    Rail {
        id: id.into(),
        label: label.into(),
        segments: Vec::new(),
        selected,
        on_select: None,
        fill: false,
        segment_width: 116.,
    }
}

impl Rail {
    pub fn segment(mut self, segment: Segment) -> Self {
        self.segments.push(segment);
        self
    }

    pub fn segments(mut self, segments: impl IntoIterator<Item = Segment>) -> Self {
        self.segments.extend(segments);
        self
    }

    /// Stretches across its container, dividing the width between segments.
    pub fn fill(mut self) -> Self {
        self.fill = true;
        self
    }

    /// Width of each segment when the rail hugs its content.
    pub fn segment_width(mut self, width: f32) -> Self {
        self.segment_width = width;
        self
    }

    pub fn on_select(mut self, handler: impl Fn(usize, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Rail {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        const INSET: f32 = 4.;
        let p = theme::palette(cx);
        let count = self.segments.len().max(1);
        let tall = self
            .segments
            .iter()
            .any(|segment| segment.caption.is_some());
        let height = if tall { 60. } else { 40. };
        let selected = self.selected.min(count - 1);
        // The pill travels as a fraction of the lane, so it tracks any rail width.
        let position = motion::spring_unit(
            (self.id.clone(), "pill"),
            selected as f32 / count as f32,
            motion::SPATIAL,
            window,
            cx,
        );
        let on_select = self.on_select;
        let total = count;

        let segments = self
            .segments
            .into_iter()
            .enumerate()
            .map(move |(index, segment)| {
                let chosen = index == selected;
                let ink = if !segment.enabled {
                    p.disabled_text
                } else if chosen {
                    p.text
                } else {
                    p.text_muted
                };
                let icon_ink = if chosen && segment.enabled {
                    p.accent_text
                } else {
                    ink
                };
                let on_select = on_select.clone();
                Radio::new(index)
                    .accessibility_label(segment.label.clone())
                    .checked(chosen)
                    .disabled(!segment.enabled)
                    .set_position(index + 1, total)
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .flex()
                    .items_center()
                    .when(!tall, |this| this.justify_center())
                    .gap(px(if tall { 11. } else { 8. }))
                    .px(px(if tall { 12. } else { 10. }))
                    .rounded(px(radius::CONTROL))
                    .text_color(ink)
                    .when(segment.enabled && !chosen, |this| {
                        this.cursor_pointer()
                            .hover(move |style| style.text_color(p.text))
                    })
                    .focus_visible(move |style| style.bg(p.hover))
                    .when_some(segment.icon, |this, name| {
                        this.child(
                            icon(name)
                                .size(px(if tall { 20. } else { 16. }))
                                .text_color(icon_ink),
                        )
                    })
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .gap(px(2.))
                            .child(
                                div()
                                    .text_size(px(if tall { size::BODY } else { size::SMALL }))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .line_height(relative(1.2))
                                    .whitespace_nowrap()
                                    .child(segment.label),
                            )
                            .when_some(segment.caption.filter(|_| tall), |this, caption| {
                                this.child(
                                    div()
                                        .text_size(px(size::CAPTION))
                                        .line_height(relative(1.25))
                                        .text_color(if segment.enabled {
                                            p.text_muted
                                        } else {
                                            p.disabled_text
                                        })
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .child(caption),
                                )
                            }),
                    )
                    .on_change(move |_, _, window, cx| {
                        if let Some(handler) = &on_select {
                            handler(index, window, cx);
                        }
                    })
            })
            .collect::<Vec<_>>();

        div()
            .relative()
            .h(px(height))
            .flex_none()
            .rounded(px(radius::CONTROL + INSET))
            .bg(p.sunken)
            .border_1()
            .border_color(p.border)
            .when(self.fill, |this| this.w_full())
            .when(!self.fill, |this| {
                this.w(px(self.segment_width * count as f32 + INSET * 2.))
            })
            .child(
                div()
                    .absolute()
                    .top(px(INSET - 1.))
                    .bottom(px(INSET - 1.))
                    .left(px(INSET - 1.))
                    .right(px(INSET - 1.))
                    .child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left(relative(position))
                            .w(relative(1. / count as f32))
                            .rounded(px(radius::CONTROL))
                            .bg(p.selected_surface)
                            .border_1()
                            .border_color(p.choice_border),
                    ),
            )
            .child(
                RadioGroup::new(self.id)
                    .axis(Axis::Horizontal)
                    .aria_label(self.label)
                    .absolute()
                    .top(px(INSET - 1.))
                    .bottom(px(INSET - 1.))
                    .left(px(INSET - 1.))
                    .right(px(INSET - 1.))
                    .flex()
                    .children(segments),
            )
    }
}
