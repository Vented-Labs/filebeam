//! Brand artwork drawn from the active palette: the Filebeam mark, the stacked-card
//! emblem, avatars and the ambient page backdrop. Nothing here is a bitmap, so every
//! piece follows presets and custom colours.

use std::sync::LazyLock;

use gpui::{
    App, Background, Bounds, BoxShadow, Div, FontWeight, Hsla, IntoElement, ParentElement,
    PathBuilder, Pixels, Point, RenderOnce, Rgba, SharedString, Styled, Window, canvas, div, point,
    prelude::FluentBuilder, px, relative,
};

use crate::{
    assets::icon,
    theme::{self, Palette},
    ui::{motion, surface},
};

/// Geometry of the folded "F and beam" mark, from `backend/public/brand/filebeam-mark.svg`.
const MARK_WIDTH: f32 = 168.;
const MARK_HEIGHT: f32 = 244.;
const MARK_BODY: &str = "M14 65L140 3C153 -3 168 5 168 20L168 42C168 57 165 63 153 69L81 105L145 134L79 166C90 171 93 178 93 191L93 225C93 239 84 248 70 242L16 216C5 211 0 203 0 191L0 87C0 77 5 69 14 65Z";
const MARK_FOLD: &str =
    "M14 65C5 69 0 77 0 87L0 158C0 150 4 145 12 141L81 105L14 73C8 70 8 68 14 65Z";
const MARK_TOP: &str =
    "M14 65L140 3C153 -3 168 5 168 20L168 42C168 57 165 63 153 69L81 105L14 73C8 70 8 68 14 65Z";
const MARK_BEAM: &str = "M12 141L81 105L145 134L79 166Z";
const MARK_STEM: &str = "M12 141L79 166C90 171 93 178 93 191L93 225C93 239 84 248 70 242L16 216C5 211 0 203 0 191L0 158C0 150 4 145 12 141Z";

