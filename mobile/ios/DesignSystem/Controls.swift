import SwiftUI

/// A switch whose thumb stretches while pressed and springs across on change.
struct FbSwitch: View {
    @Binding var isOn: Bool
    var enabled = true
    var body: some View {
        Button {
            Haptics.toggle(!isOn)
            withMotion(Motion.spatial) { isOn.toggle() }
        } label: { Color.clear.frame(width: 48, height: 28) }
            .buttonStyle(SwitchStyle(isOn: isOn, enabled: enabled))
            .disabled(!enabled)
            .accessibilityRepresentation { Toggle("", isOn: $isOn) }
    }
}

private struct SwitchStyle: ButtonStyle {
    @Environment(\.theme) private var theme
    let isOn: Bool
    let enabled: Bool
    func makeBody(configuration: Configuration) -> some View {
        SwitchVisual(isOn: isOn, enabled: enabled, pressed: configuration.isPressed)
    }
}

/// The drawn switch without behaviour, for rows that own the toggle themselves.
struct SwitchVisual: View {
    @Environment(\.theme) private var theme
    let isOn: Bool
    var enabled = true
    var pressed = false
    var body: some View {
        let track: Color = !enabled ? theme.disabledSurface : isOn ? theme.action : theme.controlBorder
        let thumbWidth: CGFloat = pressed ? 26 : 20
        ZStack(alignment: .leading) {
            Capsule().fill(track)
            Capsule()
                .fill(isOn ? theme.onAction : Color.white)
                .shadow(color: .black.opacity(0.18), radius: 2, y: 1)
                .frame(width: thumbWidth, height: 22)
                .offset(x: isOn ? 45 - thumbWidth : 3)
        }
        .frame(width: 48, height: 28)
        .motion(Motion.spatial, value: isOn)
        .motion(Motion.spatialFast, value: pressed)
        .motion(Motion.fade, value: track)
    }
}

struct Segment<Value: Hashable>: Identifiable {
    let value: Value
    let label: String
    var icon: Icon?
    var customIcon: ApprovedCustomIcon?
    var caption: String?
    var enabled = true
    var id: Value { value }
}

/// A row of mutually exclusive options with a selection pill that slides between them.
/// With captions it renders the taller two-line "transport" variant.
struct SegmentedRail<Value: Hashable>: View {
    @Environment(\.theme) private var theme
    let segments: [Segment<Value>]
    @Binding var selection: Value
    var enabled = true
    @Namespace private var namespace

    var body: some View {
        let tall = segments.contains { $0.caption != nil }
        let height: CGFloat = tall ? 64 : 48
        HStack(spacing: 0) {
            ForEach(segments) { segment in
                let selected = segment.value == selection
                let available = enabled && segment.enabled
                let ink: Color = !available ? theme.disabledText : selected ? theme.text : theme.textMuted
                Button {
                    guard !selected else { return }
                    Haptics.tick()
                    withMotion(Motion.spatial) { selection = segment.value }
                } label: {
                    HStack(spacing: tall ? Space.md : Space.sm) {
                        if let icon = segment.icon { FbIcon(icon, size: tall ? 24 : 20, tint: selected && available ? theme.accentText : ink) }
                        if let custom = segment.customIcon { CustomIcon(icon: custom, size: tall ? 24 : 20).foregroundStyle(selected && available ? theme.accentText : ink) }
                        VStack(alignment: .leading, spacing: 1) {
                            Text(segment.label).fbText(tall ? .smallStrong : .label, color: ink).lineLimit(1)
                            if let caption = segment.caption { Text(caption).fbText(.caption, color: available ? theme.textMuted : theme.disabledText).lineLimit(1) }
                        }
                    }
                    .padding(.horizontal, Space.md)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: tall ? .leading : .center)
                    .background {
                        if selected {
                            Shapes.control.fill(theme.selectedSurface)
                                .overlay(Shapes.control.strokeBorder(theme.choiceBorder, lineWidth: 1))
                                .matchedGeometryEffect(id: "pill", in: namespace)
                        }
                    }
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .disabled(!available)
                .accessibilityAddTraits(selected ? [.isSelected] : [])
            }
        }
        .padding(4)
        .frame(height: height)
        .background(theme.sunken, in: Shapes.card)
        .overlay(Shapes.card.strokeBorder(theme.border, lineWidth: 1))
        .accessibilityElement(children: .contain)
    }
}

/// A compact selectable pill for filters.
struct FilterChip: View {
    @Environment(\.theme) private var theme
    let label: String
    let selected: Bool
    var icon: Icon?
    let action: () -> Void

    init(_ label: String, selected: Bool, icon: Icon? = nil, action: @escaping () -> Void) {
        self.label = label; self.selected = selected; self.icon = icon; self.action = action
    }

    var body: some View {
        Button {
            Haptics.tick()
            action()
        } label: {
            HStack(spacing: Space.xs) {
                if let icon { FbIcon(icon, size: 16, tint: selected ? theme.accentText : theme.textMuted) }
                Text(label).fbText(.smallStrong, color: selected ? theme.accentText : theme.textMuted).lineLimit(1)
            }
            .padding(.horizontal, Space.md)
            .frame(height: 36)
            .background(selected ? theme.selectedSurface : theme.surface, in: Capsule())
            .overlay(Capsule().strokeBorder(selected ? theme.choiceBorder : theme.border, lineWidth: 1))
            .motion(Motion.fade, value: selected)
        }
        .buttonStyle(.pressScale(0.95))
        .accessibilityAddTraits(selected ? [.isSelected] : [])
    }
}

