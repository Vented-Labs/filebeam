import SwiftUI

private struct Chapter: Identifiable {
    let id: Int
    let eyebrow: String
    let title: String
    let body: String
}

/// A short, animated introduction to what Filebeam does, mirroring the chapters of the web
/// Features page. Shown once on first run and available again from About.
struct TourView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.dismiss) private var dismiss
    @State private var page = 0

    private var chapters: [Chapter] {
        [
            Chapter(id: 0, eyebrow: "Welcome", title: "End-to-end encrypted.", body: "Files and notes are encrypted on your device. Share a link, or send straight to a friend."),
            Chapter(id: 1, eyebrow: "Sharing", title: "Fast and simple.", body: "Store a transfer to download later, send it live between devices, or use Turbo to share the link while it is still uploading."),
            Chapter(id: 2, eyebrow: "Notes", title: "Send the context, too.", body: "Attach an encrypted note to your files or share one on its own. Notes can burn after the first read."),
            Chapter(id: 3, eyebrow: "Friends", title: "Send to a friend.", body: "Add a contact once and deliver to their encrypted inbox. You decide who can send to you."),
            Chapter(id: 4, eyebrow: "Privacy", title: "Private by design.", body: "Keep the key in the link or share it separately. The server only ever stores ciphertext."),
            Chapter(id: 5, eyebrow: "Devices", title: "Your computers, by name.", body: model.branding.deviceRendezvousEnabled ? "Pair computers with the Beam CLI and review or revoke them from My devices." : "Device pairing is done with the Beam CLI. It is not enabled on this instance yet."),
            Chapter(id: 6, eyebrow: "Open source", title: "Yours to inspect.", body: "Filebeam is MIT licensed. Read the source, or host your own instance and point this app at it."),
        ]
    }

    var body: some View {
        let chapters = chapters
        let last = page == chapters.count - 1
        VStack(spacing: 0) {
            HStack {
                HStack(spacing: 6) {
                    ForEach(chapters) { chapter in
                        Capsule()
                            .fill(chapter.id <= page ? theme.accentText : theme.controlBorder.opacity(0.5))
                            .frame(width: chapter.id == page ? 26 : 8, height: 6)
                    }
                }
                .motion(Motion.spatial, value: page)
                .frame(maxWidth: .infinity, alignment: .leading)
                FbButton("Skip", kind: .quiet, height: 40, fill: false) { finish() }
            }
            .frame(maxWidth: contentMaxWidth)
            .padding(.horizontal, Space.gutter)
            .padding(.vertical, Space.sm)
            TabView(selection: $page) {
                ForEach(chapters) { chapter in
                    VStack(spacing: Space.xl) {
                        Art(kind: chapter.id).frame(height: 260)
                        VStack(spacing: Space.md) {
                            Text(chapter.eyebrow).fbText(.eyebrow, color: theme.accentText)
                            Text(chapter.title).fbText(.display).multilineTextAlignment(.center).accessibilityAddTraits(.isHeader)
                            Text(chapter.body).fbText(.body, color: theme.textMuted).multilineTextAlignment(.center)
                        }
                        .frame(maxWidth: 420)
                    }
                    .padding(.horizontal, Space.xxl)
                    .tag(chapter.id)
                }
            }
            .tabViewStyle(.page(indexDisplayMode: .never))
            .onChange(of: page) { _, _ in Haptics.tick() }
            FbButton(last ? "Get started" : "Next", trailingIcon: .arrowRight) {
                if last { finish() } else { withMotion(Motion.spatial) { page += 1 } }
            }
            .frame(maxWidth: contentMaxWidth)
            .padding(.horizontal, Space.gutter)
            .padding(.vertical, Space.lg)
        }
        .toolbar(.hidden, for: .navigationBar)
        .enableSwipeBack()
    }

    private func finish() {
        model.appearance.hasSeenTour = true
        if model.path.last == .tour { model.pop() } else { dismiss() }
    }
}

