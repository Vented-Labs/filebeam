//! Builds palettes from the same definitions the web app uses
//! (`backend/app/Support/Theming`). Preset tables are copied verbatim and custom
//! colours run the same OKLCH derivation, so a colour chosen here matches the web,
//! Android and iOS clients.

use super::color::{self, Oklch, Rgb};

pub const DEFAULT_PRIMARY: Rgb = 0x8b35ff;

/// Colour presets shared with the web app. `seed` and `endpoint` anchor the brand gradient.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Preset {
    Purple,
    Blue,
    Teal,
    Green,
    Amber,
    Orange,
    Rose,
}

impl Preset {
    pub const ALL: [Self; 7] = [
        Self::Purple,
        Self::Blue,
        Self::Teal,
        Self::Green,
        Self::Amber,
        Self::Orange,
        Self::Rose,
    ];

    pub const fn id(self) -> &'static str {
        match self {
            Self::Purple => "purple",
            Self::Blue => "blue",
            Self::Teal => "teal",
            Self::Green => "green",
            Self::Amber => "amber",
            Self::Orange => "orange",
            Self::Rose => "rose",
        }
    }

    pub const fn title(self) -> &'static str {
        match self {
            Self::Purple => "Purple",
            Self::Blue => "Blue",
            Self::Teal => "Teal",
            Self::Green => "Green",
            Self::Amber => "Amber",
            Self::Orange => "Orange",
            Self::Rose => "Rose",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|preset| preset.id() == id)
    }

    pub const fn seed(self) -> Rgb {
        match self {
            Self::Purple => 0x8b35ff,
            Self::Blue => 0x3d78d8,
            Self::Teal => 0x168f85,
            Self::Green => 0x388b59,
            Self::Amber => 0xcf9b35,
            Self::Orange => 0xda7b40,
            Self::Rose => 0xcf5d78,
        }
    }

    pub const fn endpoint(self) -> Rgb {
        match self {
            Self::Purple => 0xc05ada,
            Self::Blue => 0x4a9dbd,
            Self::Teal => 0x48a778,
            Self::Green => 0x76a543,
            Self::Amber => 0xd78135,
            Self::Orange => 0xd46369,
            Self::Rose => 0xb570c0,
        }
    }

    pub fn matching(seed: Rgb) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|preset| preset.seed() == seed & 0xFF_FFFF)
    }

    const fn index(self) -> usize {
        self as usize
    }
}

/// A colour with an alpha channel, for scrims, lines and glows.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Alpha {
    pub rgb: Rgb,
    pub alpha: f64,
}

impl Alpha {
    pub const CLEAR: Self = Self { rgb: 0, alpha: 0.0 };

