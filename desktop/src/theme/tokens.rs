//! Shape, type, spacing and motion tokens. Values follow the web tokens (`--fb-*`),
//! with type and control sizes adapted to desktop density.

pub mod geometry {
    pub const TITLEBAR_HEIGHT: f32 = 46.0;
    pub const SIDEBAR: f32 = 236.0;
    pub const SIDEBAR_RAIL: f32 = 72.0;
    /// Below this window width the sidebar collapses to its rail.
    pub const COMPACT_BELOW: f32 = 1100.0;
    /// Reading width for forms and single-column pages.
    pub const CONTENT_READING: f32 = 760.0;
    /// Widest a page's content grows before it centres.
    pub const CONTENT_MAX: f32 = 1240.0;
    pub const PAGE_GUTTER: f32 = 32.0;
    pub const PAGE_GUTTER_COMPACT: f32 = 20.0;
    pub const CONTROL_HEIGHT: f32 = 38.0;
    pub const CONTROL_HEIGHT_LARGE: f32 = 44.0;
    pub const CONTROL_HEIGHT_SMALL: f32 = 30.0;
    pub const INSPECTOR: f32 = 340.0;
    pub const INSPECTOR_MIN: f32 = 300.0;
    pub const INSPECTOR_MAX: f32 = 440.0;
}

/// Corner radii. Nested corners are concentric: a shape whose edge sits `gap` inside a
/// rounded container takes the container's radius less that gap, so both curves share a
/// centre and the space between them stays even around the corner. Derive one side of
/// every such pair with [`nested`] or [`around`] instead of choosing two radii by eye.
pub mod radius {
    pub const SMALL: f32 = 8.0;
    pub const CONTROL: f32 = 10.0;
    pub const CARD: f32 = 18.0;
    pub const PANEL: f32 = 24.0;
    /// Below this a corner reads as square, so nesting stops shrinking here.
    pub const FLOOR: f32 = 2.0;

    /// The radius of a shape whose edge sits `gap` inside a container of radius `outer`.
    /// `gap` runs between the two outer edges, so it includes the container's border.
    pub const fn nested(outer: f32, gap: f32) -> f32 {
        let inner = outer - gap;
        if inner > FLOOR { inner } else { FLOOR }
    }

    /// The radius of a container whose edge sits `gap` outside a shape of radius `inner`.
    pub const fn around(inner: f32, gap: f32) -> f32 {
        inner + gap
    }
}

/// Spacing on the shared 4 px grid.
pub mod space {
    pub const XXS: f32 = 2.0;
    pub const XS: f32 = 4.0;
    pub const SM: f32 = 8.0;
    pub const MD: f32 = 12.0;
    pub const LG: f32 = 16.0;
    pub const XL: f32 = 20.0;
    pub const XXL: f32 = 24.0;
    pub const XXXL: f32 = 32.0;
    /// Between the outer edge of a panel and a row or card that hugs it.
    pub const HUG: f32 = 6.0;
    /// Between the outer edge of a control and a control set inside it.
    pub const INSET: f32 = 4.0;
}

/// Type sizes. Nothing in the app is set smaller than `CAPTION`.
pub mod text {
    pub const CAPTION: f32 = 12.0;
    pub const SMALL: f32 = 13.0;
    pub const BODY: f32 = 14.0;
    pub const SUBTITLE: f32 = 16.0;
    pub const TITLE: f32 = 20.0;
    pub const HEADLINE: f32 = 26.0;
    pub const DISPLAY: f32 = 34.0;
}

pub mod motion {
    use std::time::Duration;

    pub const CONTROLS_MS: u64 = 180;
    pub const SWITCH_MS: u64 = 300;
    pub const SELECTION_MS: u64 = 380;
    pub const ENTER_QUICK_MS: u64 = 280;
    pub const PANEL_MS: u64 = 460;
    pub const REVEAL_MS: u64 = 620;
    /// Delay between consecutive items in a staggered entrance.
    pub const STAGGER_MS: u64 = 45;
    /// `--fb-ease`: a long, soft deceleration for entrances.
    pub const EASE: [f32; 4] = [0.22, 1.0, 0.36, 1.0];
    /// `--fb-ease-hover`: opacity and colour never overshoot.
    pub const EASE_HOVER: [f32; 4] = [0.4, 0.0, 0.2, 1.0];

    /// Platform adapters pass the OS setting explicitly so it always overrides configuration.
    pub fn duration(
        milliseconds: u64,
        os_reduced_motion: bool,
        configured_reduced_motion: bool,
    ) -> Duration {
        if os_reduced_motion || configured_reduced_motion {
            Duration::ZERO
        } else {
            Duration::from_millis(milliseconds)
        }
    }
}
