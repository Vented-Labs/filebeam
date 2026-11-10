//! The desktop theme: one palette, derived by the shared engine, projected onto
//! GPUI colours and onto the component library's theme.

use std::sync::Arc;

use filebeam_client_config::{Appearance, Theme as AppearanceTheme};
use gpui::{App, Global, Hsla, px};
use gpui_component::theme::{Theme, ThemeColor, ThemeMode, ThemeTokens};

pub mod color;
pub mod engine;
pub mod tokens;

pub use engine::Preset;

/// Where the accent and surfaces come from. Stored per device, never in the shared
/// configuration, so the CLI and other clients are unaffected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorSource {
    Preset(Preset),
    /// A colour picked by the user, packed as `0xRRGGBB`.
    Custom(color::Rgb),
}

impl Default for ColorSource {
    fn default() -> Self {
        Self::Preset(Preset::Purple)
    }
}

impl ColorSource {
    pub fn encoded(self) -> String {
        match self {
            Self::Preset(preset) => format!("preset:{}", preset.id()),
            Self::Custom(seed) => format!("custom:{:06x}", seed & 0xFF_FFFF),
        }
    }

    /// Unknown or malformed values fall back to the default preset.
    pub fn decode(value: Option<&str>) -> Self {
        let Some(value) = value else {
            return Self::default();
        };
        if let Some(id) = value.strip_prefix("preset:") {
            return Preset::from_id(id).map_or_else(Self::default, Self::Preset);
        }
        value
            .strip_prefix("custom:")
            .and_then(color::parse)
            .map_or_else(Self::default, Self::Custom)
    }

    pub fn seed(self) -> color::Rgb {
        match self {
            Self::Preset(preset) => preset.seed(),
            Self::Custom(seed) => seed & 0xFF_FFFF,
        }
    }

    fn tokens(self, dark: bool) -> engine::Tokens {
        match self {
            Self::Preset(preset) => engine::preset(preset, dark),
            Self::Custom(seed) => engine::seeded(seed, dark),
        }
    }
}

/// Ink, tinted surface and border for one feedback tone.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Status {
    pub ink: Hsla,
    pub surface: Hsla,
    pub border: Hsla,
}

/// Every colour the app draws with. Role names follow the web tokens (`--fb-*`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub dark: bool,
    pub chrome: Hsla,
    pub background: Hsla,
    pub surface: Hsla,
    pub raised: Hsla,
    pub sunken: Hsla,
    pub border: Hsla,
    pub control_border: Hsla,
    pub control_border_hover: Hsla,
    pub hover: Hsla,
    pub active: Hsla,
    pub text: Hsla,
    pub text_muted: Hsla,
    pub text_subtle: Hsla,
    pub disabled_border: Hsla,
    pub disabled_surface: Hsla,
    pub disabled_text: Hsla,
    pub accent_text: Hsla,
    pub accent_surface: Hsla,
    pub selected_surface: Hsla,
    pub action: Hsla,
    pub action_top: Hsla,
    pub action_hover: Hsla,
    pub action_pressed: Hsla,
    pub on_action: Hsla,
    pub action_border: Hsla,
    pub choice_border: Hsla,
    pub progress_fill: Hsla,
    pub progress_track: Hsla,
    pub success: Status,
    pub warning: Status,
    pub danger: Status,
    pub info: Status,
    pub scrim: Hsla,
    pub line_soft: Hsla,
    pub line_strong: Hsla,
    pub shadow: Hsla,
    pub highlight: Hsla,
    pub sheen: Hsla,
    pub brand: Hsla,
    pub brand_gradient: [Hsla; 5],
    pub glow_primary: Hsla,
    pub glow_secondary: Hsla,
    pub card_back: Hsla,
    pub card_back_end: Hsla,
    pub card_front: Hsla,
    pub card_front_end: Hsla,
}

fn opaque(value: color::Rgb) -> Hsla {
    gpui::rgb(value).into()
}

fn translucent(value: engine::Alpha) -> Hsla {
    let mut color = opaque(value.rgb);
    color.a = value.alpha as f32;
    color
}