    const fn new(rgb: Rgb, alpha: f64) -> Self {
        Self { rgb, alpha }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Status {
    pub ink: Rgb,
    pub surface: Rgb,
    pub border: Rgb,
}

/// Syntax and surface colours for the note editor and viewer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Editor {
    pub background: Rgb,
    pub gutter_background: Rgb,
    pub gutter_text: Rgb,
    pub gutter_active_background: Rgb,
    pub gutter_active_text: Rgb,
    pub border: Rgb,
    pub active_line: Alpha,
    pub selection: Rgb,
    pub caret: Rgb,
    pub text: Rgb,
    pub keyword: Rgb,
    pub string: Rgb,
    pub number: Rgb,
    pub constant: Rgb,
    pub function: Rgb,
    pub property: Rgb,
    pub type_: Rgb,
    pub tag: Rgb,
    pub attribute: Rgb,
    pub comment: Rgb,
    pub punctuation: Rgb,
    pub operator: Rgb,
    pub meta: Rgb,
    pub regexp: Rgb,
    pub escape: Rgb,
    pub link: Rgb,
    pub heading: Rgb,
    pub invalid: Rgb,
}

/// Every colour role the app draws with. Roles mirror the web tokens (`--fb-*`) so a
/// screen looks the same whether the palette came from a preset or a custom colour.
#[derive(Clone, Debug, PartialEq)]
pub struct Tokens {
    pub dark: bool,
    pub chrome: Rgb,
    pub background: Rgb,
    pub surface: Rgb,
    pub raised: Rgb,
    pub sunken: Rgb,
    pub border: Rgb,
    pub control_border: Rgb,
    pub control_border_hover: Rgb,
    pub hover: Rgb,
    pub active: Rgb,
    pub text: Rgb,
    pub text_muted: Rgb,
    pub text_subtle: Rgb,
    pub disabled_border: Rgb,
    pub disabled_surface: Rgb,
    pub disabled_text: Rgb,
    pub accent_text: Rgb,
    pub accent_surface: Rgb,
    pub selected_surface: Rgb,
    pub action: Rgb,
    pub action_top: Rgb,
    pub action_hover: Rgb,
    pub action_pressed: Rgb,
    pub on_action: Rgb,
    pub action_border: Rgb,
    pub choice_border: Rgb,
    pub progress_fill: Rgb,
    pub progress_track: Rgb,
    pub success: Status,
    pub warning: Status,
    pub danger: Status,
    pub info: Status,
    pub scrim: Alpha,
    pub line_soft: Alpha,
    pub line_strong: Alpha,
    pub shadow: Alpha,
    pub highlight: Alpha,
    pub sheen: Alpha,
    pub brand: Rgb,
    /// Five stops matching the web's 135 degree mark gradient.
    pub brand_gradient: [Rgb; 5],
    pub glow_primary: Alpha,
    pub glow_secondary: Alpha,
    pub card_back: Rgb,
    pub card_back_end: Rgb,
    pub card_front: Rgb,
    pub card_front_end: Rgb,
    pub editor: Editor,
}

// Columns: chrome, page, surface, raised, sunken, footer, settings, border, control,
// control hover, hover, active.
const NEUTRALS_LIGHT: [&str; 7] = [
    "f7f7fa f7f7fa fefefe ffffff f2f1f6 f5f5f8 f5f5f8 d3d2d9 76757c 5f5d67 edebf2 e6e4ed",
    "f6f7fa f6f7fa fdfefe ffffff f0f2f6 f3f5f8 f3f5f8 d0d3d9 72767d 5a5f68 e9edf3 e2e6ed",
    "f5f8f8 f5f8f8 fdfefe ffffff eef3f3 f2f6f5 f2f6f5 cdd5d4 6e7877 556260 e6efed dee8e7",
    "f5f8f6 f5f8f6 fdfefd ffffff eff3f0 f3f6f4 f3f6f4 cfd5d0 717873 58625b e8eeea e1e8e3",
    "f9f7f4 f9f7f4 fefefd ffffff f4f2ee f6f5f2 f6f5f2 d6d3cd 7a756e 645e55 f0ece6 eae5de",
    "faf7f5 faf7f5 fefdfd ffffff f6f1ef f7f4f3 f7f4f3 d8d2ce 7c7470 675d57 f2ebe7 ece4df",
    "faf6f7 faf6f7 fefdfe ffffff f6f0f1 f8f4f4 f8f4f4 d9d1d2 7d7374 685b5d f2eaeb ede3e4",
];
const NEUTRALS_DARK: [&str; 7] = [
    "0b0914 100e16 191621 211d2a 120f19 14111a 15121c 3b3447 84778f a296ae 2a2435 322a3f",
    "080b10 0c1015 15181e 1c2025 0e1116 101318 111419 333842 777d88 959ca7 24282e 2b2f35",
    "060d0c 091110 121a19 192120 0b1312 0d1514 0e1615 2e3b39 72817f 90a09d 212a28 28312f",
    "070c09 0b110d 141a16 1b211d 0d130f 0f1410 101612 313b34 758078 949f97 232925 2a302c",
    "0e0a06 120f0a 1b1812 231f19 14100b 16120d 17130e 3d372e 837c71 a29b90 2b2721 322e28",
    "0f0907 140e0b 1d1713 241e1b 15100c 17110e 19120f 403630 867a74 a69992 2d2622 342d29",
    "0f090a 140d0e 1d1617 251d1e 160f10 181112 191213 413436 87797b a6979a 2d2526 342c2d",
];
// Columns: action, top, hover, active, on-action, light ink, dark ink, light border,
// dark border, dark progress.
const ACCENTS: [&str; 7] = [
    "7c3aed 8545e6 6d28d9 5b21b6 ffffff 6d28d9 c4a6ee 6d28d9 ad83e8 b18be7",
    "2864c5 3570ce 2257ac 1c4791 ffffff 235bac a0c2f4 2257ac 80a8e5 8cb3ed",
    "087c73 0a7e75 086b64 075b55 ffffff 086b64 8cd1c6 086b64 66b7aa 72c1b6",
    "267a48 2c7f4e 21693e 1c5935 ffffff 21693e a1d0ab 21693e 78ad87 85c495",
    "e4b34f e9bd64 dda53a d0992b 33250c 805515 e6c17c 956719 b88a35 dfb765",
    "f09a60 f3a674 e98c4b de7e3a 362014 964618 efb389 af5828 ce8b5a e5a471",
    "b83d5b bf4765 a33350 8d2b44 ffffff a33350 ecaabc a33350 d8869a dd91a6",
];
// Columns: light accent, light selected, dark accent, dark selected.
const TINTS: [&str; 7] = [
    "f5f2ff eeeaff 241c38 2e214a",
    "eff5fc e5eefb 1a2331 1e2b40",
    "eff6f5 e5f0ef 162624 18302d",
    "eff6f1 e6f0e8 19261d 1d2f23",
    "fbf7f0 f9f2e7 2d2618 3c301c",
    "fcf4f0 fbeee7 302219 402a1d",
    "fcf2f4 fbeaed 301e21 3e2429",
];
const LIGHT_EFFECTS: [(Rgb, Rgb); 7] = [
    (0x8b5cf6, 0xc66bd4),
    (0x4d87d8, 0x51a4be),
    (0x309f94, 0x70b98d),
    (0x61a76f, 0x93b660),
    (0xd7a443, 0xd48b5a),
    (0xe59a66, 0xd77d93),
    (0xd882a0, 0xb88acd),
];
/// OKLCH targets for light custom neutrals, in neutral column order.
const CUSTOM_LIGHT_TARGETS: [(f64, f64); 12] = [
    (0.977, 0.004),
    (0.977, 0.004),
    (0.996, 0.001),
    (1.0, 0.0),
    (0.961, 0.006),
    (0.970, 0.004),
    (0.970, 0.004),
    (0.867, 0.009),
    (0.565, 0.012),
    (0.485, 0.016),
    (0.944, 0.009),
    (0.924, 0.011),
];

const fn is_border_column(index: usize) -> bool {
    matches!(index, 7..=9)
}

fn row(value: &str) -> Vec<Rgb> {
    value
        .split(' ')
        .map(|hex| Rgb::from_str_radix(hex, 16).expect("authored palette tables are hex"))
        .collect()
}

/// Structural and accent roles before fixed, editor and decorative roles are added.
struct Roles {
    neutrals: Vec<Rgb>,
    accent_text: Rgb,
    accent_surface: Rgb,
    selected_surface: Rgb,
    action: Rgb,
    action_top: Rgb,
    action_hover: Rgb,
    action_active: Rgb,
    on_action: Rgb,
    action_border: Rgb,
    choice_border: Rgb,
    progress_fill: Rgb,
}

impl Roles {
    fn background(&self) -> Rgb {
        self.neutrals[1]
    }
    fn surface(&self) -> Rgb {
        self.neutrals[2]
    }
    fn sunken(&self) -> Rgb {
        self.neutrals[4]
    }
    fn border(&self) -> Rgb {
        self.neutrals[7]
    }
    fn active(&self) -> Rgb {
        self.neutrals[11]
    }
}

fn authored(preset: Preset, dark: bool) -> Roles {
    let accent = row(ACCENTS[preset.index()]);
    let tint = row(TINTS[preset.index()]);
    Roles {
        neutrals: row(if dark { NEUTRALS_DARK } else { NEUTRALS_LIGHT }[preset.index()]),
        accent_text: if dark { accent[6] } else { accent[5] },
        accent_surface: tint[if dark { 2 } else { 0 }],
        selected_surface: tint[if dark { 3 } else { 1 }],
        action: accent[0],
        action_top: accent[1],
        action_hover: accent[2],
        action_active: accent[3],
        on_action: accent[4],
        action_border: if dark { accent[8] } else { accent[7] },
        choice_border: if dark { accent[8] } else { accent[5] },
        progress_fill: if dark { accent[9] } else { accent[5] },
    }
}

struct Custom {
    chroma: f64,
    hue: f64,
    anchor: Rgb,
    endpoint: Rgb,
}

impl Custom {
    fn new(seed: Rgb) -> Self {
        let source = color::oklch(seed);
        let grey = source.c < 0.01;
        let chroma = if grey { 0.0 } else { source.c };
        let hue = if grey { 0.0 } else { source.h };
        Self {
            chroma,
            hue,
            anchor: color::rgb(0.65, chroma.min(0.14), hue),
            endpoint: color::rgb(0.72, chroma.min(0.15), hue),
        }
    }
}

fn custom(seed: Rgb, dark: bool) -> Roles {
    let custom = Custom::new(seed);
    let (chroma, hue) = (custom.chroma, custom.hue);
    let light = !dark;
    let mut neutrals: Vec<Rgb> = row(NEUTRALS_DARK[Preset::Purple.index()])
        .into_iter()
        .enumerate()
        .map(|(index, source)| {
            let border = is_border_column(index);
            if light {
                let (l, cap) = CUSTOM_LIGHT_TARGETS[index];
                return color::rgb(l, cap.min(chroma * 0.15), hue);
            }
            let from = color::oklch(source);
            let cap = if border { 0.018 } else { 0.012 };
            let scale = if border { 0.20 } else { 0.15 };
            color::rgb(from.l, from.c.min(cap).min(chroma * scale), hue)
        })
        .collect();
    let surface = neutrals[2];
    let accent_surface = color::mix(custom.anchor, surface, if light { 0.07 } else { 0.12 });
    let selected_surface = color::mix(custom.anchor, surface, if light { 0.12 } else { 0.21 });
    let mut backgrounds: Vec<Rgb> = neutrals
        .iter()
        .enumerate()
        .filter(|(index, _)| !is_border_column(*index))
        .map(|(_, value)| *value)
        .collect();
    backgrounds.extend([accent_surface, selected_surface]);

    // The reference treats an unreachable contrast target as a programming error. A
    // desktop colour picker must never abort, so each search falls back to the
    // strongest ink for the mode instead.
    let strongest = if light { 0x000000 } else { 0xffffff };
    let accent_text = color::against(
        Oklch {
            l: if light { 0.43 } else { 0.79 },
            c: chroma.min(if light { 0.11 } else { 0.10 }),
            h: hue,
        },
        &backgrounds,
        4.8,
        light,
    )
    .unwrap_or(strongest);
    for index in [8, 9] {
        neutrals[index] = color::against(color::oklch(neutrals[index]), &backgrounds, 3.0, light)
            .unwrap_or(neutrals[index]);
    }

    let warm = (45.0..=105.0).contains(&hue) && chroma >= 0.04;
    let ink: Rgb = if warm { 0x2b2318 } else { 0xffffff };
    let levels: [f64; 4] = if warm {
        [0.78, 0.80, 0.75, 0.72]
    } else {
        [0.50, 0.525, 0.46, 0.42]
    };
    let fill_chroma = chroma.min(if warm { 0.14 } else { 0.19 });
    let lowest = levels.iter().copied().fold(f64::INFINITY, f64::min);
    let highest = levels.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mut fills = levels.map(|level| color::rgb(level, fill_chroma, hue));
    for step in 0..=1000 {
        let shift = if warm { 1.0 } else { -1.0 } * f64::from(step) * 0.001;
        if lowest + shift < 0.0 || highest + shift > 1.0 {
            break;
        }
        fills = levels.map(|level| color::rgb(level + shift, fill_chroma, hue));
        if color::minimum_contrast(ink, &fills) >= 4.8 {
            break;
        }
    }

    let border = if light {
        accent_text
    } else {
        color::against(
            Oklch {
                l: 0.67,
                c: chroma.min(0.09),
                h: hue,
            },
            &backgrounds,
            3.0,
            false,
        )
        .unwrap_or(accent_text)
    };
    let track = if light { neutrals[11] } else { neutrals[7] };
    let progress = if light {
        accent_text
    } else {
        color::against(
            Oklch {
                l: 0.74,
                c: chroma.min(0.10),
                h: hue,
            },
            &[track],
            3.5,
            false,
        )
        .unwrap_or(accent_text)
    };
    Roles {
        neutrals,
        accent_text,
        accent_surface,
        selected_surface,
        action: fills[0],
        action_top: fills[1],
        action_hover: fills[2],
        action_active: fills[3],
        on_action: ink,
        action_border: border,
        choice_border: border,
        progress_fill: progress,
    }
}

struct Fixed {
    text: Vec<Rgb>,
    status: Vec<Rgb>,
    // bg, gutter bg, gutter fg, gutter active bg, gutter active fg, border, selection,
    // selection inactive, matching, search, caret
    editor_surface: Vec<Rgb>,
    // fg keyword string number constant function property type tag attribute comment
    // punctuation operator meta regexp escape link heading invalid
    syntax: Vec<Rgb>,
    active_line: Rgb,
    active_line_alpha: f64,
}

impl Fixed {
    fn new(dark: bool) -> Self {
        Self {
            text: row(if dark {
                "f4f1fa b5abbf ada3b7 443a50 2b2434 a89cb5"
            } else {
                "262230 5e576a 62596d d8d2e0 ece8f1 777080"
            }),
            status: row(if dark {
                "a1d0ab 1d3025 79b688 e6c17c 352b1b c09d58 f0a7b9 3b242d ca7d91 a0c2f4 1e2c42 789ecd"
            } else {
                "21693e edf7f0 34824e 805515 fbf4e5 986b26 b4233f fceef0 c34359 235bac edf3fc 4675b8"
            }),
            editor_surface: row(if dark {
                "120f19 191621 ada6b8 30263f eceaf2 3b3447 332a46 282233 30263f 3a3021 eceaf2"
            } else {
                "fcfbfe f3f1f7 5c606d eee9f6 292532 d7d2e0 ece6f7 eeebf4 eee9f6 f7edd6 292532"
            }),
            syntax: row(if dark {
                "eceaf2 cbacf5 91ceac e7bc87 f3aec2 9abff5 89c8d3 d6b8f0 f0a7b9 e8c095 ada6b8 aba5b7 beb7cc dcc088 f3abc0 edc49a 9abff5 ceaeff f5a3b5"
            } else {
                "292532 783ab0 176b45 925016 aa355a 245fa5 086976 744a97 a73558 8a4b17 5c606d 565c6a 565c6a 765a17 a03652 964712 245fa5 6e38b0 b4233f"
            }),
            active_line: if dark { 0xffffff } else { 0x242033 },
            active_line_alpha: f64::from(0x05) / 255.0,
        }
    }
}

fn editor(primary: Rgb, dark: bool, roles: &Roles, fixed: &Fixed) -> Editor {
    let mut surface = fixed.editor_surface.clone();
    let mut syntax = fixed.syntax.clone();
    if primary != DEFAULT_PRIMARY {
        let light = !dark;
        let bg = if light {
            color::mix(roles.sunken(), 0xffffff, 0.18)
        } else {
            roles.sunken()
        };
        let gutter = if light {
            color::mix(roles.sunken(), bg, 0.75)
        } else {
            roles.surface()
        };
        let selected = roles.selected_surface;
        let matching = color::mix(selected, if light { bg } else { gutter }, 0.75);
        let inactive = color::mix(selected, bg, 0.6);
        surface[0] = bg;
        surface[1] = gutter;
        surface[3] = matching;
        surface[5] = roles.border();
        surface[6] = selected;
        surface[7] = inactive;
        surface[8] = matching;
        let plain = [bg, selected, inactive, matching, surface[9]];
        let mut backgrounds = plain.to_vec();
        backgrounds.extend(
            plain
                .iter()
                .map(|value| color::composite(fixed.active_line, fixed.active_line_alpha, *value)),
        );
        let seed = color::oklch(primary);
        let anchor_hue = color::oklch(DEFAULT_PRIMARY).h;
        // keyword, type and heading follow the accent hue; every other syntax colour stays put.
        for index in [1, 7, 17] {
            let base = color::oklch(syntax[index]);
            let grey = seed.c < 0.01;
            let c = if grey {
                0.0
            } else {
                base.c.min(seed.c * 0.8).min(0.14)
            };
            let h = if grey {
                0.0
            } else {
                (base.h + seed.h - anchor_hue + 360.0) % 360.0
            };
            let ink = color::against(Oklch { l: base.l, c, h }, &backgrounds, 4.8, light)
                .unwrap_or(syntax[index]);
            syntax[index] = color::against(color::oklch(ink), &[bg], 5.5, light).unwrap_or(ink);
        }
    }
    Editor {
        background: surface[0],
        gutter_background: surface[1],
        gutter_text: surface[2],
        gutter_active_background: surface[3],
        gutter_active_text: surface[4],
        border: surface[5],
        active_line: Alpha::new(fixed.active_line, fixed.active_line_alpha),
        selection: surface[6],
        caret: surface[10],
        text: syntax[0],
        keyword: syntax[1],
        string: syntax[2],
        number: syntax[3],
        constant: syntax[4],
        function: syntax[5],
        property: syntax[6],
        type_: syntax[7],
        tag: syntax[8],
        attribute: syntax[9],
        comment: syntax[10],
        punctuation: syntax[11],
        operator: syntax[12],
        meta: syntax[13],
        regexp: syntax[14],
        escape: syntax[15],
        link: syntax[16],
        heading: syntax[17],
        invalid: syntax[18],
    }
}

struct Effects {
    glow_a: Alpha,
    glow_b: Alpha,
    card_back: Rgb,
    card_back_end: Rgb,
    card_front: Rgb,
    card_front_end: Rgb,
}

fn effects(primary: Rgb, dark: bool, roles: &Roles) -> Effects {
    if dark {
        let seed = color::oklch(primary);
        let anchor = color::oklch(DEFAULT_PRIMARY);
        let shift = |source: Rgb| -> Rgb {
            if primary == DEFAULT_PRIMARY {
                return source;
            }
            let from = color::oklch(source);
            let grey = seed.c < 0.01;
            color::rgb(
                from.l,
                if grey {
                    0.0
                } else {
                    from.c * (seed.c / anchor.c).min(1.4)
                },
                if grey {
                    0.0
                } else {
                    (from.h + seed.h - anchor.h + 360.0) % 360.0
                },
            )
        };
        return Effects {
            glow_a: Alpha::new(shift(0x573182), f64::from(0x1f) / 255.0),
            glow_b: Alpha::new(shift(0x342043), f64::from(0x13) / 255.0),
            card_back: shift(0x2e2639),
            card_back_end: shift(0x211b2b),
            card_front: shift(0x3d2c4c),
            card_front_end: shift(0x2c2138),
        };
    }
    let (a, b, strong) = if let Some(preset) = Preset::matching(primary) {
        let (a, b) = LIGHT_EFFECTS[preset.index()];
        (a, b, matches!(preset, Preset::Amber | Preset::Orange))
    } else {
        let seed = color::oklch(primary);
        let grey = seed.c < 0.01;
        let hue = if grey { 0.0 } else { seed.h };
        (
            color::rgb(0.67, if grey { 0.0 } else { seed.c.min(0.13) }, hue),
            color::rgb(0.73, if grey { 0.0 } else { seed.c.min(0.10) }, hue),
            seed.c >= 0.04 && (45.0..=105.0).contains(&seed.h),
        )
    };
    Effects {
        glow_a: Alpha::new(a, if strong { 0.22 } else { 0.16 }),
        glow_b: Alpha::new(b, if strong { 0.10 } else { 0.08 }),
        card_back: color::mix(a, roles.surface(), 0.04),
        card_back_end: color::mix(a, roles.surface(), 0.12),
        card_front: color::mix(a, roles.surface(), 0.09),
        card_front_end: color::mix(b, roles.surface(), 0.19),
    }
}

fn brand_gradient(seed: Rgb, endpoint: Rgb, dark: bool, is_default: bool) -> [Rgb; 5] {
    let tip = if is_default && dark {
        0xc75bfa
    } else if dark {
        endpoint
    } else {
        color::mix(endpoint, 0x000000, 0.90)
    };
    let start = color::mix(seed, 0x000000, 0.82);
    [
        start,
        color::mix(start, seed, 0.5),
        seed,
        color::mix(seed, tip, 0.5),
        tip,
    ]
}

pub fn preset(preset: Preset, dark: bool) -> Tokens {
    assemble(
        preset.seed(),
        preset.seed(),
        preset.endpoint(),
        dark,
        authored(preset, dark),
        preset == Preset::Purple,
    )
}

/// Any sRGB seed. A seed equal to a preset's seed yields that authored preset, as on the web.
pub fn seeded(seed: Rgb, dark: bool) -> Tokens {
    let rgb = seed & 0xFF_FFFF;
    if let Some(matched) = Preset::matching(rgb) {
        return preset(matched, dark);
    }
    let derived = Custom::new(rgb);
    assemble(
        rgb,
        derived.anchor,
        derived.endpoint,
        dark,
        custom(rgb, dark),
        false,
    )
}

fn assemble(
    primary: Rgb,
    gradient_seed: Rgb,
    endpoint: Rgb,
    dark: bool,
    roles: Roles,
    is_default: bool,
) -> Tokens {
    let fixed = Fixed::new(dark);
    // Text and disabled tints are authored for purple. For any other colour they keep their
    // lightness and take the hue of that palette's own neutrals, so no purple cast remains.
    let text: Vec<Rgb> = if is_default {
        fixed.text.clone()
    } else {
        let neutral = color::oklch(roles.background());
        fixed
            .text
            .iter()
            .map(|value| {
                let source = color::oklch(*value);
                color::rgb(
                    source.l,
                    if neutral.c < 0.002 { 0.0 } else { source.c },
                    neutral.h,
                )
            })
            .collect()
    };
    let effects = effects(primary, dark, &roles);
    let ink: Rgb = if dark { 0xffffff } else { 0x242033 };
    let status = |offset: usize| Status {
        ink: fixed.status[offset],
        surface: fixed.status[offset + 1],
        border: fixed.status[offset + 2],
    };
    Tokens {
        dark,
        chrome: roles.neutrals[0],
        background: roles.background(),
        surface: roles.surface(),
        raised: roles.neutrals[3],
        sunken: roles.sunken(),
        border: roles.border(),
        control_border: roles.neutrals[8],
        control_border_hover: roles.neutrals[9],
        hover: roles.neutrals[10],
        active: roles.active(),
        text: text[0],
        text_muted: text[1],
        text_subtle: text[2],
        disabled_border: text[3],
        disabled_surface: text[4],
        disabled_text: text[5],
        accent_text: roles.accent_text,
        accent_surface: roles.accent_surface,
        selected_surface: roles.selected_surface,
        action: roles.action,
        action_top: roles.action_top,
        action_hover: roles.action_hover,
        action_pressed: roles.action_active,
        on_action: roles.on_action,
        action_border: roles.action_border,
        choice_border: roles.choice_border,
        progress_fill: roles.progress_fill,
        progress_track: if dark { roles.border() } else { roles.active() },
        success: status(0),
        warning: status(3),
        danger: status(6),
        info: status(9),
        scrim: if dark {
            Alpha::new(0x08060e, 0.68)
        } else {
            Alpha::new(0x181420, 0.34)
        },
        line_soft: Alpha::new(ink, 0.07),
        line_strong: Alpha::new(ink, 0.10),
        shadow: if dark {
            Alpha::new(0x000000, 0.36)
        } else {
            Alpha::new(0x181420, 0.10)
        },
        highlight: if dark {
            Alpha::new(0xffffff, 0.08)
        } else {
            Alpha::CLEAR
        },
        sheen: if dark {
            Alpha::new(0xffffff, 0.12)
        } else {
            Alpha::CLEAR
        },
        brand: primary,
        brand_gradient: brand_gradient(gradient_seed, endpoint, dark, is_default),
        glow_primary: effects.glow_a,
        glow_secondary: effects.glow_b,
        card_back: effects.card_back,
        card_back_end: effects.card_back_end,
        card_front: effects.card_front,
        card_front_end: effects.card_front_end,
        editor: editor(primary, dark, &roles, &fixed),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    /// The web theme generator's output, shared with the Android and iOS ports.
    const GOLDEN: &str =
        include_str!("../../../mobile/android/app/src/test/fixtures/palette-golden.json");

    type Golden = BTreeMap<String, BTreeMap<String, BTreeMap<String, String>>>;

    fn golden() -> Golden {
        serde_json::from_str(GOLDEN).expect("palette fixture is valid JSON")
    }

    /// Every `#rrggbb` or `#rrggbbaa` literal in a CSS value, in order.
    fn hexes(value: &str) -> Vec<String> {
        let bytes = value.as_bytes();
        let mut found = Vec::new();
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] == b'#' {
                let end = (index + 1..bytes.len())
                    .find(|position| !bytes[*position].is_ascii_hexdigit())
                    .unwrap_or(bytes.len());
                if end - index > 6 {
                    found.push(value[index..index + 7].to_owned());
                }
                index = end;
            } else {
                index += 1;
            }
        }
        found
    }

