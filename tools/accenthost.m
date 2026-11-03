#import <AppKit/AppKit.h>
#import <objc/runtime.h>
#import <dlfcn.h>
typedef CGImageRef (*CaptureFn)(CGRect, uint32_t, uint32_t, uint32_t);
static BOOL yes(id s, SEL c) { return YES; }
static NSView *board(void) {
    NSView *v = [[NSView alloc] initWithFrame:NSMakeRect(0, 0, 580, 230)];
    v.wantsLayer = YES; v.layer.backgroundColor = NSColor.windowBackgroundColor.CGColor;
    NSButton *cb = [NSButton checkboxWithTitle:@"Checkbox" target:nil action:nil]; cb.state = NSControlStateValueOn; cb.frame = NSMakeRect(20, 190, 120, 20);
    NSButton *rb = [NSButton radioButtonWithTitle:@"Radio" target:nil action:nil]; rb.state = NSControlStateValueOn; rb.frame = NSMakeRect(150, 190, 100, 20);
    NSSwitch *sw = [[NSSwitch alloc] initWithFrame:NSMakeRect(260, 188, 60, 24)]; sw.state = NSControlStateValueOn;
    NSButton *def = [NSButton buttonWithTitle:@"Default" target:nil action:nil]; def.keyEquivalent = @"\r"; def.frame = NSMakeRect(340, 186, 100, 28);
    NSSlider *sl = [NSSlider sliderWithValue:60 minValue:0 maxValue:100 target:nil action:nil]; sl.frame = NSMakeRect(20, 150, 220, 24);
    NSProgressIndicator *pi = [[NSProgressIndicator alloc] initWithFrame:NSMakeRect(260, 150, 220, 20)]; pi.indeterminate = NO; pi.doubleValue = 60; pi.style = NSProgressIndicatorStyleBar;
    NSSegmentedControl *seg = [NSSegmentedControl segmentedControlWithLabels:@[@"One", @"Two", @"Three"] trackingMode:NSSegmentSwitchTrackingSelectOne target:nil action:nil]; seg.selectedSegment = 1; seg.frame = NSMakeRect(20, 105, 220, 28);
    NSTextField *tf = [NSTextField textFieldWithString:@"focus ring"]; tf.frame = NSMakeRect(260, 108, 200, 24);
    NSView *swatch = [[NSView alloc] initWithFrame:NSMakeRect(20, 20, 220, 60)]; swatch.wantsLayer = YES; swatch.layer.backgroundColor = NSColor.controlAccentColor.CGColor;
    NSView *sel = [[NSView alloc] initWithFrame:NSMakeRect(260, 20, 220, 60)]; sel.wantsLayer = YES; sel.layer.backgroundColor = NSColor.selectedContentBackgroundColor.CGColor;
    NSImageView *icon = [NSImageView imageViewWithImage:[NSWorkspace.sharedWorkspace iconForFile:@"/System"]]; icon.frame = NSMakeRect(496, 20, 64, 64);
    if (getenv("PRESSED")) { [def highlight:YES]; [cb highlight:YES]; }
    for (NSView *x in @[cb, rb, sw, def, sl, pi, seg, tf, swatch, sel, icon]) [v addSubview:x];
    return v;
}
// Accent host: loads the payload (argv[1], or "-" for none) the way devhost does, shows the board
// and captures it every 2 s into argv[2]/frame-N.png, argv[3] times; meanwhile `erismenus set accent`
// can change the accent live (see docs/testing.md).
int main(int argc, char **argv) { @autoreleasepool {
    if (argc < 4) { fprintf(stderr, "usage: accenthost <dylib|-> <outdir> <frames>\n"); return 2; }
    setvbuf(stdout, NULL, _IONBF, 0);
    [NSApplication sharedApplication];
    [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];
    // Controls draw their accent only in a key window of an active app; we can't take focus.
    for (NSString *sel in @[@"isKeyWindow", @"isMainWindow", @"_hasActiveControls", @"_hasKeyAppearance", @"_hasMainAppearance"]) {
        Method m = class_getInstanceMethod(NSWindow.class, NSSelectorFromString(sel));
        if (m) method_setImplementation(m, (IMP)yes);
    }
    method_setImplementation(class_getInstanceMethod(NSApplication.class, @selector(isActive)), (IMP)yes);
    if (getenv("DARK")) NSApp.appearance = [NSAppearance appearanceNamed:NSAppearanceNameDarkAqua];
    if (getenv("LIGHT")) NSApp.appearance = [NSAppearance appearanceNamed:NSAppearanceNameAqua];
    if (strcmp(argv[1], "-") != 0 && !dlopen(argv[1], RTLD_NOW)) { fprintf(stderr, "dlopen: %s\n", dlerror()); return 1; }
    NSString *out = [NSString stringWithUTF8String:argv[2]];
    int frames = atoi(argv[3]);
    CaptureFn cap = (CaptureFn)dlsym(RTLD_DEFAULT, "CGWindowListCreateImage");
    NSWindow *w = [[NSWindow alloc] initWithContentRect:NSMakeRect(200, 200, 580, 230) styleMask:NSWindowStyleMaskTitled backing:NSBackingStoreBuffered defer:NO];
    NSView *b = board(); w.contentView = b; [w makeKeyAndOrderFront:nil];
    [w makeFirstResponder:b.subviews[7]];
    for (int i = 0; i < frames; i++) {
        [[NSRunLoop currentRunLoop] runUntilDate:[NSDate dateWithTimeIntervalSinceNow:2.0]];
        NSColor *a = [NSColor.controlAccentColor colorUsingColorSpace:NSColorSpace.sRGBColorSpace];
        CGImageRef img = cap(CGRectNull, 8, (uint32_t)w.windowNumber, 1 | 8);
        NSBitmapImageRep *rep = [[NSBitmapImageRep alloc] initWithCGImage:img];
        NSString *path = [out stringByAppendingFormat:@"/frame-%d.png", i];
        [[rep representationUsingType:NSBitmapImageFileTypePNG properties:@{}] writeToFile:path atomically:YES];
        printf("frame %d: controlAccentColor %02X%02X%02X\n", i, (int)lround(a.redComponent * 255), (int)lround(a.greenComponent * 255), (int)lround(a.blueComponent * 255));
    }
}}
