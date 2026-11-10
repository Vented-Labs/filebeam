//! The app's buttons. Primary fills with the action gradient and glows; Secondary is
//! outlined; Tonal is a tinted fill for supporting actions; Quiet is text-only;
//! Danger is a quiet destructive action. While `loading` the icon gives way to a
//! spinner and presses are ignored.

use std::rc::Rc;

use gpui::{
    App, ClickEvent, ElementId, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement,
    RenderOnce, SharedString, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder, px, relative,
};
use gpui_base::Button as BaseButton;
use gpui_component::tooltip::Tooltip;

use crate::{
    theme::{
        self, Palette,
        tokens::{geometry, radius, text},
    },
    ui::{feedback::spinner, surface},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Primary,
    Secondary,
    Tonal,
    Quiet,
    /// A neutral quiet control, for icon buttons and low-emphasis actions.
    Ghost,
    Danger,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    Small,
    Medium,
    Large,
}

impl Scale {
    fn height(self) -> f32 {
        match self {
            Self::Small => geometry::CONTROL_HEIGHT_SMALL,
            Self::Medium => geometry::CONTROL_HEIGHT,
            Self::Large => geometry::CONTROL_HEIGHT_LARGE,
        }
    }

    fn padding(self) -> f32 {
        match self {
            Self::Small => 12.,
            Self::Medium => 16.,
            Self::Large => 20.,
        }
    }

    fn text(self) -> f32 {
        match self {
            Self::Small => text::SMALL,
            Self::Medium | Self::Large => text::BODY,
        }
    }

    fn icon(self) -> f32 {
        match self {
            Self::Small => 15.,
            Self::Medium => 17.,
            Self::Large => 18.,
        }
    }
}

type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

#[derive(IntoElement)]
pub struct Action {
    id: ElementId,
    label: Option<SharedString>,
    icon: Option<&'static str>,
    trailing_icon: Option<&'static str>,
    kind: Kind,
    scale: Scale,
    disabled: bool,
    loading: bool,
    fill: bool,
    tooltip: Option<SharedString>,
    accessibility_label: Option<SharedString>,
    on_click: Option<ClickHandler>,
}