    fn roles(palette: &Tokens) -> Vec<(&'static str, Rgb)> {
        vec![
            ("--fb-bg", palette.background),
            ("--fb-surface", palette.surface),
            ("--fb-surface-raised", palette.raised),
            ("--fb-surface-sunken", palette.sunken),
            ("--fb-border", palette.border),
            ("--fb-control-border", palette.control_border),
            ("--fb-control-border-hover", palette.control_border_hover),
            ("--fb-surface-hover", palette.hover),
            ("--fb-surface-active", palette.active),
            ("--fb-accent-text", palette.accent_text),
            ("--fb-accent-surface", palette.accent_surface),
            ("--fb-selected-surface", palette.selected_surface),
            ("--fb-action", palette.action),
            ("--fb-action-hover", palette.action_hover),
            ("--fb-action-active", palette.action_pressed),
            ("--fb-on-action", palette.on_action),
            ("--fb-action-border", palette.action_border),
            ("--fb-progress-fill", palette.progress_fill),
            ("--fb-progress-track", palette.progress_track),
            ("--fb-editor-bg", palette.editor.background),
            ("--fb-editor-keyword", palette.editor.keyword),
            ("--fb-editor-type", palette.editor.type_),
            ("--fb-editor-heading", palette.editor.heading),
            ("--fb-editor-selection", palette.editor.selection),
        ]
    }

