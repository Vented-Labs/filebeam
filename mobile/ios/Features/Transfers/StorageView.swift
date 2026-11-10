import SwiftUI

/// How much private space transfers occupy on this device, split by what the files are for.
struct StorageView: View {
    @Environment(AppModel.self) private var model
    @Environment(\.theme) private var theme
    @State private var usage: StorageUsage?

    var body: some View {
        SubScreen("Storage usage") {
            if let usage {
                let buckets: [(String, StorageUsageBucket, Color)] = [("Verified downloads", usage.verified, theme.progressFill), ("Upload snapshots", usage.snapshots, theme.info.ink), ("Recovery data", usage.recovery, theme.warning.ink)]
                Panel {
                    VStack(alignment: .leading, spacing: Space.lg) {
                        Text(formatBytes(usage.bytes)).fbText(.display)
                        Text("\(formatBytes(usage.free)) free on this device").fbText(.small, color: theme.textMuted)
                        GeometryReader { proxy in
                            let total = Double(max(usage.bytes, 1))
                            HStack(spacing: 0) {
                                ForEach(Array(buckets.enumerated()), id: \.offset) { _, bucket in
                                    Rectangle().fill(bucket.2).frame(width: proxy.size.width * Double(bucket.1.bytes) / total)
                                }
                                Spacer(minLength: 0)
                            }
                            .background(theme.progressTrack)
                            .clipShape(Capsule())
                        }
                        .frame(height: 10)
                    }
                    .padding(Space.lg)
                }
                .staggered(0)
                Panel {
                    ForEach(Array(buckets.enumerated()), id: \.offset) { index, bucket in
                        if index > 0 { Hairline() }
                        HStack(spacing: Space.md) {
                            Circle().fill(bucket.2).frame(width: 12, height: 12)
                            VStack(alignment: .leading, spacing: 2) {
                                Text(bucket.0).fbText(.bodyStrong)
                                Text(bucket.1.files == 1 ? "1 file" : "\(bucket.1.files) files").fbText(.small, color: theme.textMuted)
                            }
                            .frame(maxWidth: .infinity, alignment: .leading)
                            Text(formatBytes(bucket.1.bytes)).fbText(.numeric)
                        }
                        .padding(Space.lg)
                    }
                }
                .staggered(1)
                Text("Files stay private to Filebeam until you save them elsewhere. Remove a transfer from Transfers to free its space.").fbText(.small, color: theme.textMuted)
            } else {
                CenteredSpinner()
            }
        }
        .task {
            guard let paths = model.paths else { usage = StorageUsage(); return }
            usage = await StorageUsage.scan(paths: paths)
        }
    }
}
