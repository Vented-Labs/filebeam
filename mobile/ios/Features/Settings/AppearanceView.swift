import SwiftUI
import FilebeamTheme

/// Theme and colour. The instance colour, the Filebeam presets and a custom colour are offered;
/// changes spread from the control that made them.
struct AppearanceView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var lastTouch = CGPoint(x: 200, y: 300)
    @State private var customOpen = false

    private var appearance: Appearance { model.appearance.appearance }
    private var customSeed: Int? { if case let .custom(seed) = appearance.source { seed } else { nil } }

    var body: some View {
        SubScreen("Appearance") {
            PreviewCard().staggered(0)
            SectionLabel("Theme")
            SegmentedRail(segments: [
                Segment(value: ThemeMode.system, label: "System", icon: .monitor),
                Segment(value: ThemeMode.light, label: "Light", icon: .sun),
                Segment(value: ThemeMode.dark, label: "Dark", icon: .moon),
            ], selection: Binding(get: { appearance.mode }, set: { mode in change(Appearance(mode: mode, source: appearance.source)) }))
            SectionLabel("Color")
            Panel {
                VStack(alignment: .leading, spacing: Space.lg) {
                    Text("Filebeam colors").fbText(.bodyStrong)
                    FlowLayout(spacing: Space.md) {
                        let instanceSeed = model.branding.primaryColor ?? PaletteEngine.defaultPrimary
                        Swatch(label: "Instance color", colors: [Color(rgb: instanceSeed)], selected: appearance.source == .instance, icon: .restore) { change(Appearance(mode: appearance.mode, source: .instance)) }
                        ForEach(ThemePreset.allCases, id: \.self) { preset in
                            Swatch(label: preset.title, colors: [Color(rgb: preset.seed), Color(rgb: preset.endpoint)], selected: appearance.source == .preset(preset)) { change(Appearance(mode: appearance.mode, source: .preset(preset))) }
                        }
                        Swatch(label: "Custom color", colors: [Color(rgb: 0xFF5F6D), Color(rgb: 0xFFC371), Color(rgb: 0x4BC0C8), Color(rgb: 0x8B35FF)], selected: customSeed != nil, icon: .color) {
                            withMotion(Motion.spatial) { customOpen = true }
                            if customSeed == nil { change(Appearance(mode: appearance.mode, source: .custom(model.branding.primaryColor ?? PaletteEngine.defaultPrimary))) }
                        }
                    }
                    if customOpen, let seed = customSeed {
                        // Dragging applies instantly without a reveal, so the picker feels direct.
                        CustomPicker(seed: seed) { model.appearance.appearance = Appearance(mode: appearance.mode, source: .custom($0)) }
                            .transition(.opacity.combined(with: .move(edge: .top)))
                    }
                }
                .padding(Space.lg)
            }
            .motion(Motion.spatial, value: customOpen)
            FbButton("Reset", kind: .quiet, enabled: appearance != Appearance()) { change(Appearance()) }
            Text("Saved on this device.").fbText(.caption, color: theme.textSubtle)
        }
        // Remember where the finger went down so the reveal can start there.
        .simultaneousGesture(SpatialTapGesture(coordinateSpace: .global).onEnded { lastTouch = $0.location })
        .onAppear { customOpen = customSeed != nil }
    }

    private func change(_ value: Appearance) {
        guard value != appearance else { return }
        ThemeReveal.perform(from: lastTouch) { model.appearance.appearance = value }
    }
}

/// A small specimen of the current palette so a change can be judged at a glance.
private struct PreviewCard: View {
    @Environment(\.theme) private var theme
    @State private var on = true
    var body: some View {
        Panel(elevation: 8) {
            VStack(alignment: .leading, spacing: Space.md) {
                HStack(spacing: Space.md) {
                    FilebeamMark(height: 30)
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Preview").fbText(.title)
                        Text("How buttons, switches and progress look.").fbText(.small, color: theme.textMuted)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    FbSwitch(isOn: $on)
                }
                SmoothProgress(on ? 0.68 : 0.24)
                HStack(spacing: Space.md) {
                    FbButton("Send encrypted", icon: .lock, height: 44) {}
                    FbButton("Share link", kind: .secondary, icon: .arrowUpRight, height: 44, fill: false) {}
                }
            }
            .padding(Space.lg)
        }
    }
}

/// A round colour choice. The selected swatch grows a ring and a check mark.
private struct Swatch: View {
    @Environment(\.theme) private var theme
    let label: String
    let colors: [Color]
    let selected: Bool
    var icon: Icon?
    let action: () -> Void