    #[test]
    fn matches_the_web_theme_generator() {
        let mut mismatches = Vec::new();
        for (seed, modes) in golden() {
            for mode in ["dark", "light"] {
                let expected = &modes[mode];
                let palette = seeded(color::parse(&seed).unwrap(), mode == "dark");
                for (name, value) in roles(&palette) {
                    if expected.get(name) != Some(&color::format(value)) {
                        mismatches.push(format!(
                            "{seed} {mode} {name}: {} != {:?}",
                            color::format(value),
                            expected.get(name)
                        ));
                    }
                }
                let formatted =
                    |values: &[Rgb]| values.iter().map(|v| color::format(*v)).collect::<Vec<_>>();
                if formatted(&palette.brand_gradient) != hexes(&expected["--fb-brand-gradient"]) {
                    mismatches.push(format!("{seed} {mode} gradient"));
                }
                if formatted(&[palette.card_back, palette.card_back_end])
                    != hexes(&expected["--fb-dropzone-mark"])
                {
                    mismatches.push(format!("{seed} {mode} card back"));
                }
                if formatted(&[palette.card_front, palette.card_front_end])
                    != hexes(&expected["--fb-dropzone-mark-hover"])
                {
                    mismatches.push(format!("{seed} {mode} card front"));
                }
                if mode == "dark"
                    && formatted(&[palette.glow_primary.rgb, palette.glow_secondary.rgb])
                        != hexes(&expected["--fb-ambient-glow"])
                {
                    mismatches.push(format!("{seed} {mode} ambient glow"));
                }
            }
        }
        assert!(mismatches.is_empty(), "{}", mismatches.join("\n"));
    }

