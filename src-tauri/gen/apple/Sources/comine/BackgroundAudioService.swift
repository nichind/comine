import AVFoundation
import UIKit

/// Keeps the app alive in the background by playing a silent audio loop
/// and cycling background tasks. Mirrors the iTorrent approach.
class BackgroundAudioService: NSObject {
    static let shared = BackgroundAudioService()

    private var audioPlayer: AVAudioPlayer?
    private var backgroundTask: UIBackgroundTaskIdentifier = .invalid
    private(set) var isRunning = false

    private override init() {
        super.init()
    }

    func start() {
        guard !isRunning else { return }
        isRunning = true

        configureAudioSession()
        startSilentAudio()
        beginBackgroundTask()

        NSLog("[BackgroundAudio] Started background keep-alive")
    }

    func stop() {
        guard isRunning else { return }
        isRunning = false

        audioPlayer?.stop()
        audioPlayer = nil
        endBackgroundTask()

        // Deactivate audio session so other apps can use audio normally
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)

        NSLog("[BackgroundAudio] Stopped background keep-alive")
    }

    private func configureAudioSession() {
        let session = AVAudioSession.sharedInstance()
        do {
            // .playback category + .mixWithOthers so we don't interrupt other audio (e.g. VLC)
            try session.setCategory(.playback, options: .mixWithOthers)
            try session.setActive(true)
        } catch {
            NSLog("[BackgroundAudio] Failed to configure audio session: \(error)")
        }
    }

    private func startSilentAudio() {
        guard let url = Bundle.main.url(forResource: "silence", withExtension: "m4a") else {
            NSLog("[BackgroundAudio] silence.m4a not found in bundle")
            return
        }

        do {
            audioPlayer = try AVAudioPlayer(contentsOf: url)
            audioPlayer?.numberOfLoops = -1  // Loop forever
            audioPlayer?.volume = 0.01       // Near-silent
            audioPlayer?.prepareToPlay()
            audioPlayer?.play()
        } catch {
            NSLog("[BackgroundAudio] Failed to start audio player: \(error)")
        }
    }

    /// Cycle UIBackgroundTaskIdentifier to prevent the OS from suspending us.
    /// Each task gets ~30 seconds; we renew before expiry.
    private func beginBackgroundTask() {
        endBackgroundTask()

        backgroundTask = UIApplication.shared.beginBackgroundTask { [weak self] in
            // Expiration handler — renew the task
            self?.beginBackgroundTask()
        }

        // Proactively renew every 10 seconds (well before the ~30s expiry)
        DispatchQueue.main.asyncAfter(deadline: .now() + 10) { [weak self] in
            guard let self, self.isRunning else { return }
            self.beginBackgroundTask()
        }
    }

    private func endBackgroundTask() {
        if backgroundTask != .invalid {
            UIApplication.shared.endBackgroundTask(backgroundTask)
            backgroundTask = .invalid
        }
    }
}

// MARK: - FFI exports for Rust

@_cdecl("start_background_audio")
func startBackgroundAudio() {
    DispatchQueue.main.async {
        BackgroundAudioService.shared.start()
    }
}

@_cdecl("stop_background_audio")
func stopBackgroundAudio() {
    DispatchQueue.main.async {
        BackgroundAudioService.shared.stop()
    }
}

@_cdecl("is_background_audio_running")
func isBackgroundAudioRunning() -> Bool {
    return BackgroundAudioService.shared.isRunning
}