#[derive(Clone, Copy, Debug, PartialEq)]
enum Segment {
    Move(f32, f32),
    Line(f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}

/// Parses the absolute M, L, C and Z commands the brand paths use.
fn parse_path(data: &str) -> Vec<Segment> {
    let mut segments = Vec::new();
    let mut numbers: Vec<f32> = Vec::new();
    let mut command = 'M';
    let mut token = String::new();
    let flush = |token: &mut String, numbers: &mut Vec<f32>| {
        if let Ok(value) = token.parse::<f32>() {
            numbers.push(value);
        }
        token.clear();
    };
    let apply = |command: char, numbers: &mut Vec<f32>, segments: &mut Vec<Segment>| {
        match (command, numbers.as_slice()) {
            ('M', [x, y, ..]) => segments.push(Segment::Move(*x, *y)),
            ('L', [x, y, ..]) => segments.push(Segment::Line(*x, *y)),
            ('C', [a, b, c, d, x, y, ..]) => {
                segments.push(Segment::Cubic(*a, *b, *c, *d, *x, *y));
            }
            ('Z', _) => segments.push(Segment::Close),
            _ => {}
        }
        numbers.clear();
    };
    for character in data.chars() {
        if character.is_ascii_alphabetic() {
            flush(&mut token, &mut numbers);
            apply(command, &mut numbers, &mut segments);
            command = character;
            if character == 'Z' {
                apply(command, &mut numbers, &mut segments);
                command = ' ';
            }
        } else if character == ' ' || character == ',' {
            flush(&mut token, &mut numbers);
        } else if character == '-' && !token.is_empty() {
            flush(&mut token, &mut numbers);
            token.push('-');
        } else {
            token.push(character);
        }
    }
    flush(&mut token, &mut numbers);
    apply(command, &mut numbers, &mut segments);
    segments
}

static MARK_SHAPES: LazyLock<[Vec<Segment>; 5]> = LazyLock::new(|| {
    [
        parse_path(MARK_BODY),
        parse_path(MARK_FOLD),
        parse_path(MARK_TOP),
        parse_path(MARK_BEAM),
        parse_path(MARK_STEM),
    ]
});

/// Blends two colours in sRGB; good enough for decorative gradients.
fn blend(from: Hsla, to: Hsla, amount: f32) -> Hsla {
    let (a, b) = (Rgba::from(from), Rgba::from(to));
    let t = amount.clamp(0., 1.);
    Rgba {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
    .into()
}

fn paint_shape(
    shape: &[Segment],
    origin: Point<Pixels>,
    scale: f32,
    fill: impl Into<Background>,
    window: &mut Window,
) {
    let at = |x: f32, y: f32| point(origin.x + px(x * scale), origin.y + px(y * scale));
    let mut builder = PathBuilder::fill();
    for segment in shape {
        match *segment {
            Segment::Move(x, y) => builder.move_to(at(x, y)),
            Segment::Line(x, y) => builder.line_to(at(x, y)),
            Segment::Cubic(a, b, c, d, x, y) => {
                builder.cubic_bezier_to(at(x, y), at(a, b), at(c, d));
            }
            Segment::Close => builder.close(),
        }
    }
    if let Ok(path) = builder.build() {
        window.paint_path(path, fill);
    }
}

/// The Filebeam mark, drawn from the brand gradient of the active palette.
pub fn mark(p: Palette, height: f32) -> impl IntoElement {
    let gradient = p.brand_gradient;
    canvas(
        |_, _, _| {},
        move |bounds: Bounds<Pixels>, (), window, _| {
            let scale = bounds.size.height.as_f32() / MARK_HEIGHT;
            let (seed, tip) = (gradient[2], gradient[4]);
            let white = gpui::white();
            let shapes = &*MARK_SHAPES;
            paint_shape(&shapes[0], bounds.origin, scale, seed, window);
            paint_shape(
                &shapes[1],
                bounds.origin,
                scale,
                surface::angled(87., blend(tip, white, 0.12), blend(tip, white, 0.45)),
                window,
            );
            paint_shape(
                &shapes[2],
                bounds.origin,
                scale,
                surface::angled(63., gradient[1], tip),
                window,
            );
            paint_shape(
                &shapes[3],
                bounds.origin,
                scale,
                surface::angled(74., gradient[0], seed),
                window,
            );
            paint_shape(
                &shapes[4],
                bounds.origin,
                scale,
                surface::angled(141., seed, blend(seed, tip, 0.25)),
                window,
            );
        },
    )
    .w(px(height * MARK_WIDTH / MARK_HEIGHT))
    .h(px(height))
    .flex_none()
}

/// An initial on the brand gradient, used wherever a person is shown.
pub fn avatar(p: Palette, name: &str, pixels: f32) -> Div {
    let initial = name
        .trim_start_matches('@')
        .chars()
        .find(|character| character.is_alphanumeric())
        .map_or_else(
            || "?".to_owned(),
            |character| character.to_uppercase().collect(),
        );
    div()
        .size(px(pixels))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(surface::angled(
            135.,
            p.brand_gradient[2],
            p.brand_gradient[4],
        ))
        .text_size(px((pixels * 0.42).max(12.)))
        .font_weight(FontWeight::SEMIBOLD)
        .line_height(relative(1.))
        .text_color(gpui::white())
        .child(SharedString::from(initial))
}

/// A neutral placeholder where no person is signed in.
pub fn avatar_placeholder(p: Palette, pixels: f32) -> Div {
    div()
        .size(px(pixels))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(p.hover)
        .border_1()
        .border_color(p.border)
        .child(icon("user").size(px(pixels * 0.5)).text_color(p.text_muted))
}

fn rotate(x: f32, y: f32, radians: f32) -> (f32, f32) {
    let (sin, cos) = radians.sin_cos();
    (x * cos - y * sin, x * sin + y * cos)
}

/// A rounded rectangle rotated about its bottom centre, as path points.
fn card_path(
    builder: &mut PathBuilder,
    anchor: Point<Pixels>,
    width: f32,
    height: f32,
    corner: f32,
    radians: f32,
) {
    let at = |x: f32, y: f32| {
        let (rx, ry) = rotate(x, y, radians);
        point(anchor.x + px(rx), anchor.y + px(ry))
    };
    let (left, right, top, bottom) = (-width / 2., width / 2., -height, 0.);
    builder.move_to(at(left + corner, top));
    builder.line_to(at(right - corner, top));
    builder.curve_to(at(right, top + corner), at(right, top));
    builder.line_to(at(right, bottom - corner));
    builder.curve_to(at(right - corner, bottom), at(right, bottom));
    builder.line_to(at(left + corner, bottom));
    builder.curve_to(at(left, bottom - corner), at(left, bottom));
    builder.line_to(at(left, top + corner));
    builder.curve_to(at(left + corner, top), at(left, top));
    builder.close();
}

/// The stacked-card emblem from the web drop zone: an image card and a code card
/// fanned behind a front card carrying the mark, sealed with a small lock.
///
/// `engaged` fans the cards wider and lifts them (hover, drag-over). `sealed` draws
/// the lock closed and pulls the stack together, once a transfer is protected.
#[derive(IntoElement)]
pub struct Emblem {
    id: SharedString,
    engaged: bool,
    sealed: bool,
    size: f32,
}

pub fn emblem(id: impl Into<SharedString>) -> Emblem {
    Emblem {
        id: id.into(),
        engaged: false,
        sealed: false,
        size: 116.,
    }
}

impl Emblem {
    pub fn engaged(mut self, engaged: bool) -> Self {
        self.engaged = engaged;
        self
    }

    pub fn sealed(mut self, sealed: bool) -> Self {
        self.sealed = sealed;
        self
    }

    pub fn size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }
}

impl RenderOnce for Emblem {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = theme::palette(cx);
        let size = self.size;
        let fan = motion::spring_unit(
            (self.id.clone(), "fan"),
            if self.sealed {
                0.25
            } else if self.engaged {
                1.
            } else {
                0.62
            },
            motion::BOUNCY,
            window,
            cx,
        );
        let lift = motion::spring_unit(
            (self.id.clone(), "lift"),
            if self.engaged { 1. } else { 0. },
            motion::SPATIAL,
            window,
            cx,
        );
        let (frame_width, frame_height) = (size * 1.42, size * 1.04);
        let (card_width, card_height) = (size * 0.52, size * 0.62);
        let (front_width, front_height) = (card_width * 1.08, card_height * 1.08);
        let centre_x = frame_width / 2.;
        let base_y = frame_height - size * 0.14;
        let corner = size * 0.105;
        // (rotation in degrees, horizontal shift, icon)
        let backs = [
            (-21_f32, -size * 0.36, "image"),
            (23_f32, size * 0.36, "code"),
        ];
        let back_y = base_y + 6. - lift * 5.;
        let front_top = base_y - front_height - lift * 7.;
        let seal = size * 0.2;

