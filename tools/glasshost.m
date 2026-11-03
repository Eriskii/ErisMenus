// Glass host: loads the payload with dlopen, puts up every kind of Liquid Glass AppKit makes over a
// loud striped backdrop, captures each window as the window server composites it (glass is drawn
// there, so renderInContext: can't see it; a process may capture its own windows without Screen
// Recording access) and checks the pixels.
//
//   glasshost <liberismenus.dylib|-> <out-dir> [scenario ...]
//
// "-" skips loading the payload, to see the glass for comparison (checks then print what they
// measured and SKIP). With no scenarios named it runs them all. Exits 1 if any check failed.
#import <AppKit/AppKit.h>
#import <QuartzCore/QuartzCore.h>
#import <dlfcn.h>
#import <objc/message.h>
#import <objc/runtime.h>

static NSString *out;
static BOOL loaded;
static int failures;

typedef CGImageRef (*CaptureFn)(CGRect, uint32_t, uint32_t, uint32_t);

// The window as composited, at backing resolution, without its shadow.
static CGImageRef capture(NSWindow *w) {
    static CaptureFn fn;
    if (!fn) fn = (CaptureFn)dlsym(RTLD_DEFAULT, "CGWindowListCreateImage");
    const uint32_t includingWindow = 8, ignoreFraming = 1, bestResolution = 8;
    return fn(CGRectNull, includingWindow, (uint32_t)w.windowNumber, ignoreFraming | bestResolution);
}

static void save(CGImageRef image, NSString *name) {
    NSString *path = [out stringByAppendingFormat:@"/%@.png", name];
    NSData *png = [[[NSBitmapImageRep alloc] initWithCGImage:image] representationUsingType:NSBitmapImageFileTypePNG properties:@{}];
    [png writeToFile:path atomically:YES];
    printf("  %s\n", path.UTF8String);
}

// Pixels of a captured window, RGBA, top row first.
typedef struct { CGImageRef image; uint8_t *px; size_t w, h; CGFloat scale; NSRect frame; } Shot;

static Shot shoot(NSWindow *w, NSString *name) {
    Shot s = { capture(w) };
    s.frame = w.frame;
    if (!s.image) { printf("  FAIL could not capture %s\n", name.UTF8String); failures++; return s; }
    s.w = CGImageGetWidth(s.image), s.h = CGImageGetHeight(s.image);
    s.scale = s.w / w.frame.size.width;
    s.px = calloc(s.w * s.h, 4);
    CGColorSpaceRef space = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
    CGContextRef ctx = CGBitmapContextCreate(s.px, s.w, s.h, 8, s.w * 4, space, (CGBitmapInfo)kCGImageAlphaPremultipliedLast);
    CGContextDrawImage(ctx, CGRectMake(0, 0, s.w, s.h), s.image);
    CGContextRelease(ctx);
    CGColorSpaceRelease(space);
    save(s.image, name);
    return s;
}

// Luminance (0–1) at a point in the window's own coordinates (origin bottom left).
static double lum(Shot s, NSPoint p) {
    if (!s.px) return -1;
    long x = lround(p.x * s.scale), y = lround((s.frame.size.height - p.y) * s.scale);
    if (x < 0 || y < 0 || x >= (long)s.w || y >= (long)s.h) return -1;
    uint8_t *q = s.px + (y * s.w + x) * 4;
    return (0.2126 * q[0] + 0.7152 * q[1] + 0.0722 * q[2]) / 255.0;
}

// How much bluer than red a point is (-1–1).
static double blueness(Shot s, NSPoint p) {
    if (!s.px) return -1;
    long x = lround(p.x * s.scale), y = lround((s.frame.size.height - p.y) * s.scale);
    if (x < 0 || y < 0 || x >= (long)s.w || y >= (long)s.h) return -1;
    uint8_t *q = s.px + (y * s.w + x) * 4;
    return (q[2] - q[0]) / 255.0;
}

