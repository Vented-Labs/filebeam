import SwiftUI
import FilebeamDomain

/// A file or item line: icon tile, name, detail and a trailing control.
struct ItemRow<Trailing: View>: View {
    @Environment(\.theme) private var theme
    let name: String
    let detail: String
    var icon: Icon?
    var error = false
    @ViewBuilder var trailing: () -> Trailing

    init(name: String, detail: String, icon: Icon? = nil, error: Bool = false, @ViewBuilder trailing: @escaping () -> Trailing = { EmptyView() }) {
        self.name = name; self.detail = detail; self.icon = icon; self.error = error; self.trailing = trailing
    }

    var body: some View {
        HStack(spacing: Space.md) {
            FbIcon(icon ?? fileIcon(for: name), size: 22, tint: theme.accentText)
                .frame(width: 44, height: 44)
                .background(theme.accentSurface, in: Shapes.control)
                .overlay(Shapes.control.strokeBorder(theme.border, lineWidth: 1))
            VStack(alignment: .leading, spacing: 2) {
                Text(name).fbText(.bodyStrong).lineLimit(1).truncationMode(.middle)
                Text(detail).fbText(.small, color: error ? theme.danger.ink : theme.textMuted).lineLimit(1)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            trailing()
        }
        .padding(.leading, Space.md)
        .padding(.trailing, Space.sm)
        .padding(.vertical, Space.md)
        .background(theme.surface, in: Shapes.card)
        .overlay(Shapes.card.strokeBorder(error ? theme.danger.border : theme.border, lineWidth: 1))
    }
}

/// Shows when a transfer expires, refreshed every 30 seconds; the absolute time is its accessible description.
struct ExpiryChip: View {
    @Environment(\.theme) private var theme
    let expiresAt: Date
    @State private var now = Date()

    var body: some View {
        let expired = expiresAt <= now
        VStack(spacing: 2) {
            StatusPill(expired ? "Expired" : "Expires \(formatRelative(expiresAt, now: now))", tone: expired ? .danger : .neutral, icon: .clock)
            Text(formatAbsolute(expiresAt)).fbText(.caption, color: theme.textSubtle)
        }
        .task {
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(30))
                now = Date()
            }
        }
        .accessibilityElement(children: .combine)
    }
}

/// The `beam down` command for people who prefer a terminal, or why there is none.
struct CliCard: View {
    @Environment(\.theme) private var theme
    @Environment(AppModel.self) private var model
    let link: String
    var available = true
    var reason = "This transfer is not currently available for CLI download."
    @State private var command: String?

    var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: Space.md) {
                HStack(spacing: Space.md) {
                    FbIcon(.terminal, size: 20, tint: theme.accentText).frame(width: 40, height: 40).background(theme.accentSurface, in: Shapes.control)
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Download with CLI").fbText(.bodyStrong)
                        Text("Run this command in a terminal.").fbText(.small, color: theme.textMuted)
                    }
                }
                if let command {
                    CopyField(icon: .code, value: command, label: "Copy command") {
                        Clipboard.copySensitive(command)
                        model.toast("Command copied", tone: .success)
                    }
                } else {
                    Text(available ? reason : reason).fbText(.small, color: theme.textMuted)
                }
            }
            .padding(Space.lg)
        }
        .task(id: link) {
            command = available ? try? await model.service.downloadCommand(link: link) : nil
        }
    }
}

/// An attached note shown with a received transfer, with a copy action.
struct AttachedNoteCard: View {
    @Environment(\.theme) private var theme
    @Environment(AppModel.self) private var model
    let note: AttachedNoteDraft

    var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: Space.md) {
                HStack(spacing: Space.sm) {
                    FbIcon(.note, size: 20)
                    Text(note.title.isEmpty ? "Attached note" : note.title).fbText(.bodyStrong).lineLimit(1).frame(maxWidth: .infinity, alignment: .leading)
                    IconAction(.copy, "Copy") {
                        Clipboard.copySensitive(note.text)
                        model.toast("Copied", tone: .success)
                    }
                }
                CodeEditor(note.text, language: note.language.rawValue, minHeight: 96, maxHeight: 260)
            }
            .padding(Space.lg)
        }
    }
}

/// A calm holding state with the emblem, used while the app talks to the instance.
struct WorkingPanel: View {
    @Environment(\.theme) private var theme
    let title: String
    let body_: String
    init(_ title: String, body: String) { self.title = title; body_ = body }
    var body: some View {
        Panel {
            VStack(spacing: Space.lg) {
                Emblem(engaged: true, size: 108)
                Text(title).fbText(.headline).multilineTextAlignment(.center)
                Text(body_).fbText(.small, color: theme.textMuted).multilineTextAlignment(.center)
                SmoothProgress(nil)
            }
            .frame(maxWidth: .infinity)
            .padding(Space.xxl)
        }
    }
}

/// A step in a flow: an icon tile, a heading, a sentence and the controls for that step.
struct StepPanel<Content: View>: View {
    @Environment(\.theme) private var theme
    let icon: Icon?
    let customIcon: ApprovedCustomIcon?
    let title: String
    let body_: String
    @ViewBuilder let content: () -> Content

    init(_ icon: Icon? = nil, customIcon: ApprovedCustomIcon? = nil, title: String, body: String, @ViewBuilder content: @escaping () -> Content) {
        self.icon = icon; self.customIcon = customIcon; self.title = title; body_ = body; self.content = content
    }

    var body: some View {
        Panel {
            VStack(alignment: .leading, spacing: Space.lg) {
                Group {
                    if let icon { FbIcon(icon, size: 26, tint: theme.accentText) }
                    else if let customIcon { CustomIcon(icon: customIcon, size: 26).foregroundStyle(theme.accentText) }
                }
                .frame(width: 56, height: 56)
                .background(theme.accentSurface, in: Shapes.card)
                .overlay(Shapes.card.strokeBorder(theme.choiceBorder.opacity(0.5), lineWidth: 1))
                Text(title).fbText(.headline).accessibilityAddTraits(.isHeader)
                Text(body_).fbText(.small, color: theme.textMuted).fixedSize(horizontal: false, vertical: true)
                content()
            }
            .padding(Space.xl)
        }
    }
}

/// A loading indicator centred in a list.
struct CenteredSpinner: View {
    var body: some View { Spinner().frame(maxWidth: .infinity).padding(Space.xxxl) }
}
