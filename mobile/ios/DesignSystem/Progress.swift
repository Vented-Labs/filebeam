import SwiftUI

/// A determinate bar that eases towards its target instead of jumping, as the web's
/// SmoothProgress does: bigger jumps take longer (600 to 1800 ms). A nil `fraction` renders an
/// indeterminate sweep.
struct SmoothProgress: View {
    @Environment(\.theme) private var theme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    let fraction: Double?
    var height: CGFloat = 6
    var fill: Color?
    var track: Color?
    @State private var shown: Double = 0
    @State private var sweep: Double = 0

    init(_ fraction: Double?, height: CGFloat = 6, fill: Color? = nil, track: Color? = nil) {
        self.fraction = fraction; self.height = height; self.fill = fill; self.track = track
    }

    var body: some View {
        GeometryReader { proxy in
            let width = proxy.size.width
            ZStack(alignment: .leading) {
                Capsule().fill(track ?? theme.progressTrack)
                if fraction == nil {
                    let band = width * 0.32
                    Capsule()
                        .fill(LinearGradient(colors: [(fill ?? theme.progressFill).opacity(0), fill ?? theme.progressFill, (fill ?? theme.progressFill).opacity(0)], startPoint: .leading, endPoint: .trailing))
                        .frame(width: band)
                        .offset(x: -band + (width + band) * sweep)
                } else {
                    // A sliver stays visible below 1% so a started transfer never looks empty.
                    let filled = max(width * shown, shown > 0 ? height : 0)
                    Capsule().fill(fill ?? theme.progressFill).frame(width: max(0, filled))
                }
            }
            .clipShape(Capsule())
        }
        .frame(height: height)
        .onChange(of: fraction, initial: true) { _, target in
            guard let target = target.map({ min(max($0, 0), 1) }) else { return }
            if reduceMotion || target < shown { shown = target }
            else { withAnimation(.timingCurve(0.22, 1, 0.36, 1, duration: 0.6 + 1.2 * abs(target - shown))) { shown = target } }
        }
        .onAppear {
            guard fraction == nil, !reduceMotion else { return }
            withAnimation(.linear(duration: 1.4).repeatForever(autoreverses: false)) { sweep = 1 }
        }
        .accessibilityElement()
        .accessibilityLabel("Progress")
        .accessibilityValue(fraction.map { progressLabel($0) } ?? "In progress")
    }
}

/// "<1%" until one percent is reached, then whole percentages.
func progressLabel(_ fraction: Double) -> String {
    let percent = Int(min(max(fraction, 0), 1) * 100)
    return fraction > 0 && percent < 1 ? "<1%" : "\(percent)%"
}
