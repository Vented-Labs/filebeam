import UIKit

/// One vocabulary of touch feedback.
@MainActor
enum Haptics {
    private static let light = UIImpactFeedbackGenerator(style: .light)
    private static let medium = UIImpactFeedbackGenerator(style: .medium)
    private static let rigid = UIImpactFeedbackGenerator(style: .rigid)
    private static let selection = UISelectionFeedbackGenerator()
    private static let notification = UINotificationFeedbackGenerator()

    /// A light acknowledgement for presses on primary controls.
    static func tap() { light.impactOccurred(intensity: 0.7) }
    /// Moving between discrete options: tabs, segments, chips.
    static func tick() { selection.selectionChanged() }
    static func toggle(_ on: Bool) { on ? rigid.impactOccurred(intensity: 0.6) : light.impactOccurred(intensity: 0.5) }
    /// A gesture crossed the point where releasing will act.
    static func threshold() { medium.impactOccurred() }
    static func confirm() { notification.notificationOccurred(.success) }
    static func reject() { notification.notificationOccurred(.error) }
    static func warn() { notification.notificationOccurred(.warning) }
}
