use std::sync::Arc;

use filebeam_client_config::{Appearance, Theme as AppearanceTheme};
use gpui::{App, Rgba, px, rgb};
use gpui_component::theme::{Theme, ThemeColor, ThemeMode, ThemeTokens};

pub mod tokens;

#[derive(Clone, Copy)]
pub struct Palette {
    pub page: Rgba,
    pub chrome: Rgba,
    pub sidebar: Rgba,
    pub surface: Rgba,
    pub raised: Rgba,
    pub inset: Rgba,
    pub hover: Rgba,
    pub selected: Rgba,
    pub border: Rgba,
    pub border_soft: Rgba,
    pub text: Rgba,
    pub muted: Rgba,
    pub subtle: Rgba,
    pub accent: Rgba,
    pub primary: Rgba,
    pub primary_hover: Rgba,
    pub on_primary: Rgba,
    pub on_success: Rgba,
    pub on_warning: Rgba,
    pub on_danger: Rgba,
    pub focus: Rgba,
    pub success: Rgba,
    pub warning: Rgba,
    pub danger: Rgba,
    pub selection_text: Rgba,
}

macro_rules! palette {
    ($($role:ident: $hex:literal),+ $(,)?) => { Palette { $($role: rgb($hex)),+ } };
}

pub fn dark() -> Palette {
    palette!(
        page: 0x111016, chrome: 0x15131B, sidebar: 0x19161F, surface: 0x1C1823, raised: 0x231E2D,
        inset: 0x131117, hover: 0x2B2436, selected: 0x31233F, border: 0x342D3E, border_soft: 0x29242F,
        text: 0xF1EDF6, muted: 0xB4AABB, subtle: 0x92869C, accent: 0xBB97ED, primary: 0x8650CF,
        primary_hover: 0x9058D6, on_primary: 0xFFFFFF, on_success: 0x111016, on_warning: 0x111016, on_danger: 0x111016, focus: 0xC2A2F0, success: 0xA3D5BC,
        warning: 0xEDC49C, danger: 0xF0A5B2, selection_text: 0xE8D8FC,
    )
}

pub fn light() -> Palette {
    palette!(
        page: 0xF4F2F6, chrome: 0xFBFAFC, sidebar: 0xEFEDF2, surface: 0xFFFFFF, raised: 0xF6F3F9,
        inset: 0xF4F2F6, hover: 0xEAE5F0, selected: 0xECE2F8, border: 0xD9D2E1, border_soft: 0xE8E3EC,
        text: 0x292131, muted: 0x63586F, subtle: 0x766980, accent: 0x7040A8, primary: 0x7B43C2,
        primary_hover: 0x6D35B3, on_primary: 0xFFFFFF, on_success: 0xFFFFFF, on_warning: 0xFFFFFF, on_danger: 0xFFFFFF, focus: 0x743CBB, success: 0x27694B,
        warning: 0x855516, danger: 0xA4354D, selection_text: 0x582C87,
    )
}