private struct Art: View {
    @Environment(\.theme) private var theme
    let kind: Int
    var body: some View {
        switch kind {
        case 0: Emblem(engaged: true, size: 170)
        case 1: Beam(from: .customHTTP, to: .customWebRTC)
        case 2: Tile(icon: .note) {
            VStack(alignment: .leading, spacing: 6) {
                ForEach([(0.9, theme.editor.keyword), (0.6, theme.editor.string), (0.75, theme.editor.property), (0.4, theme.editor.comment)], id: \.0) { width, color in
                    GeometryReader { proxy in Capsule().fill(color.opacity(0.8)).frame(width: proxy.size.width * width, height: 8) }.frame(height: 8)
                }
            }
        }
        case 3: Beam(from: .icon(.user), to: .icon(.users))
        case 4: Tile(icon: .shield) {
            VStack(alignment: .leading, spacing: Space.sm) {
                StatusPill("Server sees: sizes, timing, expiry", tone: .neutral)
                StatusPill("Never: names, contents, keys", tone: .success, icon: .lock)
            }
        }
        case 5: Beam(from: .icon(.laptop), to: .icon(.monitor))
        default: Tile(icon: .heart) { Text("MIT").fbText(.display, color: theme.accentText) }
        }
    }
}

private enum EndpointIcon { case icon(Icon), customHTTP, customWebRTC }

/// A framed illustration: an icon badge above a small piece of content.
private struct Tile<Content: View>: View {
    @Environment(\.theme) private var theme
    let icon: Icon
    @ViewBuilder let content: () -> Content
    var body: some View {
        VStack(alignment: .leading, spacing: Space.lg) {
            FbIcon(icon, size: 26, tint: theme.accentText)
                .frame(width: 52, height: 52)
                .background(theme.accentSurface, in: Shapes.card)
                .overlay(Shapes.card.strokeBorder(theme.choiceBorder.opacity(0.5), lineWidth: 1))
            content()
        }
        .padding(Space.xl)
        .frame(width: 240)
        .background(LinearGradient(colors: [theme.cardFront, theme.cardFrontEnd], startPoint: .topLeading, endPoint: .bottomTrailing), in: Shapes.panel)
        .overlay(Shapes.panel.strokeBorder(theme.controlBorder.opacity(0.6), lineWidth: 1))
    }
}

/// Two endpoints joined by the beam: a gradient line with an encrypted packet travelling along it.
private struct Beam: View {
    @Environment(\.theme) private var theme
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    let from: EndpointIcon
    let to: EndpointIcon
    var body: some View {
        HStack(spacing: 0) {
            Endpoint(icon: from)
            TimelineView(.animation(paused: reduceMotion)) { timeline in
                let travel = reduceMotion ? 0.5 : timeline.date.timeIntervalSinceReferenceDate.truncatingRemainder(dividingBy: 1.3) / 1.3
                Canvas { context, size in
                    let y = size.height / 2
                    var line = Path()
                    line.move(to: CGPoint(x: 0, y: y))
                    line.addLine(to: CGPoint(x: size.width, y: y))
                    context.stroke(line, with: .linearGradient(Gradient(colors: theme.brandGradient), startPoint: .zero, endPoint: CGPoint(x: size.width, y: 0)), style: StrokeStyle(lineWidth: 3, lineCap: .round))
                    let x = size.width * travel
                    context.fill(Path(ellipseIn: CGRect(x: x - 12, y: y - 12, width: 24, height: 24)), with: .color(theme.brandGradient[4].opacity(0.25)))
                    context.fill(Path(ellipseIn: CGRect(x: x - 5, y: y - 5, width: 10, height: 10)), with: .color(.white))
                }
            }
            .frame(width: 120, height: 40)
            Endpoint(icon: to)
        }
    }
}

private struct Endpoint: View {
    @Environment(\.theme) private var theme
    let icon: EndpointIcon
    var body: some View {
        Group {
            switch icon {
            case let .icon(value): FbIcon(value, size: 36, tint: theme.accentText)
            case .customHTTP: CustomIcon(icon: .httpServer, size: 36).foregroundStyle(theme.accentText)
            case .customWebRTC: CustomIcon(icon: .webrtcP2p, size: 36).foregroundStyle(theme.accentText)
            }
        }
        .frame(width: 84, height: 84)
        .background(LinearGradient(colors: [theme.cardFront, theme.cardFrontEnd], startPoint: .topLeading, endPoint: .bottomTrailing), in: Shapes.panel)
        .overlay(Shapes.panel.strokeBorder(theme.controlBorder.opacity(0.6), lineWidth: 1))
    }
}
