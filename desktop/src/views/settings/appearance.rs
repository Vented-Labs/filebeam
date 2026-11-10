//! How the app looks on this device: light or dark, the colour everything is derived
//! from, and whether things move.

use filebeam_client_config::Theme as ConfigTheme;
use gpui::{
    AnyElement, Context, Hsla, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};
use gpui_component::tooltip::Tooltip;

use super::{Area, SettingsPanel};
use crate::{
    assets::icon,
    theme::{
        self, ColorSource, Palette, color,
        engine::Preset,
        tokens::{radius, space},
    },
    ui::{Tone, button, field, row, surface, text},
};

const THEMES: [(ConfigTheme, &str, &str); 3] = [
    (ConfigTheme::System, "system", "Match system"),
    (ConfigTheme::Light, "light", "Light"),
    (ConfigTheme::Dark, "dark", "Dark"),
];

fn swatch(rgb: color::Rgb) -> Hsla {
    gpui::rgb(rgb).into()
}

/// A theme card hugs its panel, and its preview sits this far inside the card.
const THEME_CARD: f32 = radius::nested(radius::PANEL, space::HUG);
const THEME_CARD_PAD: f32 = 9.;

/// A miniature window in one mode, drawn from the palette it would really use.
fn miniature(q: Palette) -> gpui::Div {
    div()
        .flex_1()
        .h_full()
        .flex()
        .bg(q.background)
        .child(
            div()
                .w(px(26.))
                .h_full()
                .flex_none()
                .bg(q.chrome)
                .border_r_1()
                .border_color(q.border)
                .p(px(5.))
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(div().h(px(5.)).rounded(px(2.)).bg(q.selected_surface))
                .child(div().h(px(5.)).rounded(px(2.)).bg(q.hover))
                .child(div().h(px(5.)).rounded(px(2.)).bg(q.hover)),
        )
        .child(
            div()
                .flex_1()
                .p(px(8.))
                .flex()
                .flex_col()
                .gap(px(6.))
                .child(
                    div()
                        .w(px(36.))
                        .h(px(6.))
                        .rounded(px(3.))
                        .bg(q.text.opacity(0.85)),
                )
                .child(
                    div()
                        .flex_1()
                        .rounded(px(5.))
                        .bg(q.surface)
                        .border_1()
                        .border_color(q.border)
                        .p(px(6.))
                        .flex()
                        .items_end()
                        .justify_end()
                        .child(div().w(px(30.)).h(px(10.)).rounded(px(4.)).bg(q.action)),
                ),
        )
}

impl SettingsPanel {
    fn select_theme(&mut self, theme: ConfigTheme, cx: &mut Context<Self>) {
        if self.theme != theme {
            self.theme = theme;
            theme::apply(&self.current_appearance(), cx);
            self.save_appearance(cx);
            cx.notify();
        }
    }

    fn set_reduced_motion(&mut self, reduced_motion: bool, cx: &mut Context<Self>) {
        self.reduced_motion = reduced_motion;
        theme::apply(&self.current_appearance(), cx);
        self.save_appearance(cx);
        cx.notify();
    }