pub fn palette_for(mode: ThemeMode) -> Palette {
    if mode.is_dark() { dark() } else { light() }
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
    let palette = palette_for(Theme::global(cx).mode);
    let theme = Theme::global_mut(cx);
    theme.colors = component_colors(palette);
    // Kit controls read resolved `ThemeTokens`, not just `ThemeColor`. Keeping
    // both projections in step avoids falling back to transparent/black defaults.
    theme.tokens = ThemeTokens::from(&theme.colors);
    apply_editor_colors(theme, palette);
    theme.radius = px(tokens::geometry::CONTROL_RADIUS);
    theme.radius_lg = px(tokens::geometry::STUDIO_RADIUS);
    // Operational errors remain scannable rather than obscuring the workspace.
    theme.notification.max_items = 3;
    theme.font_size = px(13.);
    theme.shadow = false;
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

fn apply_editor_colors(theme: &mut Theme, p: Palette) {
    let mut highlight_theme = (*theme.highlight_theme).clone();
    // Keep the selected syntax theme intact while mapping editor chrome to Prism.
    highlight_theme.style.editor_background = Some(p.inset.into());
    highlight_theme.style.editor_foreground = Some(p.text.into());
    highlight_theme.style.editor_gutter_background = Some(p.inset.into());
    highlight_theme.style.editor_active_line = Some(p.hover.into());
    theme.highlight_theme = Arc::new(highlight_theme);
}

fn component_colors(p: Palette) -> ThemeColor {
    let color = |value: Rgba| value.into();
    ThemeColor {
        accent: color(p.selected),
        accent_foreground: color(p.text),
        accordion: color(p.surface),
        background: color(p.page),
        border: color(p.border),
        button: color(p.raised),
        button_active: color(p.selected),
        button_foreground: color(p.text),
        button_hover: color(p.hover),
        button_danger: color(p.danger),
        button_danger_active: color(p.danger),
        button_danger_foreground: color(p.on_danger),
        button_danger_hover: color(p.danger),
        button_info: color(p.primary),
        button_info_active: color(p.primary),
        button_info_foreground: color(p.on_primary),
        button_info_hover: color(p.primary_hover),
        button_primary: color(p.primary),
        button_primary_active: color(p.primary_hover),
        button_primary_foreground: color(p.on_primary),
        button_primary_hover: color(p.primary_hover),
        button_secondary: color(p.raised),
        button_secondary_active: color(p.selected),
        button_secondary_foreground: color(p.text),
        button_secondary_hover: color(p.hover),
        button_success: color(p.success),
        button_success_active: color(p.success),
        button_success_foreground: color(p.on_success),
        button_success_hover: color(p.success),
        button_warning: color(p.warning),
        button_warning_active: color(p.warning),
        button_warning_foreground: color(p.on_warning),
        button_warning_hover: color(p.warning),
        group_box: color(p.surface),
        group_box_foreground: color(p.text),
        caret: color(p.primary),
        chart_1: color(p.primary),
        chart_2: color(p.accent),
        chart_3: color(p.success),
        chart_4: color(p.warning),
        chart_5: color(p.danger),
        danger: color(p.danger),
        danger_active: color(p.danger),
        danger_foreground: color(p.on_danger),
        danger_hover: color(p.danger),
        description_list_label: color(p.raised),
        description_list_label_foreground: color(p.muted),
        drag_border: color(p.focus),
        drop_target: color(p.selected),
        foreground: color(p.text),
        info: color(p.primary),
        info_active: color(p.primary),
        info_foreground: color(p.on_primary),
        info_hover: color(p.primary_hover),
        input: color(p.border),
        link: color(p.accent),
        link_active: color(p.primary),
        link_hover: color(p.primary_hover),
        list: color(p.surface),
        list_active: color(p.selected),
        list_active_border: color(p.focus),
        list_even: color(p.raised),
        list_head: color(p.chrome),
        list_hover: color(p.hover),
        muted: color(p.inset),
        muted_foreground: color(p.muted),
        popover: color(p.raised),
        popover_foreground: color(p.text),
        primary: color(p.primary),
        primary_active: color(p.primary_hover),
        primary_foreground: color(p.on_primary),
        primary_hover: color(p.primary_hover),
        progress_bar: color(p.primary),
        ring: color(p.focus),
        scrollbar: color(p.inset),
        scrollbar_thumb: color(p.subtle),
        scrollbar_thumb_hover: color(p.muted),
        secondary: color(p.raised),
        secondary_active: color(p.selected),
        secondary_foreground: color(p.text),
        secondary_hover: color(p.hover),
        selection: color(p.selected),
        sidebar: color(p.sidebar),
        sidebar_accent: color(p.selected),
        sidebar_accent_foreground: color(p.text),
        sidebar_border: color(p.border),
        sidebar_foreground: color(p.text),
        sidebar_primary: color(p.primary),
        sidebar_primary_foreground: color(p.on_primary),
        skeleton: color(p.inset),
        slider_bar: color(p.border_soft),
        slider_thumb: color(p.primary),
        success: color(p.success),
        success_foreground: color(p.on_success),
        success_hover: color(p.success),
        success_active: color(p.success),
        chart_bullish: color(p.success),
        chart_bearish: color(p.danger),
        switch: color(p.border),
        switch_thumb: color(p.on_primary),
        tab: color(p.chrome),
        tab_active: color(p.surface),
        tab_active_foreground: color(p.text),
        tab_bar: color(p.chrome),
        tab_bar_segmented: color(p.raised),
        tab_foreground: color(p.muted),
        table: color(p.surface),
        table_active: color(p.selected),
        table_active_border: color(p.focus),
        table_even: color(p.raised),
        table_head: color(p.chrome),
        table_head_foreground: color(p.muted),
        table_foot: color(p.raised),
        table_foot_foreground: color(p.muted),
        table_hover: color(p.hover),
        table_row_border: color(p.border_soft),
        title_bar: color(p.chrome),
        title_bar_border: color(p.border),
        status_bar: color(p.chrome),
        status_bar_border: color(p.border),
        tiles: color(p.surface),
        warning: color(p.warning),
        warning_active: color(p.warning),
        warning_hover: color(p.warning),
        warning_foreground: color(p.on_warning),
        overlay: color(p.inset),
        window_border: color(p.border),
        red: color(p.danger),
        red_light: color(p.danger),
        green: color(p.success),
        green_light: color(p.success),
        blue: color(p.primary),
        blue_light: color(p.accent),
        yellow: color(p.warning),
        yellow_light: color(p.warning),
        magenta: color(p.primary),
        magenta_light: color(p.accent),
        cyan: color(p.accent),
        cyan_light: color(p.accent),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn semantic_palettes_expose_every_contract_role() {
        assert_ne!(dark().border_soft, dark().border);
        assert_ne!(light().focus, light().primary);
    }

    #[test]
    fn controls_use_distinct_readable_track_and_foreground_tokens() {
        let dark_colors = component_colors(dark());
        let light_colors = component_colors(light());

        assert_ne!(dark_colors.switch, dark_colors.background);
        assert_ne!(light_colors.switch, light_colors.background);
        assert_ne!(dark_colors.switch_thumb, dark_colors.switch);
        assert_ne!(light_colors.switch_thumb, light_colors.switch);
        assert_eq!(dark_colors.switch_thumb, dark().on_primary.into());
        assert_eq!(light_colors.switch_thumb, light().on_primary.into());
        assert_ne!(dark_colors.success_foreground, dark_colors.success);
        assert_ne!(light_colors.warning_foreground, light_colors.warning);
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
            assert_eq!(Theme::global(cx).mode, ThemeMode::Light);
            assert_eq!(
                Theme::global(cx).tokens.background.color,
                light().page.into()
            );

            apply(
                &Appearance {
                    theme: AppearanceTheme::Dark,
                    reduced_motion: false,
                },
                cx,
            );
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
            assert_eq!(theme.tokens.background.color, light().page.into());
            assert_eq!(theme.tokens.switch.color, light().border.into());
            assert_eq!(theme.tokens.primary.color, resolved.primary.color);
            assert_eq!(theme.tokens.switch_thumb.color, resolved.switch_thumb.color);
            assert_eq!(theme.motion.duration_fast, std::time::Duration::ZERO);
        });
    }
}