impl From<&engine::Tokens> for Palette {
    fn from(tokens: &engine::Tokens) -> Self {
        let status = |roles: engine::Status| Status {
            ink: opaque(roles.ink),
            surface: opaque(roles.surface),
            border: opaque(roles.border),
        };
        Self {
            dark: tokens.dark,
            chrome: opaque(tokens.chrome),
            background: opaque(tokens.background),
            surface: opaque(tokens.surface),
            raised: opaque(tokens.raised),
            sunken: opaque(tokens.sunken),
            border: opaque(tokens.border),
            control_border: opaque(tokens.control_border),
            control_border_hover: opaque(tokens.control_border_hover),
            hover: opaque(tokens.hover),
            active: opaque(tokens.active),
            text: opaque(tokens.text),
            text_muted: opaque(tokens.text_muted),
            text_subtle: opaque(tokens.text_subtle),
            disabled_border: opaque(tokens.disabled_border),
            disabled_surface: opaque(tokens.disabled_surface),
            disabled_text: opaque(tokens.disabled_text),
            accent_text: opaque(tokens.accent_text),
            accent_surface: opaque(tokens.accent_surface),
            selected_surface: opaque(tokens.selected_surface),
            action: opaque(tokens.action),
            action_top: opaque(tokens.action_top),
            action_hover: opaque(tokens.action_hover),
            action_pressed: opaque(tokens.action_pressed),
            on_action: opaque(tokens.on_action),
            action_border: opaque(tokens.action_border),
            choice_border: opaque(tokens.choice_border),
            progress_fill: opaque(tokens.progress_fill),
            progress_track: opaque(tokens.progress_track),
            success: status(tokens.success),
            warning: status(tokens.warning),
            danger: status(tokens.danger),
            info: status(tokens.info),
            scrim: translucent(tokens.scrim),
            line_soft: translucent(tokens.line_soft),
            line_strong: translucent(tokens.line_strong),
            shadow: translucent(tokens.shadow),
            highlight: translucent(tokens.highlight),
            sheen: translucent(tokens.sheen),
            brand: opaque(tokens.brand),
            brand_gradient: tokens.brand_gradient.map(opaque),
            glow_primary: translucent(tokens.glow_primary),
            glow_secondary: translucent(tokens.glow_secondary),
            card_back: opaque(tokens.card_back),
            card_back_end: opaque(tokens.card_back_end),
            card_front: opaque(tokens.card_front),
            card_front_end: opaque(tokens.card_front_end),
        }
    }
}

/// The resolved look for the running app.
#[derive(Clone)]
struct Active {
    appearance: Appearance,
    source: ColorSource,
    palette: Palette,
    tokens: Arc<engine::Tokens>,
}

impl Global for Active {}

/// The colour source chosen on this device. Applied by the next [`apply`].
struct ChosenSource(ColorSource);

impl Global for ChosenSource {}

/// The palette every view draws with. Falls back to the default preset when the
/// theme has not been applied yet, as in component tests.
pub fn palette(cx: &App) -> Palette {
    if let Some(active) = cx.try_global::<Active>() {
        return active.palette;
    }
    let dark = cx
        .try_global::<Theme>()
        .is_none_or(|theme| theme.mode.is_dark());
    Palette::from(&ColorSource::default().tokens(dark))
}

/// Raw engine output for the active look, for roles GPUI colours cannot express.
pub fn tokens(cx: &App) -> Arc<engine::Tokens> {
    cx.try_global::<Active>().map_or_else(
        || Arc::new(ColorSource::default().tokens(true)),
        |active| active.tokens.clone(),
    )
}

pub fn color_source(cx: &App) -> ColorSource {
    cx.try_global::<ChosenSource>()
        .map_or_else(ColorSource::default, |chosen| chosen.0)
}

/// The palette `source` produces in one mode, for previews of a look that is not
/// the active one.
pub fn preview(source: ColorSource, dark: bool) -> Palette {
    Palette::from(&source.tokens(dark))
}

/// Records the device's colour choice. Call [`apply`] afterwards to repaint.
pub fn set_color_source(source: ColorSource, cx: &mut App) {
    cx.set_global(ChosenSource(source));
}

