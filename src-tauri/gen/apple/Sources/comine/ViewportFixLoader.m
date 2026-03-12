#import <UIKit/UIKit.h>
#import <WebKit/WebKit.h>
#import <objc/runtime.h>

static WKWebView* findWebView(UIView *view) {
    if ([view isKindOfClass:[WKWebView class]]) return (WKWebView *)view;
    for (UIView *sub in view.subviews) {
        WKWebView *wv = findWebView(sub);
        if (wv) return wv;
    }
    return nil;
}

static void applyViewportFix(void) {
    UIWindowScene *scene = (UIWindowScene *)UIApplication.sharedApplication.connectedScenes.anyObject;
    if (!scene) { NSLog(@"[ViewportFix] No scene"); return; }

    UIWindow *window = nil;
    for (UIWindow *w in scene.windows) {
        if (w.isKeyWindow) { window = w; break; }
    }
    if (!window) { NSLog(@"[ViewportFix] No key window"); return; }

    UIViewController *rootVC = window.rootViewController;
    if (!rootVC) { NSLog(@"[ViewportFix] No root VC"); return; }

    CGRect screenBounds = UIScreen.mainScreen.bounds;
    NSLog(@"[ViewportFix] Screen: %@ | VC view: %@ | Safe: %@",
          NSStringFromCGRect(screenBounds),
          NSStringFromCGRect(rootVC.view.frame),
          NSStringFromUIEdgeInsets(rootVC.view.safeAreaInsets));

    // Extend layout behind status bar and home indicator
    rootVC.edgesForExtendedLayout = UIRectEdgeAll;
    rootVC.extendedLayoutIncludesOpaqueBars = YES;

    // Force VC view to full screen
    rootVC.view.frame = screenBounds;

    // Negate safe area so content fills edge-to-edge
    UIEdgeInsets safe = rootVC.view.safeAreaInsets;
    rootVC.additionalSafeAreaInsets = UIEdgeInsetsMake(
        -safe.top, -safe.left, -safe.bottom, -safe.right
    );

    WKWebView *webview = findWebView(rootVC.view);
    if (webview) {
        webview.frame = screenBounds;
        webview.autoresizingMask = UIViewAutoresizingFlexibleWidth | UIViewAutoresizingFlexibleHeight;
        webview.scrollView.contentInsetAdjustmentBehavior = UIScrollViewContentInsetAdjustmentNever;
        webview.backgroundColor = UIColor.clearColor;
        webview.scrollView.backgroundColor = UIColor.clearColor;
        webview.opaque = NO;

        [webview setNeedsLayout];
        [webview layoutIfNeeded];
        [rootVC.view setNeedsLayout];
        [rootVC.view layoutIfNeeded];

        NSLog(@"[ViewportFix] Applied — webview: %@", NSStringFromCGRect(webview.frame));
    } else {
        NSLog(@"[ViewportFix] WKWebView NOT found");
    }
}

@interface ViewportFixLoader : NSObject
@end

@implementation ViewportFixLoader

+ (void)load {
    // +load is called by the ObjC runtime before main() — guaranteed to execute
    NSLog(@"[ViewportFix] +load called, registering observer");

    __block id observer = nil;
    observer = [[NSNotificationCenter defaultCenter]
        addObserverForName:UISceneDidActivateNotification
        object:nil
        queue:[NSOperationQueue mainQueue]
        usingBlock:^(NSNotification *note) {
            NSLog(@"[ViewportFix] Scene activated, applying fix");
            // Apply multiple times to catch any relayout
            applyViewportFix();
            dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(0.3 * NSEC_PER_SEC)),
                dispatch_get_main_queue(), ^{ applyViewportFix(); });
            dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(1.0 * NSEC_PER_SEC)),
                dispatch_get_main_queue(), ^{ applyViewportFix(); });
            // Remove observer after first activation
            [[NSNotificationCenter defaultCenter] removeObserver:observer];
        }];
}

@end