    var body: some View {
        Button {
            Haptics.tick()
            action()
        } label: {
            ZStack {
                Circle().strokeBorder(theme.accentText, lineWidth: selected ? 3 : 0).frame(width: 56, height: 56)
                Circle()
                    .fill(LinearGradient(colors: colors.count == 1 ? colors + colors : colors, startPoint: .topLeading, endPoint: .bottomTrailing))
                    .overlay(Circle().strokeBorder(theme.lineStrong, lineWidth: 1))
                    .frame(width: 44, height: 44)
                    .scaleEffect(selected ? 1 : 0.9)
                    .overlay {
                        if selected { FbIcon(.check, size: 20, tint: .white) }
                        else if let icon { FbIcon(icon, size: 18, tint: .white) }
                    }
            }
            .frame(width: 56, height: 56)
        }
        .buttonStyle(.pressScale(0.9))
        .motion(Motion.bouncy, value: selected)
        .accessibilityLabel(label)
        .accessibilityAddTraits(selected ? [.isSelected] : [])
    }
}

/// Hue and vividness sliders with a hex field. Colours are generated in OKLCH so every hue reads evenly.
private struct CustomPicker: View {
    @Environment(\.theme) private var theme
    let seed: Int
    let onChange: (Int) -> Void
    @State private var hue: Double
    @State private var chroma: Double
    @State private var hex: String

    init(seed: Int, onChange: @escaping (Int) -> Void) {
        self.seed = seed
        self.onChange = onChange
        let initial = ColorMath.oklch(seed)
        _hue = State(initialValue: initial.h)
        _chroma = State(initialValue: min(max(initial.c / 0.3, 0), 1))
        _hex = State(initialValue: ColorMath.format(seed))
    }

    var body: some View {
        VStack(spacing: Space.lg) {
            GradientSlider(label: "Hue", value: Binding(get: { hue / 360 }, set: { hue = $0 * 360; emit() }), colors: (0...12).map { Color(rgb: ColorMath.rgb(0.68, 0.17, Double($0) * 30)) })
            GradientSlider(label: "Vividness", value: Binding(get: { chroma }, set: { chroma = $0; emit() }), colors: [Color(rgb: ColorMath.rgb(0.62, 0, hue)), Color(rgb: ColorMath.rgb(0.62, 0.3, hue))])
            FbTextField($hex, label: "Hex color", placeholder: "#8b35ff", monospaced: true, keyboard: .asciiCapable)
                .onChange(of: hex) { _, text in
                    hex = String(text.prefix(7))
                    guard let color = ColorMath.parse(text.trimmingCharacters(in: .whitespaces)), text.count >= 6 else { return }
                    let parsed = ColorMath.oklch(color)
                    hue = parsed.h
                    chroma = min(max(parsed.c / 0.3, 0), 1)
                    onChange(color)
                }
        }
    }

    private func emit() {
        let color = ColorMath.rgb(0.62, chroma * 0.3, hue)
        hex = ColorMath.format(color)
        onChange(color)
    }
}

/// A slider drawn on its own gradient. It ticks as it moves and is adjustable by assistive technology.
private struct GradientSlider: View {
    @Environment(\.theme) private var theme
    let label: String
    @Binding var value: Double
    let colors: [Color]
    @State private var lastStep = -1

    var body: some View {
        VStack(alignment: .leading, spacing: Space.sm) {
            Text(label).fbText(.smallStrong)
            GeometryReader { proxy in
                let width = max(proxy.size.width, 1)
                let x = min(max(width * value, 18), width - 18)
                ZStack(alignment: .leading) {
                    Capsule().fill(LinearGradient(colors: colors, startPoint: .leading, endPoint: .trailing)).frame(height: 14)
                    Circle().fill(.white).frame(width: 30, height: 30)
                        .overlay(Circle().strokeBorder(theme.border, lineWidth: 1))
                        .shadow(color: theme.shadow, radius: 2, y: 1)
                        .offset(x: x - 15)
                }
                .frame(height: 36)
                .contentShape(Rectangle())
                .gesture(DragGesture(minimumDistance: 0).onChanged { drag in
                    let next = min(max(drag.location.x / width, 0), 1)
                    let step = Int((next * 24).rounded())
                    if step != lastStep { lastStep = step; Haptics.tick() }
                    value = next
                })
            }
            .frame(height: 36)
            .accessibilityElement()
            .accessibilityLabel(label)
            .accessibilityValue("\(Int(value * 100)) percent")
            .accessibilityAdjustableAction { direction in
                switch direction { case .increment: value = min(value + 0.05, 1); case .decrement: value = max(value - 0.05, 0); @unknown default: break }
            }
        }
    }
}
