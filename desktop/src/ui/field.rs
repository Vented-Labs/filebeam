//! Labelled inputs and read-only values. The component library supplies editing
//! behaviour; these helpers give every field the same label, sizing and messages.

use std::rc::Rc;

use gpui::{
    App, Bounds, ClickEvent, Div, ElementId, Entity, FontWeight, InteractiveElement, IntoElement,
    ParentElement, PathBuilder, Pixels, RenderOnce, SharedString, Styled, Window, canvas, div,
    point, prelude::FluentBuilder, px,
};
use gpui_base::{Checkbox as BaseCheckbox, CheckboxState};
use gpui_component::{
    Sizable, Size,
    input::{Input, InputState},
    select::{Select, SelectDelegate, SelectState},
};

use crate::{
    assets::icon,
    theme::{
        self, Palette,
        tokens::{geometry, radius, space, text as size},
    },
    ui::{
        button,
        feedback::Tone,
        motion,
        text::{self, TextExt},
    },
};

/// A single-line input at the standard control height, on the recessed field fill.
pub fn input(p: Palette, state: &Entity<InputState>) -> Input {
    Input::new(state)
        .with_size(Size::Large)
        .h(px(geometry::CONTROL_HEIGHT))
        .rounded(px(radius::CONTROL))
        .bg(p.sunken)
}

/// A masked input with the built-in show/hide control.
pub fn secret(p: Palette, state: &Entity<InputState>) -> Input {
    input(p, state).mask_toggle()
}

/// A select at the standard control height. The component's root fills its parent,
/// so place it in a box of the width it should take.
pub fn select<D>(p: Palette, state: &Entity<SelectState<D>>) -> Select<D>
where
    D: SelectDelegate + 'static,
{
    Select::new(state)
        .with_size(Size::Large)
        .h(px(geometry::CONTROL_HEIGHT))
        .rounded(px(radius::CONTROL))
        .bg(p.sunken)
}

/// The label above a control, with an optional marker.
pub fn label(p: Palette, value: impl Into<SharedString>, optional: bool) -> Div {
    div()
        .flex()
        .items_baseline()
        .gap(px(6.))
        .child(
            div()
                .text_size(px(size::SMALL))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(p.text)
                .child(value.into()),
        )
        .when(optional, |this| {
            this.child(text::caption("Optional").text_color(p.text_subtle))
        })
}

/// A supporting or error line beneath a control.
pub fn hint(p: Palette, message: impl Into<SharedString>, tone: Tone) -> Div {
    let ink = match tone {
        Tone::Neutral => p.text_muted,
        other => other.colors(p).ink,
    };
    text::caption(message).text_color(ink)
}

/// Label, control and an optional message stacked as one field.
pub fn field(p: Palette, name: impl Into<SharedString>, control: impl IntoElement) -> Div {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(7.))
        .child(label(p, name, false))
        .child(control)
}

pub fn optional_field(p: Palette, name: impl Into<SharedString>, control: impl IntoElement) -> Div {
    div()
        .w_full()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(7.))
        .child(label(p, name, true))
        .child(control)
}

type CopyHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// A read-only value with a copy action, so the value itself gets the width.
/// The primary variant fills the action with the accent.
#[derive(IntoElement)]
pub struct CopyField {
    id: ElementId,
    icon: &'static str,
    value: SharedString,
    action: SharedString,
    primary: bool,
    copied: bool,
    on_copy: Option<CopyHandler>,
}

pub fn copy_field(
    id: impl Into<ElementId>,
    icon: &'static str,
    value: impl Into<SharedString>,
) -> CopyField {
    CopyField {
        id: id.into(),
        icon,
        value: value.into(),
        action: "Copy".into(),
        primary: false,
        copied: false,
        on_copy: None,
    }
}

impl CopyField {
    /// The copy button's label, e.g. "Copy link".
    pub fn action(mut self, label: impl Into<SharedString>) -> Self {
        self.action = label.into();
        self
    }

    pub fn primary(mut self) -> Self {
        self.primary = true;
        self
    }

    /// Shows a confirmation in place of the action label.
    pub fn copied(mut self, copied: bool) -> Self {
        self.copied = copied;
        self
    }