// The brightest pixel in a rect (window coordinates): text on a black body has to show.
static double brightest(Shot s, NSRect r) {
    double best = 0;
    for (CGFloat y = r.origin.y; y < NSMaxY(r); y += 0.5)
        for (CGFloat x = r.origin.x; x < NSMaxX(r); x += 0.5) best = MAX(best, lum(s, NSMakePoint(x, y)));
    return best;
}

static void check(BOOL ok, NSString *what, double value) {
    if (!loaded) printf("  SKIP %s (measured %.3f)\n", what.UTF8String, value);
    else {
        printf("  %s %s (measured %.3f)\n", ok ? "PASS" : "FAIL", what.UTF8String, value);
        if (!ok) failures++;
    }
}

// Glass is gone where a point that would show the stripes through it is black.
static void black(Shot s, NSPoint p, NSString *what) {
    double l = lum(s, p);
    check(l >= 0 && l < 0.06, [what stringByAppendingString:@" is opaque black"], l);
}

// Glass on glass: an opaque dark gray, so it still stands out from the black under it.
static void inner(Shot s, NSPoint p, NSString *what) {
    double l = lum(s, p);
    check(l >= 0.07 && l < 0.16, [what stringByAppendingString:@" is opaque dark gray"], l);
}

static void legible(Shot s, NSRect r, NSString *what) {
    double l = brightest(s, r);
    check(l > 0.6, [what stringByAppendingString:@" has light text"], l);
}

static void shotDone(Shot s) {
    free(s.px);
    if (s.image) CGImageRelease(s.image);
}

// Saturated stripes: anything translucent or refracting over them shows color.
@interface Stripes : NSView
@end
@implementation Stripes
- (void)drawRect:(NSRect)r {
    NSArray *colors = @[ NSColor.systemRedColor, NSColor.systemYellowColor, NSColor.systemBlueColor, NSColor.whiteColor, NSColor.systemGreenColor ];
    for (int i = 0; i * 24 < self.bounds.size.width + self.bounds.size.height; i++) {
        [(NSColor *)colors[i % colors.count] set];
        NSBezierPath *p = [NSBezierPath bezierPath];
        [p moveToPoint:NSMakePoint(i * 24, 0)];
        [p lineToPoint:NSMakePoint(i * 24 + 24, 0)];
        [p lineToPoint:NSMakePoint(i * 24 + 24 - self.bounds.size.height, self.bounds.size.height)];
        [p lineToPoint:NSMakePoint(i * 24 - self.bounds.size.height, self.bounds.size.height)];
        [p fill];
    }
}
@end

static void after(double seconds, dispatch_block_t block) {
    dispatch_after(dispatch_time(DISPATCH_TIME_NOW, (int64_t)(seconds * NSEC_PER_SEC)), dispatch_get_main_queue(), block);
}

static NSWindow *stage(NSRect frame, BOOL titled) {
    NSWindow *w = [[NSWindow alloc] initWithContentRect:frame styleMask:titled ? NSWindowStyleMaskTitled : NSWindowStyleMaskBorderless
                                                backing:NSBackingStoreBuffered defer:NO];
    w.releasedWhenClosed = NO;
    Stripes *stripes = [[Stripes alloc] initWithFrame:((NSView *)w.contentView).bounds];
    stripes.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
    [w.contentView addSubview:stripes];
    [w makeKeyAndOrderFront:nil];
    return w;
}

static NSGlassEffectView *glass(NSRect frame, CGFloat radius, NSString *label) {
    NSGlassEffectView *g = [[NSGlassEffectView alloc] initWithFrame:frame];
    g.cornerRadius = radius;
    NSView *content = [[NSView alloc] initWithFrame:g.bounds];
    NSTextField *text = [NSTextField labelWithString:label];
    text.font = [NSFont systemFontOfSize:15 weight:NSFontWeightSemibold];
    text.frame = NSMakeRect(radius / 2 + 8, frame.size.height - 32, frame.size.width - radius - 16, 22);
    [content addSubview:text];
    g.contentView = content;
    return g;
}

