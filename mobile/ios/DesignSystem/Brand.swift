import SwiftUI

/// Geometry of the folded "F and beam" mark, from `backend/public/brand/filebeam-mark.svg` (168 x 244 units).
private enum MarkGeometry {
    static let width: CGFloat = 168
    static let height: CGFloat = 244
    static let body = svgPath("M14 65L140 3C153 -3 168 5 168 20L168 42C168 57 165 63 153 69L81 105L145 134L79 166C90 171 93 178 93 191L93 225C93 239 84 248 70 242L16 216C5 211 0 203 0 191L0 87C0 77 5 69 14 65Z")
    static let fold = svgPath("M14 65C5 69 0 77 0 87L0 158C0 150 4 145 12 141L81 105L14 73C8 70 8 68 14 65Z")
    static let top = svgPath("M14 65L140 3C153 -3 168 5 168 20L168 42C168 57 165 63 153 69L81 105L14 73C8 70 8 68 14 65Z")
    static let beam = svgPath("M12 141L81 105L145 134L79 166Z")
    static let stem = svgPath("M12 141L79 166C90 171 93 178 93 191L93 225C93 239 84 248 70 242L16 216C5 211 0 203 0 191L0 158C0 150 4 145 12 141Z")

    /// Parses the absolute M, L, C and Z commands the brand paths use.
    static func svgPath(_ data: String) -> Path {
        var path = Path()
        var numbers: [CGFloat] = []
        var command: Character = "M"
        var token = ""
        func flushToken() {
            if let value = Double(token) { numbers.append(CGFloat(value)) }
            token = ""
        }
        func apply() {
            switch command {
            case "M": if numbers.count >= 2 { path.move(to: CGPoint(x: numbers[0], y: numbers[1])) }
            case "L": if numbers.count >= 2 { path.addLine(to: CGPoint(x: numbers[0], y: numbers[1])) }
            case "C": if numbers.count >= 6 { path.addCurve(to: CGPoint(x: numbers[4], y: numbers[5]), control1: CGPoint(x: numbers[0], y: numbers[1]), control2: CGPoint(x: numbers[2], y: numbers[3])) }
            case "Z": path.closeSubpath()
            default: break
            }
            numbers.removeAll()
        }
        for character in data {
            if character.isLetter {
                flushToken(); apply(); command = character
                if character == "Z" { apply() }
            } else if character == " " || character == "," {
                flushToken()
            } else if character == "-" && !token.isEmpty {
                flushToken(); token = "-"
            } else {
                token.append(character)
            }
        }
        flushToken(); apply()
        return path
    }
}

/// The Filebeam mark, drawn from the brand gradient of the active palette so it follows
/// presets, the instance colour and custom colours alike.
struct FilebeamMark: View {
    @Environment(\.theme) private var theme
    var height: CGFloat = 28

    var body: some View {
        let gradient = theme.brandGradient
        Canvas { context, size in
            let factor = size.height / MarkGeometry.height
            context.scaleBy(x: factor, y: factor)
            let seed = gradient[2], tip = gradient[4]
            func lerp(_ a: Color, _ b: Color, _ t: Double) -> Color { a.blended(with: b, by: t) }
            context.fill(MarkGeometry.body, with: .color(seed))
            context.fill(MarkGeometry.fold, with: .linearGradient(Gradient(colors: [lerp(tip, .white, 0.12), lerp(tip, .white, 0.45)]), startPoint: CGPoint(x: 0, y: 91), endPoint: CGPoint(x: 81, y: 87)))
            context.fill(MarkGeometry.top, with: .linearGradient(Gradient(colors: [gradient[1], seed, tip]), startPoint: CGPoint(x: 8, y: 76), endPoint: CGPoint(x: 150, y: 5)))
            context.fill(MarkGeometry.beam, with: .linearGradient(Gradient(colors: [gradient[0], gradient[1], seed]), startPoint: CGPoint(x: 13, y: 135), endPoint: CGPoint(x: 116, y: 106)))
            context.fill(MarkGeometry.stem, with: .linearGradient(Gradient(colors: [seed, lerp(seed, tip, 0.25)]), startPoint: CGPoint(x: -8, y: 129), endPoint: CGPoint(x: 63, y: 216)))
        }
        .frame(width: height * (MarkGeometry.width / MarkGeometry.height), height: height)
        .accessibilityHidden(true)
    }
}

/// The stacked-card emblem from the web drop zone: an image card and a code card fanned behind
/// a front card carrying the mark, sealed with a small lock.
///
/// `engaged` fans the cards wider and lifts them (press, drag-over). `sealed` draws the lock closed
/// and pulls the stack together, used once a transfer is protected.
struct Emblem: View {
    @Environment(\.theme) private var theme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    var engaged = false
    var sealed = false
    var size: CGFloat = 132