/// A labelled row with a trailing switch; the whole row toggles.
struct SwitchRow: View {
    @Environment(\.theme) private var theme
    let title: String
    @Binding var isOn: Bool
    var description: String?
    var icon: Icon?
    var enabled = true

    init(_ title: String, isOn: Binding<Bool>, description: String? = nil, icon: Icon? = nil, enabled: Bool = true) {
        self.title = title; _isOn = isOn; self.description = description; self.icon = icon; self.enabled = enabled
    }

    var body: some View {
        Button {
            Haptics.toggle(!isOn)
            withMotion(Motion.spatial) { isOn.toggle() }
        } label: {
            HStack(spacing: Space.md) {
                if let icon { FbIcon(icon, size: 22, tint: enabled ? theme.textMuted : theme.disabledText) }
                VStack(alignment: .leading, spacing: 2) {
                    Text(title).fbText(.bodyStrong, color: enabled ? theme.text : theme.disabledText)
                    if let description { Text(description).fbText(.small, color: enabled ? theme.textMuted : theme.disabledText) }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                SwitchVisual(isOn: isOn, enabled: enabled)
            }
            .padding(.vertical, Space.md)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
        .accessibilityRepresentation { Toggle(title, isOn: $isOn) }
    }
}

/// A single-choice list row with a radio-style marker.
struct ChoiceRow: View {
    @Environment(\.theme) private var theme
    let title: String
    let selected: Bool
    var description: String?
    var icon: Icon?
    let action: () -> Void

    init(_ title: String, selected: Bool, description: String? = nil, icon: Icon? = nil, action: @escaping () -> Void) {
        self.title = title; self.selected = selected; self.description = description; self.icon = icon; self.action = action
    }

    var body: some View {
        Button {
            Haptics.tick()
            action()
        } label: {
            HStack(spacing: Space.md) {
                if let icon { FbIcon(icon, size: 22, tint: selected ? theme.accentText : theme.textMuted) }
                VStack(alignment: .leading, spacing: 2) {
                    Text(title).fbText(.bodyStrong)
                    if let description { Text(description).fbText(.small, color: theme.textMuted) }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                ZStack {
                    Circle().strokeBorder(selected ? theme.accentText : theme.controlBorder, lineWidth: 1.5)
                    if selected { Circle().fill(theme.accentText).frame(width: 11, height: 11) }
                }
                .frame(width: 22, height: 22)
            }
            .padding(.horizontal, Space.lg)
            .padding(.vertical, Space.md)
            .background(selected ? theme.selectedSurface : Color.clear, in: Shapes.control)
            .overlay(Shapes.control.strokeBorder(selected ? theme.choiceBorder : theme.border, lineWidth: 1))
            .contentShape(Shapes.control)
            .motion(Motion.fade, value: selected)
        }
        .buttonStyle(.pressScale(0.985))
        .accessibilityAddTraits(selected ? [.isSelected] : [])
    }
}

/// A tappable settings line: icon, title, and the current value on the trailing side.
struct OptionRow: View {
    @Environment(\.theme) private var theme
    let icon: Icon
    let title: String
    let value: String
    var enabled = true
    var highlighted = false
    let action: () -> Void

    init(_ icon: Icon, _ title: String, value: String, enabled: Bool = true, highlighted: Bool = false, action: @escaping () -> Void) {
        self.icon = icon; self.title = title; self.value = value; self.enabled = enabled; self.highlighted = highlighted; self.action = action
    }

    var body: some View {
        Button(action: action) {
            HStack(spacing: Space.md) {
                FbIcon(icon, size: 22, tint: !enabled ? theme.disabledText : highlighted ? theme.accentText : theme.textMuted)
                Text(title).fbText(.bodyStrong, color: enabled ? theme.text : theme.disabledText).lineLimit(1)
                Spacer(minLength: Space.sm)
                Text(value).fbText(.small, color: !enabled ? theme.disabledText : highlighted ? theme.accentText : theme.textMuted).lineLimit(1)
                FbIcon(.chevronRight, size: 16, tint: enabled ? theme.textSubtle : theme.disabledText)
            }
            .padding(.horizontal, Space.lg)
            .padding(.vertical, Space.lg)
            .contentShape(Rectangle())
        }
        .buttonStyle(RowPressStyle())
        .disabled(!enabled)
        .accessibilityValue(value)
    }
}

/// A navigation row inside a panel: icon, title, optional detail and a chevron.
struct LinkRow: View {
    @Environment(\.theme) private var theme
    let icon: Icon
    let title: String
    var detail: String?
    var tint: Color?
    let action: () -> Void

    init(_ icon: Icon, _ title: String, detail: String? = nil, tint: Color? = nil, action: @escaping () -> Void) {
        self.icon = icon; self.title = title; self.detail = detail; self.tint = tint; self.action = action
    }

    var body: some View {
        Button(action: action) {
            HStack(spacing: Space.md) {
                FbIcon(icon, size: 22, tint: tint ?? theme.textMuted)
                Text(title).fbText(.bodyStrong).lineLimit(1)
                Spacer(minLength: Space.sm)
                if let detail { Text(detail).fbText(.small, color: theme.textMuted).lineLimit(1) }
                FbIcon(.chevronRight, size: 16, tint: theme.textSubtle)
            }
            .padding(.horizontal, Space.lg)
            .padding(.vertical, Space.lg)
            .contentShape(Rectangle())
        }
        .buttonStyle(RowPressStyle())
        .accessibilityValue(detail ?? "")
    }
}

/// Rows inside panels tint on press instead of scaling, so lists stay still.
struct RowPressStyle: ButtonStyle {
    @Environment(\.theme) private var theme
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .background(configuration.isPressed ? theme.hover : Color.clear)
            .motion(Motion.fade, value: configuration.isPressed)
    }
}
