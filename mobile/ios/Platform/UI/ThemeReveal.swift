import UIKit

/// Freezes the current look in a snapshot and wipes it away in a growing circle from where the
/// person tapped, so a theme change spreads from the control that made it.
@MainActor
enum ThemeReveal {
    static func perform(from origin: CGPoint, apply: () -> Void) {
        guard !UIAccessibility.isReduceMotionEnabled,
              let window = UIApplication.shared.connectedScenes.compactMap({ $0 as? UIWindowScene }).flatMap(\.windows).first(where: \.isKeyWindow),
              let snapshot = window.snapshotView(afterScreenUpdates: false) else {
            apply()
            return
        }
        snapshot.frame = window.bounds
        snapshot.isUserInteractionEnabled = false
        window.addSubview(snapshot)
        apply()
        let bounds = window.bounds
        let maxRadius = hypot(max(origin.x, bounds.width - origin.x), max(origin.y, bounds.height - origin.y))
        func path(radius: CGFloat) -> CGPath {
            let path = UIBezierPath(rect: bounds)
            path.append(UIBezierPath(ovalIn: CGRect(x: origin.x - radius, y: origin.y - radius, width: radius * 2, height: radius * 2)))
            return path.cgPath
        }
        let mask = CAShapeLayer()
        mask.fillRule = .evenOdd
        mask.path = path(radius: maxRadius)
        snapshot.layer.mask = mask
        let animation = CABasicAnimation(keyPath: "path")
        animation.fromValue = path(radius: 0)
        animation.toValue = path(radius: maxRadius)
        animation.duration = 0.62
        animation.timingFunction = CAMediaTimingFunction(controlPoints: 0.22, 1, 0.36, 1)
        CATransaction.begin()
        CATransaction.setCompletionBlock { snapshot.removeFromSuperview() }
        mask.add(animation, forKey: "reveal")
        CATransaction.commit()
    }
}
