//! The single source of timing for the app. Spatial motion (position, size) uses
//! springs so it stays interruptible; effects (opacity, colour) use the web's easing
//! curves and durations. Everything routes through GPUI Base, which resolves to the
//! settled value when reduced motion is on.

use std::time::Duration;

use gpui::{App, Pixels, Styled, Window, px};
use gpui_base::{
    Easing, Presence, PresencePhase, Spring, Transition, TransitionId, spring, transition,
};

use crate::theme::tokens::motion as tokens;

fn bezier(curve: [f32; 4]) -> Easing {
    Easing::cubic_bezier(curve[0], curve[1], curve[2], curve[3])
        .expect("motion tokens are valid cubic-bezier control points")
}

/// `--fb-ease`: a long, soft deceleration for entrances.
pub fn ease() -> Easing {
    bezier(tokens::EASE)
}

/// `--fb-ease-hover`: opacity and colour never overshoot.
pub fn ease_hover() -> Easing {
    bezier(tokens::EASE_HOVER)
}

fn timed(milliseconds: u64, easing: Easing) -> Transition {
    Transition::new(Duration::from_millis(milliseconds)).easing(easing)
}

/// Hover and press colour changes.
pub fn fade() -> Transition {
    timed(tokens::CONTROLS_MS, ease_hover())
}

/// Selection colour and emphasis changes.
pub fn selection() -> Transition {
    timed(tokens::SELECTION_MS, ease_hover())
}

/// Small surfaces entering: menus, rows, inline notices.
pub fn enter_quick() -> Transition {
    timed(tokens::ENTER_QUICK_MS, ease())
}

/// Pages and panels entering.
pub fn enter() -> Transition {
    timed(tokens::PANEL_MS, ease())
}

/// Hero pieces and long reveals.
pub fn reveal() -> Transition {
    timed(tokens::REVEAL_MS, ease())
}

/// Small, quick moves: presses, toggles, chips.
pub const SPATIAL_FAST: Spring = Spring::new(Duration::from_millis(209)).with_damping(0.86);
/// The default for elements that travel: indicators, cards. Settles with a faint overshoot.
pub const SPATIAL: Spring = Spring::new(Duration::from_millis(322)).with_damping(0.8);
/// Large surfaces and hero pieces.
pub const SPATIAL_SLOW: Spring = Spring::new(Duration::from_millis(468)).with_damping(0.84);
/// Playful emphasis for confirmations and badges.
pub const BOUNCY: Spring = Spring::new(Duration::from_millis(306)).with_damping(0.55);

/// A spring over a pixel distance. The coarser tolerance ends sub-pixel settling early.
pub fn travel(
    id: impl Into<TransitionId>,
    target: Pixels,
    policy: Spring,
    window: &mut Window,
    cx: &mut App,
) -> Pixels {
    spring(id, target, policy.with_epsilon(0.25), window, cx)
}

/// A spring over a normalised `0..1` value.
pub fn spring_unit(
    id: impl Into<TransitionId>,
    target: f32,
    policy: Spring,
    window: &mut Window,
    cx: &mut App,
) -> f32 {
    spring(id, target, policy, window, cx)
}

/// An eased value that follows `target`.
pub fn eased(
    id: impl Into<TransitionId>,
    target: f32,
    policy: Transition,
    window: &mut Window,
    cx: &mut App,
) -> f32 {
    transition(id, target, policy, window, cx)
}

/// Opacity and vertical offset for content that fades and rises into place.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Entrance {
    pub opacity: f32,
    pub offset: Pixels,
}

impl Entrance {
    pub const SETTLED: Self = Self {
        opacity: 1.0,
        offset: Pixels::ZERO,
    };
}

/// Plays once when `id` first renders, `index` stagger steps after its siblings.
/// Leaving a page drops the keyed state, so returning replays the entrance.
pub fn entrance(
    id: impl Into<TransitionId>,
    index: usize,
    window: &mut Window,
    cx: &mut App,
) -> Entrance {
    entrance_with(id, index, 14.0, enter(), window, cx)
}

pub fn entrance_with(
    id: impl Into<TransitionId>,
    index: usize,
    distance: f32,
    policy: Transition,
    window: &mut Window,
    cx: &mut App,
) -> Entrance {
    let delay = Duration::from_millis(tokens::STAGGER_MS * index.min(8) as u64);
    let sample = Presence::new(id, true)
        .transition(policy.delay(delay))
        .sample(window, cx);
    if sample.phase == PresencePhase::Present {
        return Entrance::SETTLED;
    }
    Entrance {
        opacity: sample.progress.clamp(0.0, 1.0),
        offset: px(distance * (1.0 - sample.progress)),
    }
}

/// Applies entrance values without disturbing layout: the element keeps its slot
/// and is only painted offset and faded.
pub trait MotionExt: Styled + Sized {
    fn entering(self, entrance: Entrance) -> Self {
        if entrance == Entrance::SETTLED {
            return self;
        }
        self.opacity(entrance.opacity)
            .relative()
            .top(entrance.offset)
    }
}

impl<T: Styled> MotionExt for T {}
