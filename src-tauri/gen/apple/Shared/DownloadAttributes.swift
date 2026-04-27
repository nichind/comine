import ActivityKit
import Foundation

/// Defines the Live Activity / Dynamic Island content for an active download.
struct DownloadAttributes: ActivityAttributes {
    /// Fixed context that doesn't change after the activity starts.
    public struct ContentState: Codable, Hashable {
        var progress: Double       // 0.0 – 1.0
        var speedBps: Int64        // bytes per second
        var downloadedBytes: Int64
        var totalBytes: Int64
        var eta: Int               // seconds remaining
        var isFinished: Bool
    }

    /// Static data set when the activity is created.
    var title: String
    var jobId: String
}