/// Applies the configured appearance after `gpui_component::init`. System remains the default.
pub fn apply(appearance: &Appearance, cx: &mut App) {
    // GPUI Base consumes this global for native component and layered-sheet motion.
    // Kit 0.6.1 does not expose an OS accessibility query to applications, so the
    // configured preference is the only value this runtime can set explicitly.
    cx.set_reduce_motion(appearance.reduced_motion);
    match appearance.theme {
        AppearanceTheme::System => Theme::sync_system_appearance(None, cx),
        AppearanceTheme::Light => Theme::change(ThemeMode::Light, None, cx),
        AppearanceTheme::Dark => Theme::change(ThemeMode::Dark, None, cx),
    }
    let source = color_source(cx);
    let tokens = Arc::new(source.tokens(Theme::global(cx).mode.is_dark()));
    let palette = Palette::from(tokens.as_ref());
    cx.set_global(Active {
        appearance: appearance.clone(),
        source,
        palette,
        tokens: tokens.clone(),
    });

    let theme = Theme::global_mut(cx);
    theme.colors = component_colors(palette);
    // Kit controls read resolved `ThemeTokens`, not just `ThemeColor`. Keeping
    // both projections in step avoids falling back to transparent/black defaults.
    theme.tokens = ThemeTokens::from(&theme.colors);
    apply_editor_colors(theme, &tokens.editor);
    // The component library draws menu rows at `SMALL`, `INSET` inside the menu, and
    // rounds the menu itself by this value; this is the radius that keeps them concentric.
    theme.radius = px(tokens::radius::around(
        tokens::radius::SMALL,
        tokens::space::INSET,
    ));
    theme.radius_lg = px(tokens::radius::CARD);
    // Operational errors remain scannable rather than obscuring the workspace.
    theme.notification.max_items = 3;
    theme.font_family = crate::ui::text::SANS.into();
    theme.mono_font_family = crate::ui::text::MONO.into();
    theme.font_size = px(tokens::text::BODY);
    theme.mono_font_size = px(tokens::text::SMALL);
    theme.shadow = true;
    theme.motion.duration_fast = tokens::motion::duration(
        tokens::motion::CONTROLS_MS,
        false,
        appearance.reduced_motion,
    );
    theme.motion.duration_normal =
        tokens::motion::duration(tokens::motion::SWITCH_MS, false, appearance.reduced_motion);
    theme.motion.duration_slow =
        tokens::motion::duration(tokens::motion::PANEL_MS, false, appearance.reduced_motion);
    Theme::sync_base(cx);
    cx.refresh_windows();
}

/// Re-resolves the look after the operating system switched between light and dark.
/// Does nothing unless the configured theme follows the system.
pub fn follow_system(cx: &mut App) {
    let Some(appearance) = cx
        .try_global::<Active>()
        .map(|active| active.appearance.clone())
    else {
        return;
    };
    if appearance.theme == AppearanceTheme::System {
        apply(&appearance, cx);
    }
}

/// Whether the applied look was resolved from `source`. Used to skip redundant work
/// when an observer fires without a change.
pub fn is_applied(source: ColorSource, dark: bool, cx: &App) -> bool {
    cx.try_global::<Active>()
        .is_some_and(|active| active.source == source && active.palette.dark == dark)
}

fn apply_editor_colors(theme: &mut Theme, editor: &engine::Editor) {
    let mut highlight_theme = (*theme.highlight_theme).clone();
    // Keep the selected syntax theme intact while mapping editor chrome to the palette.
    highlight_theme.style.editor_background = Some(opaque(editor.background));
    highlight_theme.style.editor_foreground = Some(opaque(editor.text));
    highlight_theme.style.editor_gutter_background = Some(opaque(editor.gutter_background));
    highlight_theme.style.editor_active_line = Some(translucent(editor.active_line));
    theme.highlight_theme = Arc::new(highlight_theme);
}

