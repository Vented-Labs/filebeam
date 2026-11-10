//! Modal prompts: a title, a short explanation and a row of actions.

use std::rc::Rc;

use gpui::{AnyElement, App, ParentElement, SharedString, Styled, Window, div, px};
use gpui_component::WindowExt;

use crate::{
    theme::{self, Palette, tokens::radius},
    ui::{button, text},
};

const WIDTH: f32 = 440.;

/// The words of a prompt. A destructive prompt colours its confirming action and
/// ignores Enter, so removing something always takes a deliberate click.
#[derive(Clone)]
pub struct Prompt {
    title: SharedString,
    body: Option<SharedString>,
    confirm: SharedString,
    cancel: SharedString,
    destructive: bool,
}

pub fn prompt(title: impl Into<SharedString>) -> Prompt {
    Prompt {
        title: title.into(),
        body: None,
        confirm: "Continue".into(),
        cancel: "Cancel".into(),
        destructive: false,
    }
}

impl Prompt {
    pub fn body(mut self, body: impl Into<SharedString>) -> Self {
        self.body = Some(body.into());
        self
    }

    pub fn confirm(mut self, label: impl Into<SharedString>) -> Self {
        self.confirm = label.into();
        self
    }

    pub fn cancel(mut self, label: impl Into<SharedString>) -> Self {
        self.cancel = label.into();
        self
    }

    pub fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    /// Opens the prompt. `on_confirm` runs once the person agrees.
    pub fn open(
        self,
        window: &mut Window,
        cx: &mut App,
        on_confirm: impl Fn(&mut Window, &mut App) + 'static,
    ) {
        self.open_with(
            window,
            cx,
            |_, _, _| None,
            move |window, cx| {
                on_confirm(window, cx);
                true
            },
            |_, _| {},
        );
    }

    /// Opens the prompt around extra content, such as a field. `on_confirm` returns
    /// whether the prompt may close; `on_cancel` runs when it is dismissed instead.
    pub fn open_with(
        self,
        window: &mut Window,
        cx: &mut App,
        content: impl Fn(Palette, &mut Window, &mut App) -> Option<AnyElement> + 'static,
        on_confirm: impl Fn(&mut Window, &mut App) -> bool + 'static,
        on_cancel: impl Fn(&mut Window, &mut App) + 'static,
    ) {
        let on_confirm = Rc::new(on_confirm);
        let on_cancel = Rc::new(on_cancel);
        window.open_dialog(cx, move |dialog, window, cx| {
            let p = theme::palette(cx);
            let spec = self.clone();
            let destructive = spec.destructive;
            let (click, enter) = (on_confirm.clone(), on_confirm.clone());
            let (dismiss, escape) = (on_cancel.clone(), on_cancel.clone());
            let confirm = if destructive {
                button::destructive("dialog-confirm")
            } else {
                button::primary("dialog-confirm")
            };
            dialog
                .w(px(WIDTH))
                .p(px(24.))
                .bg(p.raised)
                .border_color(p.border)
                .rounded(px(radius::PANEL))
                .close_button(false)
                .title(text::subtitle(spec.title).text_color(p.text))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(16.))
                        .children(
                            spec.body
                                .map(|body| text::body(body).text_color(p.text_muted)),
                        )
                        .children(content(p, window, cx)),
                )
                .footer(
                    div()
                        .flex()
                        .justify_end()
                        .gap(px(8.))
                        .child(button::quiet("dialog-cancel").label(spec.cancel).on_click(
                            move |_, window, cx| {
                                dismiss(window, cx);
                                window.close_dialog(cx);
                            },
                        ))
                        .child(confirm.label(spec.confirm).on_click(move |_, window, cx| {
                            if click(window, cx) {
                                window.close_dialog(cx);
                            }
                        })),
                )
                .on_ok(move |_, window, cx| !destructive && enter(window, cx))
                .on_cancel(move |_, window, cx| {
                    escape(window, cx);
                    true
                })
        });
    }
}
