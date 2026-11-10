//! Rows inside panels: settings toggles, navigation links, label/value facts and the
//! selectable rows of a list.

use std::rc::Rc;

use gpui::{
    App, ClickEvent, Div, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    SharedString, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};
use gpui_component::{Disableable, switch::Switch};

use crate::{
    assets::icon,
    theme::{
        self, Palette,
        tokens::{radius, space},
    },
    ui::text::{self, TextExt},
};

type ToggleHandler = Rc<dyn Fn(&bool, &mut Window, &mut App)>;

/// A labelled row with a trailing switch.
#[derive(IntoElement)]
pub struct SwitchRow {
    id: ElementId,
    title: SharedString,
    description: Option<SharedString>,
    icon: Option<&'static str>,
    checked: bool,
    disabled: bool,
    on_toggle: Option<ToggleHandler>,
}

pub fn switch_row(
    id: impl Into<ElementId>,
    title: impl Into<SharedString>,
    checked: bool,
) -> SwitchRow {
    SwitchRow {
        id: id.into(),
        title: title.into(),
        description: None,
        icon: None,
        checked,
        disabled: false,
        on_toggle: None,
    }
}

impl SwitchRow {
    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    pub fn icon(mut self, name: &'static str) -> Self {
        self.icon = Some(name);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_toggle(mut self, handler: impl Fn(&bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_toggle = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for SwitchRow {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = theme::palette(cx);
        let ink = if self.disabled {
            p.disabled_text
        } else {
            p.text
        };
        let muted = if self.disabled {
            p.disabled_text
        } else {
            p.text_muted
        };
        let handler = self.on_toggle;
        div()
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(12.))
            .py(px(10.))
            .when_some(self.icon, |this, name| {
                this.child(icon(name).size(px(18.)).flex_none().text_color(muted))
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .child(text::body_strong(self.title).text_color(ink))
                    .when_some(self.description, |this, description| {
                        this.child(text::small(description).text_color(muted))
                    }),
            )
            .child(
                Switch::new(self.id)
                    .checked(self.checked)
                    .disabled(self.disabled)
                    .when_some(handler, |this, handler| {
                        this.on_click(move |checked, window, cx| handler(checked, window, cx))
                    }),
            )
    }
}

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// A navigation row that hugs its panel: icon, title, optional detail and a chevron.
/// Place it in a [`Hugged`] container.
#[derive(IntoElement)]
pub struct LinkRow {
    id: ElementId,
    icon: &'static str,
    title: SharedString,
    detail: Option<SharedString>,
    on_click: Option<ClickHandler>,
}

pub fn link_row(
    id: impl Into<ElementId>,
    icon: &'static str,
    title: impl Into<SharedString>,
) -> LinkRow {
    LinkRow {
        id: id.into(),
        icon,
        title: title.into(),
        detail: None,
        on_click: None,
    }
}

impl LinkRow {
    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for LinkRow {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = theme::palette(cx);
        let handler = self.on_click;
        slot().child(
            hugging_frame(Fit::PANEL_DENSE)
                .id(self.id)
                .px(px(LINK_INSET))
                .cursor_pointer()
                .hover(move |style| style.bg(p.hover))
                .active(move |style| style.bg(p.active))
                .child(
                    icon(self.icon)
                        .size(px(18.))
                        .flex_none()
                        .text_color(p.text_muted),
                )
                .child(
                    text::body_strong(self.title)
                        .flex_1()
                        .single_line()
                        .text_color(p.text),
                )
                .when_some(self.detail, |this, detail| {
                    this.child(text::small(detail).flex_none().text_color(p.text_muted))
                })
                .child(
                    icon("arrow-right")
                        .size(px(15.))
                        .flex_none()
                        .text_color(p.text_subtle),
                )
                .when_some(handler, |this, handler| {
                    this.on_click(move |event, window, cx| handler(event, window, cx))
                }),
        )
    }
}

/// A label and its value on one line, for detail panes.
pub fn fact(p: Palette, label: impl Into<SharedString>, value: impl Into<SharedString>) -> Div {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .items_baseline()
        .justify_between()
        .gap(px(16.))
        .child(text::small(label).flex_none().text_color(p.text_muted))
        .child(
            text::small_strong(value)
                .min_w_0()
                .text_right()
                .text_color(p.text),
        )
}

/// A square tile behind an icon, for a heading or a row that stands on its own.
/// Inside a hugging row use [`lead_tile`], which follows the row's corners.
pub fn icon_tile(p: Palette, name: &'static str, pixels: f32, accent: bool) -> Div {
    let (fill, ink) = if accent {
        (p.accent_surface, p.accent_text)
    } else {
        (p.hover, p.text_muted)
    };
    div()
        .size(px(pixels))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(radius::CONTROL))
        .bg(fill)
        .child(icon(name).size(px(pixels * 0.48)).text_color(ink))
}

/// How rows sit inside a container: each row hugs it `space::HUG` from its outer edge
/// and leads with a tile or avatar the same distance from the row's own edge on every
/// side, so the container, the row and the tile share their corner centres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fit {
    outer: f32,
    height: f32,
    lead_gap: f32,
}

impl Fit {
    /// Rows directly inside a panel.
    pub const PANEL: Self = Self {
        outer: radius::PANEL,
        height: 64.,
        lead_gap: 10.,
    };
    /// Single-line rows directly inside a panel.
    pub const PANEL_DENSE: Self = Self {
        outer: radius::PANEL,
        height: 52.,
        lead_gap: 10.,
    };
    /// Denser rows inside a card.
    pub const CARD: Self = Self {
        outer: radius::CARD,
        height: 52.,
        lead_gap: 6.,
    };

