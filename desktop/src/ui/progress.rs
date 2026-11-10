//! A determinate bar that eases towards its target instead of jumping, as the web's
//! SmoothProgress does. `None` renders an indeterminate sweep.

use std::time::Duration;

use gpui::{
    Animation, AnimationExt, App, ElementId, Hsla, IntoElement, ParentElement, RenderOnce, Styled,
    Window, div, prelude::FluentBuilder, px, relative,
};
use gpui_base::{Transition, transition};

use crate::{theme, ui::motion};

#[derive(IntoElement)]
pub struct Progress {
    id: ElementId,
    fraction: Option<f32>,
    height: f32,
    fill: Option<Hsla>,
    track: Option<Hsla>,
}

pub fn progress(id: impl Into<ElementId>, fraction: Option<f32>) -> Progress {
    Progress {
        id: id.into(),
        fraction: fraction.map(|value| value.clamp(0., 1.)),
        height: 6.,
        fill: None,
        track: None,
    }
}

impl Progress {
    pub fn height(mut self, height: f32) -> Self {
        self.height = height;
        self
    }

    pub fn fill(mut self, color: Hsla) -> Self {
        self.fill = Some(color);
        self
    }

    pub fn track(mut self, color: Hsla) -> Self {
        self.track = Some(color);
        self
    }
}

impl RenderOnce for Progress {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let p = theme::palette(cx);
        let fill = self.fill.unwrap_or(p.progress_fill);
        let track = div()
            .relative()
            .w_full()
            .h(px(self.height))
            .flex_none()
            .rounded_full()
            .overflow_hidden()
            .bg(self.track.unwrap_or(p.progress_track));
        match self.fraction {
            Some(target) => {
                let shown = transition(
                    (self.id, "fill"),
                    target,
                    Transition::new(Duration::from_millis(720)).easing(motion::ease()),
                    window,
                    cx,
                );
                // A sliver stays visible below 1% so a started transfer never looks empty.
                track.when(shown > 0., |this| {
                    this.child(
                        div()
                            .h_full()
                            .min_w(px(self.height))
                            .w(relative(shown))
                            .rounded_full()
                            .bg(fill),
                    )
                })
            }
            None => track.child(
                div()
                    .absolute()
                    .top_0()
                    .bottom_0()
                    .w(relative(0.32))
                    .rounded_full()
                    .bg(fill)
                    .with_animation(
                        (self.id, "sweep"),
                        Animation::new(Duration::from_millis(1400)).repeat(),
                        |this, delta| this.left(relative(-0.32 + 1.32 * delta)),
                    ),
            ),
        }
    }
}

/// "<1%" until one percent is reached, then whole percentages.
pub fn percent_label(fraction: f32) -> String {
    let percent = (fraction.clamp(0., 1.) * 100.) as u32;
    if fraction > 0. && percent < 1 {
        "<1%".into()
    } else {
        format!("{percent}%")
    }
}

#[cfg(test)]
mod tests {
    use super::percent_label;

    #[test]
    fn percentages_never_round_a_started_transfer_down_to_zero() {
        assert_eq!(percent_label(0.), "0%");
        assert_eq!(percent_label(0.004), "<1%");
        assert_eq!(percent_label(0.5), "50%");
        assert_eq!(percent_label(0.999), "99%");
        assert_eq!(percent_label(4.), "100%");
    }
}
