import SwiftUI
import FilebeamDomain

private let descriptionMaximum = 2000

/// Reports a transfer to the instance's moderators. Only its identifier and this text are sent.
struct ReportSheet: View {
    @Environment(\.dismiss) private var dismiss
    @Environment(AppModel.self) private var model
    @State private var category = ReportCategory.spam
    @State private var description = ""
    @State private var email = ""

    var body: some View {
        let receive = model.receive
        FbSheet(title: "Report this transfer", subtitle: receive.reportSent ? nil : "Tell the instance's moderators what is wrong. File contents stay encrypted.") {
            if receive.reportSent {
                EmptyState(.check, "Thank you", body: "The report was sent to the moderators.", tone: .success)
                FbButton("Done") { dismiss() }
            } else {
                FlowLayout(spacing: Space.sm) {
                    ForEach(ReportCategory.allCases, id: \.self) { item in
                        FilterChip(item.label, selected: item == category) { category = item }
                    }
                }
                FbTextField($description, label: "What is wrong?", placeholder: "Describe the problem", supporting: "\(description.count) / \(descriptionMaximum)", multiline: true, minLines: 4, autocapitalization: .sentences)
                    .onChange(of: description) { _, value in if value.count > descriptionMaximum { description = String(value.prefix(descriptionMaximum)) } }
                FbTextField($email, label: "Email (optional)", placeholder: "So moderators can follow up", keyboard: .emailAddress, contentType: .emailAddress)
                if let error = receive.reportError { Notice(error.isEmpty ? "The report could not be sent." : error, tone: .danger) }
                FbButton("Send report", enabled: !description.isBlank, loading: receive.reportBusy) {
                    Task { await receive.report(category: category, description: description.trimmingCharacters(in: .whitespacesAndNewlines), email: email.trimmingCharacters(in: .whitespaces)) }
                }
            }
        }
    }
}

extension ReportCategory {
    var label: String {
        switch self { case .spam: "Spam"; case .malware: "Malware"; case .illegalContent: "Illegal content"; case .privacy: "Privacy"; case .copyright: "Copyright"; case .other: "Other" }
    }
}

/// Lays children out in rows, wrapping like text.
struct FlowLayout: Layout {
    var spacing: CGFloat = 8

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let width = proposal.width ?? .infinity
        var x: CGFloat = 0, y: CGFloat = 0, rowHeight: CGFloat = 0
        for subview in subviews {
            let size = subview.sizeThatFits(.unspecified)
            if x + size.width > width, x > 0 { x = 0; y += rowHeight + spacing; rowHeight = 0 }
            x += size.width + spacing
            rowHeight = max(rowHeight, size.height)
        }
        return CGSize(width: width == .infinity ? x : width, height: y + rowHeight)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        var x = bounds.minX, y = bounds.minY, rowHeight: CGFloat = 0
        for subview in subviews {
            let size = subview.sizeThatFits(.unspecified)
            if x + size.width > bounds.maxX, x > bounds.minX { x = bounds.minX; y += rowHeight + spacing; rowHeight = 0 }
            subview.place(at: CGPoint(x: x, y: y), proposal: ProposedViewSize(size))
            x += size.width + spacing
            rowHeight = max(rowHeight, size.height)
        }
    }
}
