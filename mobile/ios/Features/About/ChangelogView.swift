import SwiftUI
import FilebeamDomain

/// Release notes mirrored by the instance: searchable, paged, with pre-releases on request.
struct ChangelogView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var query = ""
    @State private var prereleases = false
    @State private var page = 1
    @State private var listing: ChangelogListing?
    @State private var error: String?

    var body: some View {
        SubScreen("Changelog") {
            FbTextField($query, placeholder: "Search release notes", icon: .search, autocapitalization: .sentences, submitLabel: .search)
                .onChange(of: query) { _, value in query = String(value.prefix(120)); page = 1 }
            if listing?.hasPrereleases == true {
                Panel { SwitchRow("Include pre-releases", isOn: Binding(get: { prereleases }, set: { prereleases = $0; page = 1 })).padding(.horizontal, Space.lg) }
            }
            if let versions = listing?.versions, versions.count > 1, query.isBlank {
                VStack(alignment: .leading, spacing: Space.sm) {
                    Text("Jump to version").fbText(.eyebrow, color: theme.textMuted)
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(spacing: Space.sm) {
                            ForEach(versions.prefix(40)) { version in FilterChip(version.tag, selected: false) { model.push(.release(version.tag)) } }
                        }
                    }
                }
            }
            if listing?.stale == true { Notice("These notes may be out of date. The instance could not refresh them recently.", tone: .warning) }
            if let listing {
                if error != nil || listing.archiveState == "unavailable" {
                    EmptyState(.alert, "Release notes are unavailable", body: error ?? "This instance has not mirrored its release notes yet.", tone: .warning)
                } else if listing.releases.isEmpty {
                    EmptyState(.search, query.isBlank ? "No releases yet" : "No releases match that search", tone: .neutral)
                } else {
                    ForEach(listing.releases) { release in
                        Button { model.push(.release(release.tag)) } label: {
                            Panel(border: release.featured ? theme.choiceBorder : nil) {
                                VStack(alignment: .leading, spacing: Space.sm) {
                                    HStack(spacing: Space.sm) {
                                        Text(release.tag).fbText(.codeSmall, color: theme.accentText)
                                        if release.latest { StatusPill("Latest", tone: .success) }
                                        if release.prerelease { StatusPill("Pre-release", tone: .warning) }
                                    }
                                    Text(release.title).fbText(release.featured ? .headline : .title)
                                    if !release.description.isBlank { Text(release.description).fbText(.small, color: theme.textMuted).lineLimit(3) }
                                    if let published = release.publishedAt { Text(formatAbsolute(published)).fbText(.caption, color: theme.textSubtle) }
                                }
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .padding(Space.lg)
                            }
                        }
                        .buttonStyle(.pressScale(0.985))
                    }
                    if listing.lastPage > 1 {
                        HStack(spacing: Space.md) {
                            FbButton("Newer", kind: .secondary, enabled: page > 1) { page -= 1 }
                            Text("\(page) / \(listing.lastPage)").fbText(.numeric, color: theme.textMuted)
                            FbButton("Older", kind: .secondary, enabled: page < listing.lastPage) { page += 1 }
                        }
                    }
                }
            } else if let error {
                EmptyState(.alert, "Release notes are unavailable", body: error, tone: .warning)
            } else { CenteredSpinner() }
        }
        .task(id: "\(query)|\(prereleases)|\(page)") {
            // Typing settles before a search goes out, as on the web.
            if !query.isEmpty { try? await Task.sleep(for: .milliseconds(250)) }
            guard !Task.isCancelled else { return }
            error = nil
            do { listing = try await model.service.changelog(instance: model.instance, page: page, term: query.nilIfEmpty, prereleases: prereleases) }
            catch { self.error = readableMessage(error); listing = nil }
        }
    }
}