    #[test]
    fn text_and_actions_meet_the_contrast_contract() {
        for seed in golden().keys() {
            for dark in [true, false] {
                let palette = seeded(color::parse(seed).unwrap(), dark);
                let surfaces = [
                    palette.background,
                    palette.surface,
                    palette.raised,
                    palette.sunken,
                ];
                assert!(
                    color::minimum_contrast(palette.text, &surfaces) >= 4.5,
                    "{seed} text"
                );
                assert!(
                    color::minimum_contrast(palette.accent_text, &surfaces) >= 4.5,
                    "{seed} accent"
                );
                assert!(
                    color::contrast(palette.on_action, palette.action) >= 4.8,
                    "{seed} action"
                );
                assert!(
                    color::minimum_contrast(palette.control_border, &surfaces) >= 3.0,
                    "{seed} control"
                );
            }
        }
    }

    #[test]
    fn every_preset_and_mode_builds() {
        for candidate in Preset::ALL {
            for dark in [true, false] {
                let palette = preset(candidate, dark);
                assert_eq!(palette.dark, dark);
                assert_eq!(palette.brand, candidate.seed());
                assert_eq!(Preset::from_id(candidate.id()), Some(candidate));
            }
        }
    }

    #[test]
    fn arbitrary_seeds_never_panic() {
        // The colour picker can produce any sRGB value, including extremes the
        // authored presets never exercise.
        for seed in [
            0x000000, 0xffffff, 0xff0000, 0x00ff00, 0x0000ff, 0xffff00, 0x00ffff, 0xff00ff,
            0x010101, 0xfefefe, 0x7f7f00, 0x123456,
        ] {
            for dark in [true, false] {
                let palette = seeded(seed, dark);
                assert_eq!(palette.brand, seed);
            }
        }
    }
}