    /// Repaints with `source` and remembers it for this device only.
    pub(super) fn select_source(&mut self, source: ColorSource, cx: &mut Context<Self>) {
        if theme::color_source(cx) == source {
            return;
        }
        theme::set_color_source(source, cx);
        theme::apply(&self.current_appearance(), cx);
        self.clear_notice(Area::Appearance, cx);
        if self.home.as_os_str().is_empty() {
            return;
        }
        let home = self.home.clone();
        let encoded = source.encoded();
        let executor = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let saved = executor
                .spawn(async move {
                    let mut state = crate::platform::preferences::load(&home);
                    state.accent = Some(encoded);
                    crate::platform::preferences::save(&home, &state)
                })
                .await;
            if let Err(error) = saved {
                let _ = this.update(cx, |this, cx| {
                    this.say(
                        Area::Appearance,
                        Tone::Danger,
                        format!("The colour applies now but could not be saved: {error}"),
                        cx,
                    );
                });
            }
        })
        .detach();
        cx.notify();
    }

    pub(super) fn apply_custom_color(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        match color::parse(self.custom_color.read(cx).value().trim()) {
            Some(seed) => self.select_source(ColorSource::Custom(seed), cx),
            None => self.say(
                Area::Appearance,
                Tone::Danger,
                "Enter a colour as six hex digits, such as #2f6feb.",
                cx,
            ),
        }
    }

    fn theme_card(
        &self,
        theme: ConfigTheme,
        id: &'static str,
        label: &'static str,
        p: Palette,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let selected = self.theme == theme;
        let source = theme::color_source(cx);
        let frame = div()
            .h(px(86.))
            .flex()
            .rounded(px(radius::nested(THEME_CARD, THEME_CARD_PAD + 1.)))
            .overflow_hidden()
            .border_1()
            .border_color(p.border);
        let frame = match theme {
            ConfigTheme::System => frame
                .child(miniature(theme::preview(source, false)))
                .child(miniature(theme::preview(source, true))),
            ConfigTheme::Light => frame.child(miniature(theme::preview(source, false))),
            ConfigTheme::Dark => frame.child(miniature(theme::preview(source, true))),
        };
        div()
            .id(SharedString::from(format!("appearance-{id}")))
            .flex_1()
            .min_w(px(150.))
            .p(px(THEME_CARD_PAD))
            .flex()
            .flex_col()
            .gap(px(10.))
            .rounded(px(THEME_CARD))
            .border_1()
            .cursor_pointer()
            .when(selected, |this| {
                this.bg(p.selected_surface).border_color(p.choice_border)
            })
            .when(!selected, |this| {
                this.border_color(p.border)
                    .hover(move |style| style.bg(p.hover))
            })
            .child(frame)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .size(px(16.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .border_1()
                            .border_color(if selected {
                                p.choice_border
                            } else {
                                p.control_border
                            })
                            .when(selected, |this| {
                                this.child(div().size(px(8.)).rounded_full().bg(p.action))
                            }),
                    )
                    .child(text::small_strong(label).text_color(if selected {
                        p.text
                    } else {
                        p.text_muted
                    })),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.select_theme(theme, cx)))
    }

    pub(super) fn appearance(&mut self, p: Palette, cx: &mut Context<Self>) -> AnyElement {
        let source = theme::color_source(cx);
        let custom = matches!(source, ColorSource::Custom(_));
        let draft = color::parse(self.custom_color.read(cx).value().trim());
        let motion = cx.entity().downgrade();

        let dot =
            |id: SharedString, title: &'static str, fill: gpui::Background, selected: bool| {
                div()
                    .id(id)
                    .size(px(44.))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .border_2()
                    .border_color(if selected {
                        p.choice_border
                    } else {
                        gpui::transparent_black()
                    })
                    .cursor_pointer()
                    .tooltip(move |window, cx| Tooltip::new(title).build(window, cx))
                    .child(
                        div()
                            .size(px(32.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .bg(fill)
                            .when(selected, |this| {
                                this.child(div().size(px(10.)).rounded_full().bg(gpui::white()))
                            }),
                    )
            };
        let presets: Vec<_> = Preset::ALL
            .into_iter()
            .map(|preset| {
                dot(
                    SharedString::from(format!("accent-{}", preset.id())),
                    preset.title(),
                    surface::angled(135., swatch(preset.seed()), swatch(preset.endpoint())),
                    source == ColorSource::Preset(preset),
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.select_source(ColorSource::Preset(preset), cx)
                }))
            })
            .collect();
        let themes: Vec<_> = THEMES
            .into_iter()
            .map(|(theme, id, label)| self.theme_card(theme, id, label, p, cx))
            .collect();

        div()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(16.))
            .children(self.notice(Area::Appearance, p))
            .child(
                surface::panel(p)
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .px(px(22.))
                            .pt(px(22.))
                            .pb(px(14.))
                            .child(text::eyebrow("Theme").text_color(p.text_subtle)),
                    )
                    // The cards hug the panel, the border being the first pixel of the gap.
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(space::HUG))
                            .p(px(space::HUG - 1.))
                            .children(themes),
                    ),
            )
            .child(
                surface::panel(p)
                    .p(px(22.))
                    .flex()
                    .flex_col()
                    .gap(px(14.))
                    .child(text::eyebrow("Colour").text_color(p.text_subtle))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .gap(px(6.))
                            .children(presets)
                            .child(
                                div()
                                    .id("accent-custom")
                                    .h(px(44.))
                                    .px(px(14.))
                                    .ml(px(6.))
                                    .flex()
                                    .items_center()
                                    .gap(px(8.))
                                    .rounded_full()
                                    .border_1()
                                    .cursor_pointer()
                                    .when(custom, |this| {
                                        this.bg(p.selected_surface).border_color(p.choice_border)
                                    })
                                    .when(!custom, |this| {
                                        this.border_color(p.control_border)
                                            .hover(move |style| style.bg(p.hover))
                                    })
                                    .child(icon("color").size(px(16.)).text_color(p.text_muted))
                                    .child(text::small_strong("Custom").text_color(p.text))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.custom_color
                                            .update(cx, |input, cx| input.focus(window, cx));
                                        if color::parse(this.custom_color.read(cx).value().trim())
                                            .is_some()
                                        {
                                            this.apply_custom_color(window, cx);
                                        }
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(
                                div().w(px(180.)).flex_none().child(
                                    field::input(p, &self.custom_color)
                                        .id("accent-custom-value")
                                        .prefix(
                                            div()
                                                .size(px(16.))
                                                .rounded(px(4.))
                                                .border_1()
                                                .border_color(p.control_border)
                                                .when_some(draft, |this, seed| {
                                                    this.bg(swatch(seed))
                                                }),
                                        ),
                                ),
                            )
                            .child(
                                button::secondary("accent-custom-apply")
                                    .label("Use this colour")
                                    .disabled(draft.is_none())
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.apply_custom_color(window, cx)
                                    })),
                            ),
                    )
                    .child(field::hint(
                        p,
                        "Surfaces, text and controls are derived from the colour and adjusted to \
                         stay readable. It applies to this device only.",
                        Tone::Neutral,
                    )),
            )
            .child(
                surface::panel(p)
                    .p(px(22.))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(text::eyebrow("Motion").text_color(p.text_subtle))
                    .child(
                        row::switch_row("reduced-motion", "Reduce motion", self.reduced_motion)
                            .description(
                                "Pages and controls change without sliding, springing or fading.",
                            )
                            .on_toggle(move |on, _, cx| {
                                let on = *on;
                                let _ = motion.update(cx, |this, cx| {
                                    this.set_reduced_motion(on, cx);
                                });
                            }),
                    ),
            )
            .into_any_element()
    }
}