impl Action {
    fn new(id: impl Into<ElementId>, kind: Kind) -> Self {
        Self {
            id: id.into(),
            label: None,
            icon: None,
            trailing_icon: None,
            kind,
            scale: Scale::Medium,
            disabled: false,
            loading: false,
            fill: false,
            tooltip: None,
            accessibility_label: None,
            on_click: None,
        }
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// An approved icon name, shown before the label.
    pub fn icon(mut self, name: &'static str) -> Self {
        self.icon = Some(name);
        self
    }

    pub fn trailing_icon(mut self, name: &'static str) -> Self {
        self.trailing_icon = Some(name);
        self
    }

    pub fn small(mut self) -> Self {
        self.scale = Scale::Small;
        self
    }

    pub fn large(mut self) -> Self {
        self.scale = Scale::Large;
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn loading(mut self, loading: bool) -> Self {
        self.loading = loading;
        self
    }

    /// Stretches to the width of its container.
    pub fn fill(mut self) -> Self {
        self.fill = true;
        self
    }

    pub fn tooltip(mut self, tooltip: impl Into<SharedString>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.accessibility_label = Some(label.into());
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

pub fn primary(id: impl Into<ElementId>) -> Action {
    Action::new(id, Kind::Primary)
}

pub fn secondary(id: impl Into<ElementId>) -> Action {
    Action::new(id, Kind::Secondary)
}

pub fn tonal(id: impl Into<ElementId>) -> Action {
    Action::new(id, Kind::Tonal)
}

pub fn quiet(id: impl Into<ElementId>) -> Action {
    Action::new(id, Kind::Quiet)
}

pub fn ghost(id: impl Into<ElementId>) -> Action {
    Action::new(id, Kind::Ghost)
}

pub fn danger(id: impl Into<ElementId>) -> Action {
    Action::new(id, Kind::Danger)
}

struct Look {
    ink: Hsla,
    border: Hsla,
    hover_fill: Hsla,
    hover_border: Hsla,
    press_fill: Hsla,
}

fn look(kind: Kind, p: Palette) -> Look {
    let clear = gpui::transparent_black();
    match kind {
        Kind::Primary => Look {
            ink: p.on_action,
            border: p.action_border,
            hover_fill: p.action_hover,
            hover_border: p.action_border,
            press_fill: p.action_pressed,
        },
        Kind::Secondary => Look {
            ink: p.text,
            border: p.control_border,
            hover_fill: p.hover,
            hover_border: p.control_border_hover,
            press_fill: p.active,
        },
        Kind::Tonal => Look {
            ink: p.accent_text,
            border: p.choice_border.opacity(0.45),
            hover_fill: p.selected_surface,
            hover_border: p.choice_border,
            press_fill: p.selected_surface,
        },
        Kind::Quiet => Look {
            ink: p.accent_text,
            border: clear,
            hover_fill: p.hover,
            hover_border: clear,
            press_fill: p.active,
        },
        Kind::Ghost => Look {
            ink: p.text_muted,
            border: clear,
            hover_fill: p.hover,
            hover_border: clear,
            press_fill: p.active,
        },
        Kind::Danger => Look {
            ink: p.danger.ink,
            border: clear,
            hover_fill: p.danger.surface,
            hover_border: p.danger.border.opacity(0.5),
            press_fill: p.danger.surface,
        },
    }
}

impl RenderOnce for Action {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = theme::palette(cx);
        let look = look(self.kind, p);
        let inert = self.disabled || self.loading;
        let ink = if self.disabled {
            p.disabled_text
        } else {
            look.ink
        };
        let icon_size = px(self.scale.icon());
        let icon_only = self.label.is_none();
        let accessibility_label = self
            .accessibility_label
            .or_else(|| self.label.clone())
            .or_else(|| self.tooltip.clone());

        let mut button = BaseButton::new(self.id)
            .disabled(inert)
            .h(px(self.scale.height()))
            .flex_none()
            .gap(px(8.))
            .rounded(px(radius::CONTROL))
            .border_1()
            .text_size(px(self.scale.text()))
            .font_weight(FontWeight::SEMIBOLD)
            .line_height(relative(1.))
            .text_color(ink)
            .when(icon_only, |this| this.w(px(self.scale.height())))
            .when(!icon_only, |this| this.px(px(self.scale.padding())))
            .when(self.fill, |this| this.w_full().flex_1());

        button = match (self.kind, self.disabled) {
            (Kind::Primary | Kind::Secondary | Kind::Tonal, true) => button
                .bg(p.disabled_surface)
                .border_color(p.disabled_border),
            (Kind::Quiet | Kind::Ghost | Kind::Danger, true) => {
                button.border_color(gpui::transparent_black())
            }
            (Kind::Primary, false) => button
                .bg(surface::action_fill(p))
                .border_color(look.border)
                .shadow(surface::shadow_action(p)),
            (Kind::Secondary, false) => button.bg(p.surface).border_color(look.border),
            (Kind::Tonal, false) => button.bg(p.accent_surface).border_color(look.border),
            (Kind::Quiet | Kind::Ghost | Kind::Danger, false) => button.border_color(look.border),
        };
        if !inert {
            let (hover_fill, hover_border, press_fill) =
                (look.hover_fill, look.hover_border, look.press_fill);
            button = button
                .cursor_pointer()
                .hover(move |style| style.bg(hover_fill).border_color(hover_border))
                .active(move |style| style.bg(press_fill))
                .focus_visible(move |style| style.border_color(p.choice_border));
        }
        if let Some(label) = accessibility_label {
            button = button.accessibility_label(label);
        }
        if let Some(handler) = self.on_click {
            button = button.on_click(move |event, window, cx| handler(event, window, cx));
        }
        if let Some(tooltip) = self.tooltip {
            button =
                button.tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx));
        }

        button
            .when(self.loading, |this| {
                this.child(spinner(ink, self.scale.icon()))
            })
            .when_some(self.icon.filter(|_| !self.loading), |this, name| {
                this.child(crate::assets::icon(name).size(icon_size).text_color(ink))
            })
            .when_some(self.label, |this, label| {
                this.child(div().whitespace_nowrap().child(label))
            })
            .when_some(self.trailing_icon, |this, name| {
                this.child(
                    crate::assets::icon(name)
                        .size(px(self.scale.icon() - 2.))
                        .text_color(ink),
                )
            })
    }
}

/// A square icon-only control with an accessible label and tooltip.
pub fn icon_button(id: impl Into<ElementId>, icon: &'static str, label: &'static str) -> Action {
    Action::new(id, Kind::Ghost)
        .icon(icon)
        .tooltip(label)
        .accessibility_label(label)
}
