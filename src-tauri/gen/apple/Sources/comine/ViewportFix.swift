import UIKit
import WebKit

private func applyViewportFix() {
    guard let windowScene = UIApplication.shared.connectedScenes.first as? UIWindowScene,
          let window = windowScene.windows.first(where: { $0.isKeyWindow }),
          let rootVC = window.rootViewController else {
        NSLog("[ViewportFix] No root VC found")
        return
    }

    let screenBounds = UIScreen.main.bounds
    NSLog("[ViewportFix] Screen: \(screenBounds), VC view: \(rootVC.view.frame), safe: \(rootVC.view.safeAreaInsets)")

    // Extend layout behind all bars (status bar, home indicator)
    rootVC.edgesForExtendedLayout = .all
    rootVC.extendedLayoutIncludesOpaqueBars = true

    // Force the VC view to full screen
    rootVC.view.frame = screenBounds

    // Negate safe area insets so content fills edge-to-edge
    let safe = rootVC.view.safeAreaInsets
    rootVC.additionalSafeAreaInsets = UIEdgeInsets(
        top: -safe.top, left: -safe.left,
        bottom: -safe.bottom, right: -safe.right
    )

    // Find the WKWebView in the view hierarchy
    func findWebView(in view: UIView) -> WKWebView? {
        if let wv = view as? WKWebView { return wv }
        for sub in view.subviews {
            if let wv = findWebView(in: sub) { return wv }
        }
        return nil
    }

    if let webview = findWebView(in: rootVC.view) {
        webview.frame = screenBounds
        webview.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        webview.scrollView.contentInsetAdjustmentBehavior = .never
        webview.backgroundColor = .clear
        webview.scrollView.backgroundColor = .clear
        webview.isOpaque = false

        // Force layout recalculation
        webview.setNeedsLayout()
        webview.layoutIfNeeded()
        rootVC.view.setNeedsLayout()
        rootVC.view.layoutIfNeeded()

        NSLog("[ViewportFix] Applied — webview frame: \(webview.frame), screen: \(screenBounds)")
    } else {
        NSLog("[ViewportFix] WKWebView NOT found in hierarchy")
    }
}

@_cdecl("fix_ios_viewport")
func fixIosViewport() {
    NSLog("[ViewportFix] Called from Rust")
    // Apply with multiple delays — the view hierarchy may not be ready immediately
    for delay in [0.0, 0.1, 0.5, 1.0, 2.0] {
        DispatchQueue.main.asyncAfter(deadline: .now() + delay) {
            applyViewportFix()
        }
    }
}
