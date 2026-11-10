import SwiftUI
import FilebeamDomain

private enum Info: Identifiable { case about, privacy, help, cli, licenses; var id: Self { self } }
private let cliUnix = "curl -fsSL 'https://releases.filebeam.io/cli/install.sh' | sh"
private let cliWindows = "irm 'https://releases.filebeam.io/cli/install.ps1' | iex"

/// What Filebeam is, the connected instance, where to get help, and the open-source notices.
struct AboutView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.openURL) private var openURL
    @State private var open: Info?

    private var appVersion: String { Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "1.0.0" }

    var body: some View {
        let branding = model.branding
        SubScreen("About") {
            Panel {
                HStack(spacing: Space.lg) {
                    FilebeamMark(height: 52)
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Filebeam").fbText(.headline)
                        Text("End-to-end encrypted files and notes.").fbText(.small, color: theme.textMuted)
                        Text("App version \(appVersion)").fbText(.caption, color: theme.textSubtle)
                    }
                }
                .padding(Space.lg)
            }
            .staggered(0)
            SectionLabel("Learn")
            Panel {
                LinkRow(.bolt, "What Filebeam can do") { model.push(.tour) }
                if branding.changelogEnabled {
                    Hairline()
                    LinkRow(.updates, "Changelog", detail: branding.version.map { "v\($0)" }) { model.push(.changelog) }
                }
                Hairline()
                LinkRow(.info, "About") { open = .about }
                Hairline()
                LinkRow(.shield, "Privacy") { open = .privacy }
                Hairline()
                LinkRow(.help, "Help") { open = .help }
                Hairline()
                LinkRow(.terminal, "Get the Beam CLI") { open = .cli }
            }
            .staggered(1)
            SectionLabel("Connected instance")
            Panel {
                VStack(alignment: .leading, spacing: Space.xs) {
                    Text(branding.name).fbText(.bodyStrong)
                    Text(model.instance.origin).fbText(.codeSmall, color: theme.textMuted)
                    if let version = branding.version { Text("Server version \(version)").fbText(.caption, color: theme.textSubtle) }
                    if let copyright = branding.copyright { Text("© \(copyright)").fbText(.caption, color: theme.textSubtle) }
                }
                .padding(Space.lg)
                if let github = branding.githubURL, let url = URL(string: github) {
                    Hairline()
                    LinkRow(.code, "Source code") { openURL(url) }
                }
                ForEach(branding.communityLinks, id: \.url) { link in
                    if let url = URL(string: link.url) {
                        Hairline()
                        LinkRow(.globe, link.name) { openURL(url) }
                    }
                }
            }
            .staggered(2)
            Panel { LinkRow(.book, "Open-source licenses") { open = .licenses } }.staggered(3)
        }
        .sheet(item: $open) { info in
            switch info {
            case .about: InfoSheet(title: "About", body: "Share end-to-end encrypted files and notes with temporary links.")
            case .privacy: InfoSheet(title: "Privacy", body: "Files are encrypted on this device. The decryption key remains with the people you choose to share it with.")
            case .help: InfoSheet(title: "Help", body: "Choose files or write a note, then share the generated link with its intended recipient.")
            case .cli: CliSheet()
            case .licenses: LicensesSheet()
            }
        }
    }
}

private struct InfoSheet: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(\.theme) private var theme
    let title: String
    let body_: String
    init(title: String, body: String) { self.title = title; body_ = body }
    var body: some View {
        FbSheet(title: title) {
            Text(body_).fbText(.body, color: theme.textMuted)
            FbButton("Done") { dismiss() }
        }
    }
}

private struct CliSheet: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var platform = "unix"
    var body: some View {
        let command = platform == "unix" ? cliUnix : cliWindows
        FbSheet(title: "Get the Beam CLI", subtitle: "Send and receive from a terminal on Linux, macOS or Windows.") {
            SegmentedRail(segments: [Segment(value: "unix", label: "Linux and macOS"), Segment(value: "windows", label: "Windows")], selection: $platform)
            CopyField(icon: .terminal, value: command, label: "Copy", primary: true) {
                Clipboard.copy(command)
                model.toast("Command copied", tone: .success)
            }
            Text("After installing:").fbText(.small, color: theme.textMuted)
            CodeEditor("beam up ./file.zip\nbeam down <url or id>\nbeam", language: "plain", minHeight: 96, maxHeight: 160)
        }
    }
}

private struct LicensesSheet: View {
    @Environment(\.theme) private var theme
    private var notices: [(String, String)] {
        ["LICENSE"].compactMap { name in
            guard let url = Bundle.main.url(forResource: name, withExtension: "txt"), let text = try? String(contentsOf: url, encoding: .utf8) else { return nil }
            return (name, text)
        }
    }
    var body: some View {
        FbSheet(title: "Open-source licenses", fitted: false) {
            ForEach(notices, id: \.0) { name, text in
                Text(name).fbText(.bodyStrong)
                Text(text).fbText(.codeSmall, color: theme.textMuted).textSelection(.enabled)
            }
            Text("Icons are SF Symbols provided by Apple for use in apps on Apple platforms. The transport icons are original Filebeam artwork.").fbText(.small, color: theme.textMuted)
        }
    }
}
