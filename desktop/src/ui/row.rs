//! Rows inside panels: settings toggles, navigation links and label/value facts.

use std::rc::Rc;

use gpui::{
    App, ClickEvent, Div, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    SharedString, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};
use gpui_component::{Disableable, switch::Switch};

use crate::{
    assets::icon,
    theme::{self, Palette, tokens::radius},
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

/// A navigation row: icon, title, optional detail and a chevron.
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
        div()
            .id(self.id)
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(12.))
            .px(px(12.))
            .py(px(12.))
            .rounded(px(radius::CONTROL))
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
            })
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

/// A square tile behind an icon, used to lead list rows.
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
