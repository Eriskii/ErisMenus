#import <AppKit/AppKit.h>
#import <WebKit/WebKit.h>
#import <objc/runtime.h>
#import <dlfcn.h>
typedef CGImageRef (*CaptureFn)(CGRect, uint32_t, uint32_t, uint32_t);
static BOOL yes(id s, SEL c) { return YES; }
int main(int argc, char **argv) { @autoreleasepool {
    [NSApplication sharedApplication]; [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];
    for (NSString *sel in @[@"isKeyWindow", @"isMainWindow", @"_hasActiveControls", @"_hasKeyAppearance", @"_hasMainAppearance"]) { Method m = class_getInstanceMethod(NSWindow.class, NSSelectorFromString(sel)); if (m) method_setImplementation(m, (IMP)yes); }
    method_setImplementation(class_getInstanceMethod(NSApplication.class, @selector(isActive)), (IMP)yes);
    void (*h)(void) = dlsym(RTLD_DEFAULT, "hooks_install"); if (h) h();
    NSWindow *w = [[NSWindow alloc] initWithContentRect:NSMakeRect(200, 200, 400, 120) styleMask:NSWindowStyleMaskTitled backing:NSBackingStoreBuffered defer:NO];
    WKWebView *wv = [[WKWebView alloc] initWithFrame:w.contentView.bounds configuration:[WKWebViewConfiguration new]];
    w.contentView = wv;
    [wv loadHTMLString:@"<html><body style='font:16px -apple-system'><input type=checkbox checked> check <input type=radio checked> radio <input type=range value=70><br><progress value=60 max=100></progress> <div style='display:inline-block;width:60px;height:20px;background:AccentColor'></div> <div style='display:inline-block;width:60px;height:20px;background:-apple-system-control-accent'></div></body></html>" baseURL:nil];
    [w makeKeyAndOrderFront:nil]; [NSApp activateIgnoringOtherApps:YES];
    [[NSRunLoop currentRunLoop] runUntilDate:[NSDate dateWithTimeIntervalSinceNow:3.0]];
    CGImageRef img = ((CaptureFn)dlsym(RTLD_DEFAULT, "CGWindowListCreateImage"))(CGRectNull, 8, (uint32_t)w.windowNumber, 1 | 8);
    [[[[NSBitmapImageRep alloc] initWithCGImage:img] representationUsingType:NSBitmapImageFileTypePNG properties:@{}] writeToFile:@(argv[1]) atomically:YES];
}}