/// One release, rendered natively from the instance's sanitised node tree.
struct ReleaseView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @Environment(\.openURL) private var openURL
    let tag: String
    @State private var detail: ReleaseDetail?
    @State private var failed = false

    var body: some View {
        SubScreen(tag, actions: {
            if detail != nil {
                IconAction(.link, "Copy link") {
                    Clipboard.copy("\(model.instance.origin)/changelog/\(tag)")
                    model.toast("Copied", tone: .success)
                }
            }
        }) {
            if failed {
                EmptyState(.alert, "Release notes are unavailable", tone: .warning)
            } else if let detail {
                VStack(alignment: .leading, spacing: Space.sm) {
                    HStack(spacing: Space.sm) {
                        if detail.summary.latest { StatusPill("Latest", tone: .success) }
                        if detail.summary.prerelease { StatusPill("Pre-release", tone: .warning) }
                    }
                    Text(detail.summary.title).fbText(.display).accessibilityAddTraits(.isHeader)
                    let meta = [detail.summary.publishedAt.map(formatAbsolute), detail.author.map { "@\($0)" }].compactMap { $0 }
                    if !meta.isEmpty { Text(meta.joined(separator: " · ")).fbText(.small, color: theme.textMuted) }
                }
                let headings = detail.blocks.flatMap { ReleaseNodeRenderer.headings($0) }
                if headings.count > 2 {
                    Panel {
                        VStack(alignment: .leading, spacing: Space.xs) {
                            Text("In this release").fbText(.eyebrow, color: theme.textMuted)
                            ForEach(headings, id: \.self) { Text($0).fbText(.small, color: theme.accentText).lineLimit(1) }
                        }
                        .padding(Space.lg)
                    }
                }
                ForEach(Array(detail.blocks.enumerated()), id: \.offset) { _, nodes in
                    ReleaseNodeRenderer(nodes: nodes)
                }
                Hairline()
                HStack(spacing: Space.md) {
                    FbButton(detail.newer?.tag ?? "Newer", kind: .secondary, icon: .arrowLeft, enabled: detail.newer != nil) { if let newer = detail.newer { model.replace(.release(newer.tag)) } }
                    FbButton(detail.older?.tag ?? "Older", kind: .secondary, trailingIcon: .arrowRight, enabled: detail.older != nil) { if let older = detail.older { model.replace(.release(older.tag)) } }
                }
                if let html = detail.htmlURL, let url = URL(string: html) {
                    FbButton("View on GitHub", kind: .quiet, trailingIcon: .arrowUpRight) { openURL(url) }
                }
            } else { CenteredSpinner() }
        }
        .task(id: tag) {
            failed = false
            do { detail = try await model.service.changelogRelease(instance: model.instance, tag: tag) } catch { failed = true }
        }
    }
}

/// Renders a list of nodes: block elements stack, runs of inline content flow as one paragraph.
struct ReleaseNodeRenderer: View {
    @Environment(\.theme) private var theme
    let nodes: [ReleaseNode]

    private static let blockTags: Set<String> = ["section", "div", "p", "blockquote", "pre", "hr", "h1", "h2", "h3", "h4", "h5", "h6", "ul", "ol", "table", "img"]

    static func headings(_ nodes: [ReleaseNode]) -> [String] {
        nodes.flatMap { node -> [String] in
            guard case let .element(tag, _, children) = node else { return [] }
            if tag == "h2" || tag == "h3" { return [children.map(\.plainText).joined().trimmingCharacters(in: .whitespacesAndNewlines)] }
            return headings(children)
        }
    }

    private enum Group { case inline([ReleaseNode]); case block(ReleaseNode) }

    private var groups: [Group] {
        var result: [Group] = []
        var inline: [ReleaseNode] = []
        for node in nodes {
            if case let .element(tag, _, _) = node, Self.blockTags.contains(tag) {
                if !inline.isEmpty { result.append(.inline(inline)); inline = [] }
                result.append(.block(node))
            } else if case let .text(text) = node, text.isBlank, inline.isEmpty {
                continue
            } else { inline.append(node) }
        }
        if !inline.isEmpty { result.append(.inline(inline)) }
        return result
    }

    var body: some View {
        VStack(alignment: .leading, spacing: Space.md) {
            ForEach(Array(groups.enumerated()), id: \.offset) { _, group in
                switch group {
                case let .inline(nodes): paragraph(nodes, style: .body)
                case let .block(node): block(node)
                }
            }
        }
    }

    @ViewBuilder private func paragraph(_ nodes: [ReleaseNode], style: FbTextStyle) -> some View {
        let text = inlineText(nodes, style: style)
        if !text.characters.isEmpty { Text(text).fbText(style).tint(theme.accentText) }
    }