// A point inside `view`, as a fraction of its size, in its window's coordinates.
static NSPoint at(NSView *view, CGFloat fx, CGFloat fy) {
    return [view convertPoint:NSMakePoint(view.bounds.size.width * fx, view.bounds.size.height * fy) toView:nil];
}

static NSWindow *windowOfClass(NSString *name) {
    for (NSWindow *w in NSApp.windows)
        if (w.isVisible && [NSStringFromClass(w.class) isEqual:name]) return w;
    return nil;
}

static BOOL glassLoaded(void) {
    unsigned n = 0;
    Class *classes = objc_copyClassList(&n);
    BOOL found = NO;
    for (unsigned i = 0; i < n && !found; i++) found = strncmp(class_getName(classes[i]), "ErisMenusGlassBuild", 19) == 0;
    free(classes);
    return found;
}

@interface Host : NSObject <NSApplicationDelegate>
@property NSArray<NSString *> *scenarios;
// Prefixes the PNG names of scenarios rerun in light mode.
@property NSString *tag;
@end

@implementation Host

- (void)applicationDidFinishLaunching:(NSNotification *)n {
    [NSApp activateIgnoringOtherApps:YES];
    [self start:0];
}

// The payload installs on the main thread once the app runs (dlopen) or a moment after launch (the
// daemon); start once its glass is in.
- (void)start:(int)tries {
    if (!loaded || glassLoaded()) { after(0.3, ^{ [self run:0]; }); return; }
    if (tries > 100) { printf("the payload's glass never loaded\n"); exit(1); }
    after(0.1, ^{ [self start:tries + 1]; });
}

- (void)run:(NSUInteger)i {
    if (!self.tag) self.tag = @"";
    if (i >= self.scenarios.count) {
        printf("%s\n", failures ? "FAILED" : (loaded ? "all checks passed" : "done (no payload, nothing checked)"));
        exit(failures ? 1 : 0);
    }
    NSString *name = self.scenarios[i];
    printf("%s\n", name.UTF8String);
    void (^next)(void) = ^{ after(0.2, ^{ [self run:i + 1]; }); };
    // A scenario is the method of its name, dashes as underscores, taking `next`.
    SEL method = NSSelectorFromString([[name stringByReplacingOccurrencesOfString:@"-" withString:@"_"] stringByAppendingString:@":"]);
    if (![self respondsToSelector:method]) {
        printf("  unknown scenario\n");
        failures++;
        next();
        return;
    }
    ((void (*)(id, SEL, id))objc_msgSend)(self, method, next);
}

// NSGlassEffectView: regular, a big radius, clear, tinted.
- (void)glass:(void (^)(void))next {
    NSWindow *w = stage(NSMakeRect(200, 200, 560, 320), NO);
    NSGlassEffectView *a = glass(NSMakeRect(20, 170, 250, 130), 12, @"Regular");
    NSGlassEffectView *b = glass(NSMakeRect(290, 170, 250, 130), 48, @"Radius 48");
    NSGlassEffectView *c = glass(NSMakeRect(20, 20, 250, 130), 24, @"Clear");
    c.style = NSGlassEffectViewStyleClear;
    NSGlassEffectView *d = glass(NSMakeRect(290, 20, 250, 130), 24, @"Tinted");
    d.tintColor = NSColor.systemBlueColor;
    for (NSView *v in @[ a, b, c, d ]) [w.contentView addSubview:v];
    after(0.8, ^{
        Shot s = shoot(w, @"glass");
        black(s, at(a, 0.8, 0.3), @"regular glass");
        black(s, at(b, 0.8, 0.3), @"radius 48 glass");
        black(s, at(c, 0.8, 0.3), @"clear glass");
        legible(s, [a convertRect:NSMakeRect(10, 90, 120, 34) toView:nil], @"regular glass");
        double tint = lum(s, at(d, 0.8, 0.3));
        check(tint >= 0 && tint < 0.2, @"tinted glass is opaque and dark", tint);
        double corner = lum(s, at(b, 0.005, 0.005));
        check(corner > 0.2, @"outside the radius 48 corner shows the backdrop", corner);
        shotDone(s);
        [w close];
        next();
    });
}

