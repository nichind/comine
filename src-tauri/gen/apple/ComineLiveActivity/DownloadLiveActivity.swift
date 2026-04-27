import ActivityKit
import SwiftUI
import WidgetKit

@available(iOS 16.1, *)
struct DownloadLiveActivity: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: DownloadAttributes.self) { context in
            // Lock Screen / Banner view
            lockScreenView(context: context)
        } dynamicIsland: { context in
            DynamicIsland {
                // Expanded view (long-press Dynamic Island)
                DynamicIslandExpandedRegion(.leading) {
                    Label(context.attributes.title, systemImage: "arrow.down.circle.fill")
                        .font(.caption)
                        .lineLimit(1)
                }
                DynamicIslandExpandedRegion(.trailing) {
                    Text(formatSpeed(context.state.speedBps))
                        .font(.caption.monospacedDigit())
                        .foregroundColor(.secondary)
                }
                DynamicIslandExpandedRegion(.bottom) {
                    VStack(spacing: 4) {
                        ProgressView(value: context.state.progress)
                            .tint(.blue)
                        HStack {
                            Text("\(Int(context.state.progress * 100))%")
                                .font(.caption2.monospacedDigit())
                            Spacer()
                            if context.state.eta > 0 && !context.state.isFinished {
                                Text(formatETA(context.state.eta))
                                    .font(.caption2.monospacedDigit())
                                    .foregroundColor(.secondary)
                            }
                        }
                    }
                }
            } compactLeading: {
                // Compact leading (left pill)
                Image(systemName: "arrow.down.circle.fill")
                    .foregroundColor(.blue)
            } compactTrailing: {
                // Compact trailing (right pill)
                Text("\(Int(context.state.progress * 100))%")
                    .font(.caption2.monospacedDigit())
            } minimal: {
                // Minimal (single circle when another activity is present)
                ProgressView(value: context.state.progress)
                    .progressViewStyle(.circular)
                    .tint(.blue)
            }
        }
    }

    @ViewBuilder
    func lockScreenView(context: ActivityViewContext<DownloadAttributes>) -> some View {
        VStack(spacing: 8) {
            HStack {
                Image(systemName: "arrow.down.circle.fill")
                    .foregroundColor(.blue)
                Text(context.attributes.title)
                    .font(.subheadline.weight(.medium))
                    .lineLimit(1)
                Spacer()
                if context.state.isFinished {
                    Image(systemName: "checkmark.circle.fill")
                        .foregroundColor(.green)
                } else {
                    Text(formatSpeed(context.state.speedBps))
                        .font(.caption.monospacedDigit())
                        .foregroundColor(.secondary)
                }
            }
            ProgressView(value: context.state.progress)
                .tint(context.state.isFinished ? .green : .blue)
            HStack {
                Text(formatBytes(context.state.downloadedBytes))
                    .font(.caption2.monospacedDigit())
                if context.state.totalBytes > 0 {
                    Text("/ \(formatBytes(context.state.totalBytes))")
                        .font(.caption2.monospacedDigit())
                        .foregroundColor(.secondary)
                }
                Spacer()
                if context.state.eta > 0 && !context.state.isFinished {
                    Text(formatETA(context.state.eta))
                        .font(.caption2.monospacedDigit())
                        .foregroundColor(.secondary)
                }
            }
        }
        .padding()
    }
}

// MARK: - Formatting Helpers

private func formatSpeed(_ bps: Int64) -> String {
    let units = ["B/s", "KB/s", "MB/s", "GB/s"]
    var value = Double(bps)
    var unitIdx = 0
    while value >= 1024 && unitIdx < units.count - 1 {
        value /= 1024
        unitIdx += 1
    }
    return String(format: "%.1f %@", value, units[unitIdx])
}

private func formatBytes(_ bytes: Int64) -> String {
    let units = ["B", "KB", "MB", "GB"]
    var value = Double(bytes)
    var unitIdx = 0
    while value >= 1024 && unitIdx < units.count - 1 {
        value /= 1024
        unitIdx += 1
    }
    return unitIdx == 0
        ? String(format: "%.0f %@", value, units[unitIdx])
        : String(format: "%.1f %@", value, units[unitIdx])
}

private func formatETA(_ seconds: Int) -> String {
    if seconds < 60 { return "\(seconds)s" }
    if seconds < 3600 { return "\(seconds / 60)m \(seconds % 60)s" }
    return "\(seconds / 3600)h \(seconds % 3600 / 60)m"
}

// MARK: - Widget Bundle

@available(iOS 16.1, *)
@main
struct ComineLiveActivityBundle: WidgetBundle {
    var body: some Widget {
        DownloadLiveActivity()
    }
}