        let stroke = p.control_border.opacity(0.55);
        let (back_from, back_to) = (p.card_back, p.card_back_end);
        let artwork = canvas(
            |_, _, _| {},
            move |bounds: Bounds<Pixels>, (), window, _| {
                for (degrees, shift, _) in backs {
                    let anchor = point(
                        bounds.origin.x + px(centre_x + shift * fan),
                        bounds.origin.y + px(back_y),
                    );
                    let radians = (degrees * fan).to_radians();
                    let mut fill = PathBuilder::fill();
                    card_path(&mut fill, anchor, card_width, card_height, corner, radians);
                    if let Ok(path) = fill.build() {
                        window.paint_path(path, surface::angled(145., back_from, back_to));
                    }
                    let mut outline = PathBuilder::stroke(px(1.));
                    card_path(
                        &mut outline,
                        anchor,
                        card_width,
                        card_height,
                        corner,
                        radians,
                    );
                    if let Ok(path) = outline.build() {
                        window.paint_path(path, stroke);
                    }
                }
            },
        )
        .absolute()
        .size_full();

        let glyph = size * 0.19;
        let icons = backs.map(|(degrees, shift, name)| {
            let radians = (degrees * fan).to_radians();
            let (dx, dy) = rotate(0., -card_height / 2., radians);
            div()
                .absolute()
                .left(px(centre_x + shift * fan + dx - glyph / 2.))
                .top(px(back_y + dy - glyph / 2.))
                .child(
                    icon(name)
                        .size(px(glyph))
                        .text_color(p.text_muted.opacity(0.7))
                        .rotate(gpui::radians(radians)),
                )
        });