// Control Center's module window: a glass panel in a borderless status-level panel, holding a
// container of glass tiles (two close enough to merge into one shape), an "on" toggle drawn as a
// plain accent fill, and labels. The stripes are inside the same window, behind the panel, since
// a capture of one window composites only that window.
- (void)control_center:(void (^)(void))next {
    NSPanel *p = [[NSPanel alloc] initWithContentRect:NSMakeRect(700, 300, 380, 340)
                                            styleMask:NSWindowStyleMaskBorderless | NSWindowStyleMaskNonactivatingPanel
                                              backing:NSBackingStoreBuffered defer:NO];
    p.level = NSStatusWindowLevel;
    p.releasedWhenClosed = NO;
    NSView *root = p.contentView;
    Stripes *stripes = [[Stripes alloc] initWithFrame:root.bounds];
    [root addSubview:stripes];
    NSGlassEffectView *panel = glass(NSMakeRect(20, 20, 340, 300), 26, @"Wi-Fi");
    [root addSubview:panel];
    NSView *content = panel.contentView;
    NSGlassEffectContainerView *container = [[NSGlassEffectContainerView alloc] initWithFrame:content.bounds];
    container.spacing = 16;
    NSView *tiles = [[NSView alloc] initWithFrame:container.bounds];
    NSMutableArray *circles = [NSMutableArray array];
    for (int i = 0; i < 3; i++) {
        NSGlassEffectView *circle = [[NSGlassEffectView alloc] initWithFrame:NSMakeRect(24 + i * 60, 170, 48, 48)];
        circle.cornerRadius = 24;
        NSImageView *icon = [NSImageView imageViewWithImage:[NSImage imageWithSystemSymbolName:@[ @"wifi", @"battery.75", @"speaker.wave.2" ][i] accessibilityDescription:nil]];
        icon.frame = NSMakeRect(12, 12, 24, 24);
        icon.contentTintColor = NSColor.whiteColor;
        circle.contentView = icon;
        [tiles addSubview:circle];
        [circles addObject:circle];
    }
    NSGlassEffectView *left = glass(NSMakeRect(24, 30, 140, 110), 18, @"Display");
    NSGlassEffectView *right = glass(NSMakeRect(170, 30, 140, 110), 18, @"Sound");
    [tiles addSubview:left];
    [tiles addSubview:right];
    container.contentView = tiles;
    [content addSubview:container];
    NSView *on = [[NSView alloc] initWithFrame:NSMakeRect(204, 170, 48, 48)];
    on.wantsLayer = YES;
    on.layer.backgroundColor = NSColor.systemBlueColor.CGColor;
    on.layer.cornerRadius = 24;
    [content addSubview:on];
    [p orderFrontRegardless];
    after(0.8, ^{
        Shot s = shoot(p, @"control-center");
        black(s, at(panel, 0.9, 0.62), @"module panel");
        inner(s, at(circles[0], 0.15, 0.5), @"module circle");
        inner(s, at(left, 0.5, 0.2), @"module tile");
        inner(s, [left convertPoint:NSMakePoint(143, 30) toView:nil], @"between merged tiles");
        legible(s, [panel convertRect:NSMakeRect(15, 266, 80, 26) toView:nil], @"module panel");
        double accent = lum(s, at(on, 0.5, 0.5));
        check(accent > 0.15, @"the accent toggle keeps its color", accent);
        double outside = lum(s, NSMakePoint(5, 5));
        check(outside > 0.2, @"outside the panel shows the stripes", outside);
        shotDone(s);
        [p close];
        next();
    });
}

