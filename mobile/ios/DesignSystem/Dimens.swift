import SwiftUI

/// Spacing on the web's 4 pt grid.
enum Space {
    static let xxs: CGFloat = 2
    static let xs: CGFloat = 4
    static let sm: CGFloat = 8
    static let md: CGFloat = 12
    static let lg: CGFloat = 16
    static let xl: CGFloat = 20
    static let xxl: CGFloat = 24
    static let xxxl: CGFloat = 32
    /// Horizontal page margin.
    static let gutter: CGFloat = 20
    /// Minimum interactive size.
    static let touch: CGFloat = 48
    static let control: CGFloat = 52
}

/// Corner radii from the web tokens, with a touch-sized control radius.
enum Radius {
    static let sm: CGFloat = 8
    static let control: CGFloat = 14
    static let card: CGFloat = 18
    static let panel: CGFloat = 24
    static let sheet: CGFloat = 28
}

enum Shapes {
    static var sm: RoundedRectangle { RoundedRectangle(cornerRadius: Radius.sm, style: .continuous) }
    static var control: RoundedRectangle { RoundedRectangle(cornerRadius: Radius.control, style: .continuous) }
    static var card: RoundedRectangle { RoundedRectangle(cornerRadius: Radius.card, style: .continuous) }
    static var panel: RoundedRectangle { RoundedRectangle(cornerRadius: Radius.panel, style: .continuous) }
    static var pill: Capsule { Capsule(style: .continuous) }
}

/// Content never grows wider than this on iPad.
let contentMaxWidth: CGFloat = 640