        div()
            .relative()
            .w(px(frame_width))
            .h(px(frame_height))
            .flex_none()
            .child(artwork)
            .children(icons)
            .child(
                div()
                    .absolute()
                    .left(px(centre_x - front_width / 2.))
                    .top(px(front_top))
                    .w(px(front_width))
                    .h(px(front_height))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(corner * 1.08))
                    .bg(surface::angled(140., p.card_front, p.card_front_end))
                    .border_1()
                    .border_color(p.control_border.opacity(0.7))
                    .shadow(vec![BoxShadow {
                        color: p.shadow,
                        offset: point(px(0.), px(8.)),
                        blur_radius: px(18.),
                        spread_radius: px(0.),
                        inset: false,
                    }])
                    .child(mark(p, size * 0.27)),
            )
            .child(
                div()
                    .absolute()
                    .left(px(centre_x + front_width / 2. - seal * 0.62))
                    .top(px(front_top + front_height - seal * 0.7))
                    .size(px(seal))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .border_1()
                    .when(self.sealed, |this| {
                        this.bg(p.action).border_color(p.action_border)
                    })
                    .when(!self.sealed, |this| {
                        this.bg(p.raised).border_color(p.control_border)
                    })
                    .child(
                        icon("lock")
                            .size(px(seal * 0.56))
                            .text_color(if self.sealed {
                                p.on_action
                            } else {
                                p.text_muted
                            }),
                    ),
            )
    }
}

fn glow(color: Hsla, width: f32, spread: f32, blur: f32) -> Div {
    div()
        .absolute()
        .w(px(width))
        .h(px(2.))
        .ml(px(-width / 2.))
        .rounded_full()
        .shadow(vec![BoxShadow {
            color,
            offset: point(px(0.), px(0.)),
            blur_radius: px(blur),
            spread_radius: px(spread),
            inset: false,
        }])
}

/// The page backdrop: the palette background lit by two soft glows, as on the web.
/// `energy` in `0..1` brightens the light while work is in flight.
pub fn ambient(p: Palette, energy: f32) -> Div {
    let boost = 1. + energy.clamp(0., 1.) * 1.2;
    let lit = |color: Hsla| Hsla {
        a: (color.a * boost).min(1.),
        ..color
    };
    div()
        .absolute()
        .inset_0()
        .overflow_hidden()
        .bg(p.background)
        .child(
            glow(lit(p.glow_primary), 420., 150., 230.)
                .left(relative(0.49))
                .top(relative(0.10)),
        )
        .child(
            glow(lit(p.glow_secondary), 300., 120., 200.)
                .left(relative(0.92))
                .top(relative(0.46)),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn brand_paths_parse_every_command_including_adjacent_negatives() {
        let beam = parse_path(MARK_BEAM);
        assert_eq!(
            beam,
            vec![
                Segment::Move(12., 141.),
                Segment::Line(81., 105.),
                Segment::Line(145., 134.),
                Segment::Line(79., 166.),
                Segment::Close,
            ]
        );
        let body = parse_path(MARK_BODY);
        assert_eq!(body.first(), Some(&Segment::Move(14., 65.)));
        assert_eq!(body.last(), Some(&Segment::Close));
        assert!(body.contains(&Segment::Cubic(153., -3., 168., 5., 168., 20.)));
        assert_eq!(parse_path("M1-2L3,-4Z").len(), 3);
    }

    #[test]
    fn every_mark_shape_is_a_closed_outline() {
        for shape in MARK_SHAPES.iter() {
            assert!(matches!(shape.first(), Some(Segment::Move(..))));
            assert_eq!(shape.last(), Some(&Segment::Close));
        }
    }

    #[test]
    fn card_rotation_pivots_about_the_bottom_centre() {
        let (x, y) = rotate(0., -10., std::f32::consts::FRAC_PI_2);
        assert!((x - 10.).abs() < 1e-4 && y.abs() < 1e-4);
        assert_eq!(rotate(0., 0., 1.3), (0., 0.));
    }
}