- (void)popover:(void (^)(void))next {
    NSWindow *w = stage(NSMakeRect(200, 200, 420, 300), YES);
    NSButton *button = [NSButton buttonWithTitle:@"Anchor" target:nil action:nil];
    button.frame = NSMakeRect(150, 30, 120, 30);
    [w.contentView addSubview:button];
    NSPopover *pop = [NSPopover new];
    NSViewController *vc = [NSViewController new];
    vc.view = [[NSView alloc] initWithFrame:NSMakeRect(0, 0, 220, 120)];
    NSTextField *t = [NSTextField labelWithString:@"Popover"];
    t.font = [NSFont systemFontOfSize:15 weight:NSFontWeightSemibold];
    t.frame = NSMakeRect(16, 84, 180, 22);
    [vc.view addSubview:t];
    pop.contentViewController = vc;
    after(0.3, ^{
        [pop showRelativeToRect:button.bounds ofView:button preferredEdge:NSRectEdgeMaxY];
        after(0.8, ^{
            NSWindow *pw = vc.view.window;
            Shot s = shoot(pw, [self.tag stringByAppendingString:@"popover"]);
            black(s, at(vc.view, 0.8, 0.2), @"popover body");
            legible(s, [vc.view convertRect:NSMakeRect(10, 80, 100, 30) toView:nil], @"popover");
            shotDone(s);
            [pop close];
            [w close];
            next();
        });
    });
}

- (void)sheet:(void (^)(void))next {
    NSWindow *w = stage(NSMakeRect(200, 200, 460, 300), YES);
    NSAlert *alert = [NSAlert new];
    alert.messageText = @"An alert sheet";
    alert.informativeText = @"Its body was glass.";
    [alert beginSheetModalForWindow:w completionHandler:nil];
    after(1.0, ^{
        NSWindow *sheet = alert.window;
        Shot s = shoot(sheet, [self.tag stringByAppendingString:@"sheet"]);
        NSView *v = sheet.contentView;
        black(s, at(v, 0.97, 0.5), @"alert sheet body");
        legible(s, [v convertRect:v.bounds toView:nil], @"alert sheet");
        shotDone(s);
        [w endSheet:sheet];
        [w close];
        next();
    });
}

// A modal alert window (runModal), checked from a timer that runs in the modal loop.
- (void)alert:(void (^)(void))next {
    NSAlert *alert = [NSAlert new];
    alert.messageText = @"A modal alert";
    alert.informativeText = @"Run with runModal.";
    NSTimer *timer = [NSTimer timerWithTimeInterval:1.0 repeats:NO block:^(NSTimer *t) {
        NSWindow *aw = alert.window;
        Shot s = shoot(aw, @"alert");
        NSView *v = aw.contentView;
        double l = lum(s, at(v, 0.97, 0.5));
        printf("  (alert window body measured %.3f; it's %sglass)\n", l, [self hasGlass:aw.contentView.superview.layer] ? "" : "not ");
        legible(s, [v convertRect:v.bounds toView:nil], @"modal alert");
        shotDone(s);
        [NSApp abortModal];
    }];
    [NSRunLoop.currentRunLoop addTimer:timer forMode:NSModalPanelRunLoopMode];
    [alert runModal];
    next();
}

- (BOOL)hasGlass:(CALayer *)layer {
    if ([NSStringFromClass(layer.class) isEqual:@"SwiftUI.SDFLayer"]) return YES;
    for (CALayer *sub in layer.sublayers)
        if ([self hasGlass:sub]) return YES;
    return NO;
}