    /// The radius of a row, or of a well a row opens into.
    pub const fn radius(self) -> f32 {
        radius::nested(self.outer, space::HUG)
    }

    /// The side of the leading tile or avatar.
    pub const fn lead(self) -> f32 {
        self.height - 2. * self.lead_gap
    }

    const fn lead_radius(self) -> f32 {
        radius::nested(self.radius(), self.lead_gap)
    }

    /// The height one [`slot`] takes, for lists that lay their rows out by size.
    pub const fn slot_height(self) -> f32 {
        self.height + SPACING
    }
}

/// Space between two hugging rows. Each [`slot`] carries half of it above and below.
const SPACING: f32 = 4.;
const BORDER: f32 = 1.;
/// Inside a row with no leading tile, its content starts clear of the corner's curve.
const LINK_INSET: f32 = 14.;
/// Where that content starts, measured from the inside of a [`Hugged`] container, for
/// plain content that shares the container with such rows.
pub const HUGGED_CONTENT_INSET: f32 = LINK_INSET + BORDER;

/// Pads a bordered container so the rows it holds sit `space::HUG` inside its outer
/// edge on every side, which is what makes their corners concentric with its own.
pub trait Hugged: Styled + Sized {
    fn hugged(self) -> Self {
        self.px(px(space::HUG - BORDER))
            .py(px(space::HUG - BORDER - SPACING / 2.))
    }
}

impl<T: Styled> Hugged for T {}

/// The space one hugging row takes in a list, spacing included.
pub fn slot() -> Div {
    div().w_full().flex_none().py(px(SPACING / 2.))
}

/// The shape of a row that hugs its container and follows its corners, without any
/// response to the pointer. Place it in a [`slot`] inside a [`Hugged`] container.
pub fn hugging_frame(fit: Fit) -> Div {
    div()
        .w_full()
        .min_w_0()
        .h(px(fit.height))
        .flex()
        .items_center()
        .gap(px(12.))
        .px(px(fit.lead_gap - BORDER))
        .rounded(px(fit.radius()))
        .border_1()
        .border_color(gpui::transparent_black())
}

/// A [`hugging_frame`] that can be chosen: a selected row is tinted and outlined, any
/// other shows the hover fill.
pub fn hugging(p: Palette, fit: Fit, selected: bool) -> Div {
    hugging_frame(fit)
        .cursor_pointer()
        .when(selected, |this| {
            this.bg(p.selected_surface)
                .border_color(p.choice_border.opacity(0.55))
        })
        .when(!selected, |this| this.hover(move |style| style.bg(p.hover)))
}

/// The tile that leads a hugging row, concentric with the row's own corners.
pub fn lead_tile(p: Palette, fit: Fit, name: &'static str, accent: bool) -> Div {
    icon_tile(p, name, fit.lead(), accent).rounded(px(fit.lead_radius()))
}
