import Foundation

#if canImport(ActivityKit)
import ActivityKit
#endif

/// Manages Live Activities for download progress (Dynamic Island + Lock Screen).
/// Requires iOS 16.1+. Gracefully no-ops on older versions.
class LiveActivityManager {
    static let shared = LiveActivityManager()

    #if canImport(ActivityKit)
    @available(iOS 16.2, *)
    private var currentActivity: Activity<DownloadAttributes>? {
        get { _currentActivity as? Activity<DownloadAttributes> }
        set { _currentActivity = newValue }
    }
    #endif

    private var _currentActivity: Any?
    private var activeJobId: String?

    private init() {}

    func start(jobId: String, title: String) {
        #if canImport(ActivityKit)
        guard #available(iOS 16.2, *) else { return }

        // End any existing activity first
        stop()

        guard ActivityAuthorizationInfo().areActivitiesEnabled else {
            NSLog("[LiveActivity] Activities not authorized")
            return
        }

        let attributes = DownloadAttributes(title: title, jobId: jobId)
        let state = DownloadAttributes.ContentState(
            progress: 0,
            speedBps: 0,
            downloadedBytes: 0,
            totalBytes: 0,
            eta: 0,
            isFinished: false
        )

        do {
            let activity = try Activity.request(
                attributes: attributes,
                content: .init(state: state, staleDate: nil),
                pushType: nil
            )
            currentActivity = activity
            activeJobId = jobId
            NSLog("[LiveActivity] Started for job \(jobId)")
        } catch {
            NSLog("[LiveActivity] Failed to start: \(error)")
        }
        #endif
    }

    func update(
        jobId: String,
        progress: Double,
        speedBps: Int64,
        downloadedBytes: Int64,
        totalBytes: Int64,
        eta: Int
    ) {
        #if canImport(ActivityKit)
        guard #available(iOS 16.2, *) else { return }
        guard activeJobId == jobId, let activity = currentActivity else { return }

        let state = DownloadAttributes.ContentState(
            progress: min(max(progress, 0), 1),
            speedBps: speedBps,
            downloadedBytes: downloadedBytes,
            totalBytes: totalBytes,
            eta: eta,
            isFinished: false
        )

        Task {
            await activity.update(.init(state: state, staleDate: nil))
        }
        #endif
    }

    func finish(jobId: String) {
        #if canImport(ActivityKit)
        guard #available(iOS 16.2, *) else { return }
        guard activeJobId == jobId, let activity = currentActivity else { return }

        let state = DownloadAttributes.ContentState(
            progress: 1.0,
            speedBps: 0,
            downloadedBytes: 0,
            totalBytes: 0,
            eta: 0,
            isFinished: true
        )

        Task {
            let content = ActivityContent(state: state, staleDate: nil)
            await activity.end(content, dismissalPolicy: ActivityUIDismissalPolicy.after(Date.now.addingTimeInterval(5)))
            NSLog("[LiveActivity] Finished for job \(jobId)")
        }

        activeJobId = nil
        _currentActivity = nil
        #endif
    }

    func stop() {
        #if canImport(ActivityKit)
        guard #available(iOS 16.2, *) else { return }

        if let activity = currentActivity {
            Task {
                let policy = ActivityUIDismissalPolicy.immediate
                await activity.end(nil as ActivityContent<DownloadAttributes.ContentState>?, dismissalPolicy: policy)
            }
        }
        activeJobId = nil
        _currentActivity = nil
        #endif
    }
}

// MARK: - FFI exports for Rust

@_cdecl("live_activity_start")
func liveActivityStart(_ jobIdPtr: UnsafePointer<CChar>, _ titlePtr: UnsafePointer<CChar>) {
    let jobId = String(cString: jobIdPtr)
    let title = String(cString: titlePtr)
    DispatchQueue.main.async {
        LiveActivityManager.shared.start(jobId: jobId, title: title)
    }
}

@_cdecl("live_activity_update")
func liveActivityUpdate(
    _ jobIdPtr: UnsafePointer<CChar>,
    _ progress: Double,
    _ speedBps: Int64,
    _ downloadedBytes: Int64,
    _ totalBytes: Int64,
    _ eta: Int32
) {
    let jobId = String(cString: jobIdPtr)
    DispatchQueue.main.async {
        LiveActivityManager.shared.update(
            jobId: jobId,
            progress: progress,
            speedBps: speedBps,
            downloadedBytes: downloadedBytes,
            totalBytes: totalBytes,
            eta: Int(eta)
        )
    }
}

@_cdecl("live_activity_finish")
func liveActivityFinish(_ jobIdPtr: UnsafePointer<CChar>) {
    let jobId = String(cString: jobIdPtr)
    DispatchQueue.main.async {
        LiveActivityManager.shared.finish(jobId: jobId)
    }
}

@_cdecl("live_activity_stop")
func liveActivityStop() {
    DispatchQueue.main.async {
        LiveActivityManager.shared.stop()
    }
}