    pub fn on_copy(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_copy = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for CopyField {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = theme::palette(cx);
        let handler = self.on_copy;
        let label = if self.copied {
            SharedString::from("Copied")
        } else {
            self.action
        };
        let action = if self.primary {
            button::primary(self.id)
        } else {
            button::secondary(self.id)
        }
        .inset(geometry::CONTROL_HEIGHT_LARGE, radius::CONTROL)
        .icon(if self.copied { "check" } else { "copy" })
        .label(label)
        .when_some(handler, |this, handler| {
            this.on_click(move |event, window, cx| handler(event, window, cx))
        });
        div()
            .w_full()
            .min_w_0()
            .h(px(geometry::CONTROL_HEIGHT_LARGE))
            .flex()
            .items_center()
            .gap(px(10.))
            .pl(px(12.))
            // The border is the first pixel of the inset the button is sized for.
            .pr(px(space::INSET - 1.))
            .rounded(px(radius::CONTROL))
            .bg(p.sunken)
            .border_1()
            .border_color(p.border)
            .child(
                icon(self.icon)
                    .size(px(16.))
                    .flex_none()
                    .text_color(p.text_muted),
            )
            .child(
                text::code(self.value)
                    .flex_1()
                    .single_line()
                    .text_color(p.text_muted),
            )
            .child(action)
    }
}

type CheckHandler = Rc<dyn Fn(bool, &mut Window, &mut App)>;

/// A checkbox with a drawn tick. The approved "check" icon is a circled tick, which
/// reads as a ring at this size, so the mark is painted directly.
#[derive(IntoElement)]
pub struct Check {
    id: ElementId,
    label: SharedString,
    checked: bool,
    disabled: bool,
    on_change: Option<CheckHandler>,
}

/// `label` is the accessible name; it is not drawn.
pub fn check(id: impl Into<ElementId>, label: impl Into<SharedString>, checked: bool) -> Check {
    Check {
        id: id.into(),
        label: label.into(),
        checked,
        disabled: false,
        on_change: None,
    }
}

impl Check {
    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn on_change(mut self, handler: impl Fn(bool, &mut Window, &mut App) + 'static) -> Self {
        self.on_change = Some(Rc::new(handler));
        self
    }
}

impl RenderOnce for Check {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        const SIZE: f32 = 18.;
        let p = theme::palette(cx);
        let mark = motion::eased(
            (self.id.clone(), "mark"),
            if self.checked { 1. } else { 0. },
            motion::fade(),
            window,
            cx,
        );
        let ink = p.on_action.opacity(mark);
        let handler = self.on_change;
        BaseCheckbox::new(self.id)
            .accessibility_label(self.label)
            .checked(self.checked)
            .disabled(self.disabled)
            .size(px(SIZE))
            .flex_none()
            .rounded(px(5.))
            .border_1()
            .when(self.checked, |this| {
                this.bg(p.action).border_color(p.action_border)
            })
            .when(!self.checked, |this| {
                this.bg(p.sunken)
                    .border_color(p.control_border)
                    .hover(move |style| style.border_color(p.control_border_hover))
            })
            .when(self.disabled, |this| this.opacity(0.5))
            .when(!self.disabled, |this| this.cursor_pointer())
            .focus_visible(move |style| style.border_color(p.choice_border))
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds: Bounds<Pixels>, (), window, _| {
                        if mark <= 0. {
                            return;
                        }
                        let at = |x: f32, y: f32| {
                            point(bounds.origin.x + px(x), bounds.origin.y + px(y))
                        };
                        let mut tick = PathBuilder::stroke(px(1.8));
                        tick.move_to(at(4.2, 9.2));
                        tick.line_to(at(7.4, 12.2));
                        tick.line_to(at(12.8, 5.6));
                        if let Ok(path) = tick.build() {
                            window.paint_path(path, ink);
                        }
                    },
                )
                .absolute()
                .top(px(-1.))
                .left(px(-1.))
                .size(px(SIZE)),
            )
            .when_some(handler, |this, handler| {
                this.on_change(move |state, _, window, cx| {
                    handler(state == CheckboxState::Checked, window, cx)
                })
            })
    }
}