- (void)tooltip:(void (^)(void))next {
    NSWindow *w = stage(NSMakeRect(200, 200, 420, 200), YES);
    NSButton *button = [NSButton buttonWithTitle:@"Hover" target:nil action:nil];
    button.frame = NSMakeRect(40, 40, 120, 30);
    button.toolTip = @"A tooltip, black now";
    [w.contentView addSubview:button];
    after(0.4, ^{
        id manager = ((id (*)(id, SEL))objc_msgSend)(NSClassFromString(@"NSToolTipManager"), NSSelectorFromString(@"sharedToolTipManager"));
        ((void (*)(id, SEL, id, id))objc_msgSend)(manager, NSSelectorFromString(@"_displayTemporaryToolTipForView:withString:"), button, @"A tooltip, black now");
        after(0.8, ^{
            NSWindow *tip = windowOfClass(@"NSToolTipPanel");
            if (!tip) {
                printf("  FAIL no tooltip window (is this app frontmost?)\n");
                failures++;
            } else {
                Shot s = shoot(tip, @"tooltip");
                NSSize size = tip.frame.size;
                black(s, NSMakePoint(3, size.height / 2), @"tooltip body");
                legible(s, NSMakeRect(0, 0, size.width, size.height), @"tooltip");
                // The capture keeps the window's alpha; drawn over black, a clear corner reads 0 as well, so
                // look at the alpha.
                long cx = 0, cy = (long)s.h - 1;
                double alpha = s.px ? s.px[(cy * s.w + cx) * 4 + 3] / 255.0 : -1;
                check(alpha >= 0 && alpha < 0.2, @"tooltip corner is rounded off (clear)", alpha);
                shotDone(s);
            }
            [w close];
            next();
        });
    });
}

// A button with the glass bezel, and glass in light mode over white (adaptive glass turns its
// text dark over light backgrounds; ours is black, so the text has to stay light).
- (void)button:(void (^)(void))next {
    NSWindow *w = stage(NSMakeRect(200, 200, 420, 160), YES);
    NSButton *button = [NSButton buttonWithTitle:@"Glass button" target:nil action:nil];
    button.bezelStyle = NSBezelStyleGlass;
    button.controlSize = NSControlSizeLarge;
    button.frame = NSMakeRect(40, 50, 200, 40);
    [w.contentView addSubview:button];
    after(0.8, ^{
        Shot s = shoot(w, @"button");
        black(s, at(button, 0.08, 0.5), @"glass button");
        shotDone(s);
        [w close];
        next();
    });
}

- (void)light:(void (^)(void))next {
    NSWindow *w = [[NSWindow alloc] initWithContentRect:NSMakeRect(200, 200, 320, 180) styleMask:NSWindowStyleMaskBorderless
                                                backing:NSBackingStoreBuffered defer:NO];
    w.releasedWhenClosed = NO;
    w.appearance = [NSAppearance appearanceNamed:NSAppearanceNameAqua];
    w.backgroundColor = NSColor.whiteColor;
    NSGlassEffectView *g = glass(NSMakeRect(20, 20, 280, 140), 20, @"Light mode");
    [w.contentView addSubview:g];
    [w makeKeyAndOrderFront:nil];
    after(0.8, ^{
        Shot s = shoot(w, @"light");
        black(s, at(g, 0.8, 0.3), @"glass in light mode");
        legible(s, [g convertRect:NSMakeRect(10, 100, 140, 34) toView:nil], @"glass in light mode");
        shotDone(s);
        [w close];
        next();
    });
}

// Tinted glass in a key window (tints only show in an active one): the tint stays, now over black.
- (void)tint:(void (^)(void))next {
    NSWindow *w = stage(NSMakeRect(200, 200, 320, 180), YES);
    NSGlassEffectView *g = glass(NSMakeRect(20, 20, 280, 140), 24, @"Tinted");
    g.tintColor = NSColor.systemBlueColor;
    [w.contentView addSubview:g];
    [NSApp activateIgnoringOtherApps:YES];
    after(0.8, ^{
        Shot s = shoot(w, @"tint");
        double blue = blueness(s, at(g, 0.8, 0.3));
        check(blue > 0.4, @"tinted glass keeps its blue", blue);
        double l = lum(s, at(g, 0.8, 0.3));
        printf("  (tinted body luminance %.3f)\n", l);
        shotDone(s);
        [w close];
        next();
    });
}