fn component_colors(p: Palette) -> ThemeColor {
    // Filled status controls put the page colour on the status ink, which is light in
    // dark themes and dark in light themes.
    let on_status = if p.dark { p.background } else { p.raised };
    let selection = p.action.opacity(if p.dark { 0.42 } else { 0.24 });
    ThemeColor {
        accent: p.hover,
        accent_foreground: p.text,
        accordion: p.surface,
        background: p.background,
        border: p.border,
        button: p.surface,
        button_active: p.active,
        button_foreground: p.text,
        button_hover: p.hover,
        button_danger: p.danger.ink,
        button_danger_active: p.danger.border,
        button_danger_foreground: on_status,
        button_danger_hover: p.danger.border,
        button_info: p.info.ink,
        button_info_active: p.info.border,
        button_info_foreground: on_status,
        button_info_hover: p.info.border,
        button_primary: p.action,
        button_primary_active: p.action_pressed,
        button_primary_foreground: p.on_action,
        button_primary_hover: p.action_hover,
        button_secondary: p.surface,
        button_secondary_active: p.active,
        button_secondary_foreground: p.text,
        button_secondary_hover: p.hover,
        button_success: p.success.ink,
        button_success_active: p.success.border,
        button_success_foreground: on_status,
        button_success_hover: p.success.border,
        button_warning: p.warning.ink,
        button_warning_active: p.warning.border,
        button_warning_foreground: on_status,
        button_warning_hover: p.warning.border,
        group_box: p.surface,
        group_box_foreground: p.text,
        caret: p.accent_text,
        chart_1: p.brand_gradient[2],
        chart_2: p.brand_gradient[4],
        chart_3: p.success.ink,
        chart_4: p.warning.ink,
        chart_5: p.danger.ink,
        danger: p.danger.ink,
        danger_active: p.danger.border,
        danger_foreground: on_status,
        danger_hover: p.danger.border,
        description_list_label: p.sunken,
        description_list_label_foreground: p.text_muted,
        drag_border: p.choice_border,
        drop_target: p.selected_surface.opacity(0.6),
        foreground: p.text,
        info: p.info.ink,
        info_active: p.info.border,
        info_foreground: on_status,
        info_hover: p.info.border,
        input: p.control_border,
        link: p.accent_text,
        link_active: p.accent_text,
        link_hover: p.text,
        list: p.surface,
        list_active: p.selected_surface,
        list_active_border: p.choice_border,
        list_even: p.surface,
        list_head: p.sunken,
        list_hover: p.hover,
        muted: p.sunken,
        muted_foreground: p.text_muted,
        popover: p.raised,
        popover_foreground: p.text,
        primary: p.action,
        primary_active: p.action_pressed,
        primary_foreground: p.on_action,
        primary_hover: p.action_hover,
        progress_bar: p.progress_fill,
        ring: p.choice_border,
        scrollbar: gpui::transparent_black(),
        scrollbar_thumb: p.control_border.opacity(0.45),
        scrollbar_thumb_hover: p.control_border_hover.opacity(0.7),
        secondary: p.surface,
        secondary_active: p.active,
        secondary_foreground: p.text,
        secondary_hover: p.hover,
        selection,
        sidebar: p.chrome,
        sidebar_accent: p.selected_surface,
        sidebar_accent_foreground: p.text,
        sidebar_border: p.border,
        sidebar_foreground: p.text,
        sidebar_primary: p.action,
        sidebar_primary_foreground: p.on_action,
        skeleton: p.hover,
        slider_bar: p.progress_track,
        slider_thumb: p.action,
        success: p.success.ink,
        success_foreground: on_status,
        success_hover: p.success.border,
        success_active: p.success.border,
        chart_bullish: p.success.ink,
        chart_bearish: p.danger.ink,
        switch: p.control_border,
        switch_thumb: gpui::white(),
        tab: p.chrome,
        tab_active: p.surface,
        tab_active_foreground: p.text,
        tab_bar: p.chrome,
        tab_bar_segmented: p.sunken,
        tab_foreground: p.text_muted,
        table: p.surface,
        table_active: p.selected_surface,
        table_active_border: p.choice_border,
        table_even: p.surface,
        table_head: p.sunken,
        table_head_foreground: p.text_muted,
        table_foot: p.sunken,
        table_foot_foreground: p.text_muted,
        table_hover: p.hover,
        table_row_border: p.line_soft,
        title_bar: p.chrome,
        title_bar_border: p.line_soft,
        status_bar: p.chrome,
        status_bar_border: p.line_soft,
        tiles: p.surface,
        warning: p.warning.ink,
        warning_active: p.warning.border,
        warning_hover: p.warning.border,
        warning_foreground: on_status,
        overlay: p.scrim,
        window_border: p.border,
        red: p.danger.ink,
        red_light: p.danger.border,
        green: p.success.ink,
        green_light: p.success.border,
        blue: p.info.ink,
        blue_light: p.info.border,
        yellow: p.warning.ink,
        yellow_light: p.warning.border,
        magenta: p.brand_gradient[4],
        magenta_light: p.accent_text,
        cyan: p.info.ink,
        cyan_light: p.info.border,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn palette_for(source: ColorSource, dark: bool) -> Palette {
        Palette::from(&source.tokens(dark))
    }

    #[test]
    fn colour_sources_round_trip_and_reject_unknown_values() {
        for source in [
            ColorSource::Preset(Preset::Teal),
            ColorSource::Custom(0x12ab9f),
            ColorSource::default(),
        ] {
            assert_eq!(ColorSource::decode(Some(&source.encoded())), source);
        }
        assert_eq!(ColorSource::decode(None), ColorSource::default());
        assert_eq!(
            ColorSource::decode(Some("preset:nope")),
            ColorSource::default()
        );
        assert_eq!(
            ColorSource::decode(Some("custom:zzzzzz")),
            ColorSource::default()
        );
    }

    #[test]
    fn controls_use_distinct_readable_track_and_foreground_tokens() {
        for dark in [true, false] {
            let colors = component_colors(palette_for(ColorSource::default(), dark));
            assert_ne!(colors.switch, colors.background);
            assert_ne!(colors.switch_thumb, colors.switch);
            assert_ne!(colors.success_foreground, colors.success);
            assert_ne!(colors.warning_foreground, colors.warning);
            assert_ne!(colors.primary_foreground, colors.primary);
            assert!(
                colors.overlay.a < 1.0,
                "the dialog scrim must stay translucent"
            );
        }
    }

    #[gpui::test]
    fn apply_replaces_resolved_tokens_when_switching_back_to_light(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            apply(
                &Appearance {
                    theme: AppearanceTheme::Light,
                    reduced_motion: false,
                },
                cx,
            );
            let light = palette_for(ColorSource::default(), false);
            assert_eq!(Theme::global(cx).mode, ThemeMode::Light);
            assert_eq!(Theme::global(cx).tokens.background.color, light.background);
            assert_eq!(palette(cx), light);

            apply(
                &Appearance {
                    theme: AppearanceTheme::Dark,
                    reduced_motion: false,
                },
                cx,
            );
            assert!(palette(cx).dark);
            apply(
                &Appearance {
                    theme: AppearanceTheme::Light,
                    reduced_motion: true,
                },
                cx,
            );

            let theme = Theme::global(cx);
            let resolved = ThemeTokens::from(&theme.colors);
            assert_eq!(theme.mode, ThemeMode::Light);
            assert_eq!(theme.tokens.background.color, light.background);
            assert_eq!(theme.tokens.switch.color, light.control_border);
            assert_eq!(theme.tokens.primary.color, resolved.primary.color);
            assert_eq!(theme.tokens.switch_thumb.color, resolved.switch_thumb.color);
            assert_eq!(theme.motion.duration_fast, std::time::Duration::ZERO);
        });
    }

    #[gpui::test]
    fn the_chosen_colour_source_drives_the_applied_palette(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            let appearance = Appearance {
                theme: AppearanceTheme::Dark,
                reduced_motion: true,
            };
            apply(&appearance, cx);
            let purple = palette(cx);

            set_color_source(ColorSource::Preset(Preset::Teal), cx);
            apply(&appearance, cx);
            let teal = palette(cx);
            assert_ne!(teal.action, purple.action);
            assert_eq!(teal, palette_for(ColorSource::Preset(Preset::Teal), true));
            assert_eq!(Theme::global(cx).colors.primary, teal.action);
            assert!(is_applied(ColorSource::Preset(Preset::Teal), true, cx));
            assert!(!is_applied(ColorSource::default(), true, cx));
        });
    }
}