    var body: some View {
        let fan: CGFloat = sealed ? 0.25 : engaged ? 1 : 0.62
        let lift: CGFloat = engaged ? 1 : 0
        let cardWidth = size * 0.52, cardHeight = size * 0.62
        TimelineView(.animation(minimumInterval: 1 / 30, paused: reduceMotion)) { timeline in
            let phase = reduceMotion ? 0 : timeline.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 6.4) / 6.4
            let drift = reduceMotion ? 0 : sin(phase * 2 * .pi)
            ZStack {
                backCard(icon: .image, rotation: -21, shift: -size * 0.36, phase: 1, fan: fan, lift: lift, drift: drift, width: cardWidth, height: cardHeight)
                backCard(icon: .code, rotation: 23, shift: size * 0.36, phase: -1, fan: fan, lift: lift, drift: drift, width: cardWidth, height: cardHeight)
                RoundedRectangle(cornerRadius: 16, style: .continuous)
                    .fill(LinearGradient(colors: [theme.cardFront, theme.cardFrontEnd], startPoint: .topLeading, endPoint: .bottomTrailing))
                    .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).strokeBorder(theme.controlBorder.opacity(0.7), lineWidth: 1))
                    .overlay(FilebeamMark(height: size * 0.26))
                    .frame(width: cardWidth * 1.08, height: cardHeight * 1.08)
                    .shadow(color: theme.shadow, radius: 18, y: 8)
                    .offset(y: -lift * 7 - CGFloat(drift) * 2.5)
                // The seal sits on the front card's lower corner.
                FbIcon(.lock, size: size * 0.11, tint: sealed ? theme.onAction : theme.textMuted)
                    .frame(width: size * 0.2, height: size * 0.2)
                    .background(sealed ? theme.action : theme.raised, in: Circle())
                    .overlay(Circle().strokeBorder(sealed ? theme.actionBorder : theme.controlBorder, lineWidth: 1))
                    .scaleEffect(sealed ? 1.15 : 1)
                    .offset(x: cardWidth * 0.5, y: cardHeight * 0.44 - lift * 7 - CGFloat(drift) * 2.5)
            }
        }
        .frame(width: size * 1.35, height: size)
        .motion(Motion.bouncy, value: fan)
        .motion(Motion.spatial, value: lift)
        .motion(Motion.bouncy, value: sealed)
        .accessibilityHidden(true)
    }

    private func backCard(icon: Icon, rotation: Double, shift: CGFloat, phase: Double, fan: CGFloat, lift: CGFloat, drift: Double, width: CGFloat, height: CGFloat) -> some View {
        RoundedRectangle(cornerRadius: 16, style: .continuous)
            .fill(LinearGradient(colors: [theme.cardBack, theme.cardBackEnd], startPoint: .topLeading, endPoint: .bottomTrailing))
            .overlay(RoundedRectangle(cornerRadius: 16, style: .continuous).strokeBorder(theme.controlBorder.opacity(0.55), lineWidth: 1))
            .overlay(FbIcon(icon, size: size * 0.2, tint: theme.textMuted.opacity(0.7)))
            .frame(width: width, height: height)
            .rotationEffect(.degrees(rotation * fan), anchor: .bottom)
            .offset(x: shift * fan, y: 6 - lift * 5 + CGFloat(drift * phase * 2))
    }
}

/// The page backdrop: the palette background lit by two soft ellipses, as on the web.
/// `energy` in 0...1 brightens the glow while work is in flight.
struct AmbientBackground: View {
    @Environment(\.theme) private var theme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    var energy: Double = 0

    var body: some View {
        TimelineView(.animation(minimumInterval: 1 / 20, paused: reduceMotion)) { timeline in
            let phase = reduceMotion ? 0 : timeline.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 18) / 18
            let drift = reduceMotion ? 0 : sin(phase * 2 * .pi)
            Canvas { context, size in
                let boost = 1 + min(max(energy, 0), 1) * 1.4
                func glow(_ color: Color, _ centerX: Double, _ centerY: Double, _ radiusX: Double, _ radiusY: Double) {
                    let center = CGPoint(x: size.width * centerX, y: size.width * centerY)
                    let rx = size.width * radiusX, ry = size.width * radiusY
                    var layer = context
                    layer.translateBy(x: center.x, y: center.y)
                    layer.scaleBy(x: 1, y: ry / rx)
                    let rect = CGRect(x: -rx, y: -rx, width: rx * 2, height: rx * 2)
                    layer.fill(Path(ellipseIn: rect), with: .radialGradient(Gradient(colors: [color.opacity(min(1, boost)), color.opacity(0)]), center: .zero, startRadius: 0, endRadius: rx))
                }
                glow(theme.glowPrimary, 0.49 + drift * 0.05, 0.24, 0.95, 0.8)
                glow(theme.glowSecondary, 0.92 - drift * 0.06, 0.78 + drift * 0.04, 0.8, 0.62)
            }
        }
        .background(theme.background)
        .ignoresSafeArea()
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }
}

extension Color {
    /// Blends two colours in sRGB; good enough for decorative gradients.
    func blended(with other: Color, by amount: Double) -> Color {
        let a = UIColor(self).resolvedComponents, b = UIColor(other).resolvedComponents
        let t = min(max(amount, 0), 1)
        return Color(.sRGB, red: a.0 + (b.0 - a.0) * t, green: a.1 + (b.1 - a.1) * t, blue: a.2 + (b.2 - a.2) * t, opacity: a.3 + (b.3 - a.3) * t)
    }
}

extension UIColor {
    var resolvedComponents: (Double, Double, Double, Double) {
        var r: CGFloat = 0, g: CGFloat = 0, b: CGFloat = 0, a: CGFloat = 0
        getRed(&r, green: &g, blue: &b, alpha: &a)
        return (Double(r), Double(g), Double(b), Double(a))
    }
}