// The same in an app in light mode, where glass content is dark: on black it has to turn light.
- (void)inLight:(void (^)(void))next run:(SEL)scenario {
    NSApp.appearance = [NSAppearance appearanceNamed:NSAppearanceNameAqua];
    self.tag = @"light-";
    ((void (*)(id, SEL, id))objc_msgSend)(self, scenario, ^{
        NSApp.appearance = nil;
        self.tag = @"";
        next();
    });
}

- (void)light_popover:(void (^)(void))next {
    [self inLight:next run:@selector(popover:)];
}

- (void)light_sheet:(void (^)(void))next {
    [self inLight:next run:@selector(sheet:)];
}

// Everything SwiftUI rebuilds the glass for, then glass views made and dropped: must stay black
// and not crash.
- (void)dynamic:(void (^)(void))next {
    NSWindow *w = stage(NSMakeRect(200, 200, 420, 260), NO);
    NSGlassEffectView *g = glass(NSMakeRect(20, 20, 200, 120), 16, @"Changing");
    [w.contentView addSubview:g];
    NSArray *steps = @[
        ^{ g.cornerRadius = 40; }, ^{ g.tintColor = NSColor.systemGreenColor; }, ^{ g.tintColor = nil; },
        ^{ g.style = NSGlassEffectViewStyleClear; }, ^{ g.style = NSGlassEffectViewStyleRegular; },
        ^{ g.hidden = YES; }, ^{ g.hidden = NO; },
        ^{ w.appearance = [NSAppearance appearanceNamed:NSAppearanceNameAqua]; },
        ^{ w.appearance = [NSAppearance appearanceNamed:NSAppearanceNameDarkAqua]; },
        ^{
            [NSAnimationContext runAnimationGroup:^(NSAnimationContext *c) {
                c.duration = 0.4;
                g.animator.frame = NSMakeRect(60, 60, 340, 180);
            }];
        },
        ^{
            for (int i = 0; i < 40; i++) {
                NSGlassEffectView *x = glass(NSMakeRect(i * 5, 0, 60, 40), 10, @"x");
                [w.contentView addSubview:x];
                [x display];
                [x removeFromSuperview];
            }
        },
    ];
    for (NSUInteger i = 0; i < steps.count; i++) after(0.15 * i, steps[i]);
    after(0.15 * steps.count + 0.8, ^{
        Shot s = shoot(w, @"dynamic");
        black(s, at(g, 0.85, 0.2), @"glass after every change");
        legible(s, [g convertRect:g.bounds toView:nil], @"glass after every change");
        shotDone(s);
        [w close];
        next();
    });
}

@end

int main(int argc, char **argv) {
    @autoreleasepool {
        if (argc < 3) {
            fprintf(stderr, "usage: glasshost <payload|-|--injected> <out-dir> [scenario ...]\n");
            return 2;
        }
        setvbuf(stdout, NULL, _IONBF, 0);
        out = @(argv[2]);
        [NSFileManager.defaultManager createDirectoryAtPath:out withIntermediateDirectories:YES attributes:nil error:nil];
        [NSApplication sharedApplication];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];
        BOOL injected = strcmp(argv[1], "--injected") == 0;
        if (!injected && strcmp(argv[1], "-") != 0 && !dlopen(argv[1], RTLD_NOW)) {
            fprintf(stderr, "dlopen: %s\n", dlerror());
            return 1;
        }
        loaded = strcmp(argv[1], "-") != 0;
        Host *host = [Host new];
        NSMutableArray *names = [NSMutableArray array];
        for (int i = 3; i < argc; i++) [names addObject:@(argv[i])];
        host.scenarios = names.count ? names : @[ @"glass", @"control-center", @"popover", @"sheet", @"alert", @"tooltip", @"button", @"tint", @"light", @"light-popover", @"light-sheet", @"dynamic" ];
        NSApp.delegate = host;
        [NSApp run];
    }
}