    private func inlineText(_ nodes: [ReleaseNode], style: FbTextStyle, inherited: AttributeContainer = AttributeContainer()) -> AttributedString {
        var result = AttributedString()
        for node in nodes {
            switch node {
            case let .text(text):
                var piece = AttributedString(text)
                piece.mergeAttributes(inherited)
                result += piece
            case let .element(tag, attributes, children):
                var container = inherited
                switch tag {
                case "br": result += AttributedString("\n")
                case "strong": container.font = style.font.weight(.semibold); result += inlineText(children, style: style, inherited: container)
                case "em": container.font = style.font.italic(); result += inlineText(children, style: style, inherited: container)
                case "del": container.strikethroughStyle = .single; result += inlineText(children, style: style, inherited: container)
                case "code": container.font = .system(.footnote, design: .monospaced); container.backgroundColor = theme.hover; result += inlineText(children, style: style, inherited: container)
                case "a":
                    // Only web links open; anything else renders as plain text.
                    if let href = attributes["href"], href.hasPrefix("https://") || href.hasPrefix("http://"), let url = URL(string: href) {
                        container.link = url
                        container.underlineStyle = .single
                        container.foregroundColor = theme.accentText
                    }
                    result += inlineText(children, style: style, inherited: container)
                case "input": result += AttributedString(attributes["checked"] != nil ? "☑ " : "☐ ")
                case "img": result += AttributedString(attributes["alt"] ?? "")
                default: result += inlineText(children, style: style, inherited: container)
                }
            }
        }
        return result
    }

    @ViewBuilder private func block(_ node: ReleaseNode) -> some View {
        if case let .element(tag, attributes, children) = node {
            switch tag {
            case "h1", "h2": paragraph(children, style: .headline).padding(.top, Space.md).accessibilityAddTraits(.isHeader)
            case "h3": paragraph(children, style: .title).padding(.top, Space.sm).accessibilityAddTraits(.isHeader)
            case "h4", "h5", "h6": paragraph(children, style: .subtitle).accessibilityAddTraits(.isHeader)
            case "p": paragraph(children, style: .body)
            case "hr": Hairline()
            case "pre":
                ScrollView(.horizontal, showsIndicators: false) {
                    Text(children.map(\.plainText).joined().trimmingCharacters(in: .newlines)).fbText(.codeSmall, color: theme.editor.text).padding(Space.md)
                }
                .background(theme.editor.background, in: Shapes.control)
                .overlay(Shapes.control.strokeBorder(theme.editor.border, lineWidth: 1))
            case "blockquote":
                HStack(alignment: .top, spacing: Space.md) {
                    Capsule().fill(theme.choiceBorder).frame(width: 3)
                    ReleaseNodeRenderer(nodes: children)
                }
                .fixedSize(horizontal: false, vertical: true)
            case "ul", "ol":
                let start = Int(attributes["start"] ?? "") ?? 1
                let items = children.compactMap { child -> [ReleaseNode]? in if case let .element(tag, _, grandchildren) = child, tag == "li" { grandchildren } else { nil } }
                VStack(alignment: .leading, spacing: Space.sm) {
                    ForEach(Array(items.enumerated()), id: \.offset) { index, item in
                        HStack(alignment: .top, spacing: Space.sm) {
                            Text(tag == "ol" ? "\(start + index)." : "•").fbText(.body, color: theme.accentText)
                            ReleaseNodeRenderer(nodes: item)
                        }
                    }
                }
            case "table":
                let rows = Self.rows(children)
                VStack(spacing: 0) {
                    ForEach(Array(rows.enumerated()), id: \.offset) { index, row in
                        if index > 0 { Hairline() }
                        HStack(alignment: .top, spacing: Space.md) {
                            ForEach(Array(row.enumerated()), id: \.offset) { _, cell in Text(cell).fbText(index == 0 ? .smallStrong : .small).frame(maxWidth: .infinity, alignment: .leading) }
                        }
                        .padding(Space.md)
                        .background(index == 0 ? theme.sunken : Color.clear)
                    }
                }
                .clipShape(Shapes.control)
                .overlay(Shapes.control.strokeBorder(theme.border, lineWidth: 1))
            case "img": if let alt = attributes["alt"], !alt.isBlank { Text(alt).fbText(.small, color: theme.textMuted) }
            default: ReleaseNodeRenderer(nodes: children)
            }
        }
    }

    private static func rows(_ nodes: [ReleaseNode]) -> [[String]] {
        nodes.flatMap { node -> [[String]] in
            guard case let .element(tag, _, children) = node else { return [] }
            if tag == "tr" {
                return [children.compactMap { cell -> String? in if case let .element(_, _, grandchildren) = cell { grandchildren.map(\.plainText).joined().trimmingCharacters(in: .whitespacesAndNewlines) } else { nil } }]
            }
            return rows(children)
        }
    }
}
