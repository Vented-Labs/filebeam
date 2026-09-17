import SwiftUI

extension Color { static let filebeamViolet = Color("FilebeamViolet", bundle: .main) }

struct PrimaryActionButton: View {
    let title: String
    var isLoading = false
    var disabled = false
    let action: () -> Void
    var body: some View {
        Button(action: action) {
            HStack { if isLoading { ProgressView().tint(.white) }; Text(title).frame(maxWidth: .infinity) }
        }
        .buttonStyle(.borderedProminent).tint(.filebeamViolet).controlSize(.large)
        .disabled(disabled || isLoading).accessibilityHint(disabled ? "Complete the required fields first." : "")
    }
}

struct InlineNotice: View {
    let text: String
    var body: some View { Label(text, systemImage: "exclamationmark.circle").font(.footnote).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true) }
}

struct EmptyStateView: View {
    let title: String
    let detail: String
    var body: some View { ContentUnavailableView(title, systemImage: "tray", description: Text(detail)) }
}

extension UInt64 {
    var filebeamBytes: String { ByteCountFormatter.string(fromByteCount: Int64(clamping: self), countStyle: .file) }
}
