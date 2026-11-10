import SwiftUI

/// The single source of timing for the app. Spatial motion (position, size, shape) uses springs
/// so it stays interruptible; effects (opacity, colour) use the web's easing curve and durations.
/// No screen defines its own durations.
enum Motion {
    private static func spring(_ stiffness: Double, _ ratio: Double) -> Animation {
        .interpolatingSpring(mass: 1, stiffness: stiffness, damping: 2 * ratio * stiffness.squareRoot(), initialVelocity: 0)
    }

    /// Small, quick moves: presses, toggles, chips.
    static let spatialFast = spring(900, 0.86)
    /// The default for elements that travel: indicators, cards, sheets. Settles with a faint overshoot.
    static let spatial = spring(380, 0.8)
    /// Large surfaces and hero pieces.
    static let spatialSlow = spring(180, 0.84)
    /// Playful emphasis for confirmations and badges.
    static let bouncy = spring(420, 0.55)
    /// Opacity and colour never overshoot (`--fb-ease-hover`).
    static let fade = Animation.timingCurve(0.4, 0, 0.2, 1, duration: 0.18)
    static let selection = Animation.timingCurve(0.4, 0, 0.2, 1, duration: 0.38)
    /// `--fb-ease`: a long, soft deceleration for entrances.
    static let enter = Animation.timingCurve(0.22, 1, 0.36, 1, duration: 0.46)
    static let enterQuick = Animation.timingCurve(0.22, 1, 0.36, 1, duration: 0.28)
    static let exit = Animation.timingCurve(0.4, 0, 1, 1, duration: 0.2)
    static let reveal = Animation.timingCurve(0.22, 1, 0.36, 1, duration: 0.62)
    /// Delay between consecutive items in a staggered entrance.
    static let stagger: Double = 0.045
}

extension View {
    /// Animates `value` with `animation`, or snaps when the person turned motion off.
    func motion<V: Equatable>(_ animation: Animation, value: V) -> some View { modifier(MotionModifier(animation: animation, value: value)) }
}

private struct MotionModifier<V: Equatable>: ViewModifier {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    let animation: Animation
    let value: V
    func body(content: Content) -> some View { content.animation(reduceMotion ? nil : animation, value: value) }
}

/// Runs `body` inside an animation unless motion is reduced.
@MainActor func withMotion<Result>(_ animation: Animation, reduce: Bool = UIAccessibility.isReduceMotionEnabled, _ body: () throws -> Result) rethrows -> Result {
    try withAnimation(reduce ? nil : animation, body)
}
