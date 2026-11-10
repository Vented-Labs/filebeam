//! Containers and elevation: the layered surfaces every screen is built from.

use gpui::{
    Background, BoxShadow, Div, Hsla, Styled, div, linear_color_stop, linear_gradient, point, px,
};

use crate::theme::{
    Palette,
    tokens::{radius, space},
};

fn drop(color: Hsla, y: f32, blur: f32, spread: f32) -> BoxShadow {
    BoxShadow {
        color,
        offset: point(px(0.), px(y)),
        blur_radius: px(blur),
        spread_radius: px(spread),
        inset: false,
    }
}

/// `--fb-shadow-panel`: a soft lift for the main surfaces.
pub fn shadow_panel(p: Palette) -> Vec<BoxShadow> {
    if p.dark {
        vec![drop(gpui::black().opacity(0.16), 16., 40., 0.)]
    } else {
        vec![
            drop(p.shadow.opacity(0.06), 8., 24., 0.),
            drop(p.shadow.opacity(0.04), 1., 3., 0.),
        ]
    }
}

/// `--fb-shadow-popover`: menus, toasts and floating cards.
pub fn shadow_popover(p: Palette) -> Vec<BoxShadow> {
    if p.dark {
        vec![
            drop(gpui::black().opacity(0.36), 14., 32., 0.),
            drop(gpui::black().opacity(0.16), 2., 6., 0.),
        ]
    } else {
        vec![
            drop(p.shadow.opacity(0.10), 12., 28., 0.),
            drop(p.shadow.opacity(0.06), 2., 6., 0.),
        ]
    }
}

/// The accent glow under a primary action, with a top-lit edge in dark themes.
pub fn shadow_action(p: Palette) -> Vec<BoxShadow> {
    let mut shadows = vec![drop(
        p.action.opacity(if p.dark { 0.34 } else { 0.22 }),
        6.,
        16.,
        -2.,
    )];
    if p.highlight.a > 0.0 {
        shadows.push(BoxShadow {
            color: p.highlight,
            offset: point(px(0.), px(1.)),
            blur_radius: px(0.),
            spread_radius: px(0.),
            inset: true,
        });
    }
    shadows
}

/// A two-stop gradient running top to bottom.
pub fn vertical(from: Hsla, to: Hsla) -> Background {
    linear_gradient(180., linear_color_stop(from, 0.), linear_color_stop(to, 1.))
}

/// A two-stop gradient at a CSS angle (0 is up, clockwise).
pub fn angled(angle: f32, from: Hsla, to: Hsla) -> Background {
    linear_gradient(
        angle,
        linear_color_stop(from, 0.),
        linear_color_stop(to, 1.),
    )
}

/// Dark themes use a subtle top-lit gradient on primary actions; light themes stay flat.
pub fn action_fill(p: Palette) -> Background {
    if p.dark {
        vertical(p.action_top, p.action)
    } else {
        p.action.into()
    }
}

/// The 135 degree brand gradient, reduced to its outer stops.
pub fn brand_fill(p: Palette) -> Background {
    angled(135., p.brand_gradient[1], p.brand_gradient[4])
}

/// The standard raised container: surface fill, hairline border, soft shadow.
pub fn panel(p: Palette) -> Div {
    div()
        .bg(p.surface)
        .border_1()
        .border_color(p.border)
        .rounded(px(radius::PANEL))
        .shadow(shadow_panel(p))
}

/// A smaller grouped surface inside a page or panel.
pub fn card(p: Palette) -> Div {
    div()
        .bg(p.surface)
        .border_1()
        .border_color(p.border)
        .rounded(px(radius::CARD))
}

/// A recessed well for values, code and editors.
pub fn well(p: Palette) -> Div {
    div()
        .bg(p.sunken)
        .border_1()
        .border_color(p.border)
        .rounded(px(radius::CONTROL))
}

/// A recessed well that hugs a container of radius `outer`, for what a row opens into.
pub fn hugging_well(p: Palette, outer: f32) -> Div {
    div()
        .bg(p.sunken)
        .border_1()
        .border_color(p.border)
        .rounded(px(radius::nested(outer, space::HUG)))
}

/// A one-pixel separator in the palette's soft line colour.
pub fn hairline(p: Palette) -> Div {
    div().h(px(1.)).w_full().flex_none().bg(p.line_soft)
}

/// A vertical one-pixel separator.
pub fn hairline_vertical(p: Palette) -> Div {
    div().w(px(1.)).h_full().flex_none().bg(p.line_soft)
}
