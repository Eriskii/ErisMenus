// Development host: loads the payload with dlopen, then pops up each kind of menu, drives it with
// synthetic key presses and draws our panels into PNGs, printing what the app hears back (delegate
// calls, actions, return values) so they can be checked against AppKit's own menus.
//
//   devhost <liberismenus.dylib|-> <out-dir> [scenario ...]
//
// "-" skips loading the payload, to see AppKit's behaviour for comparison.
#import <AppKit/AppKit.h>
#import <QuartzCore/QuartzCore.h>
#import <dlfcn.h>
#import <objc/message.h>
#import <objc/runtime.h>

static NSString *out;

@interface Host : NSObject <NSMenuDelegate>
@property NSWindow *window;
@property NSPopUpButton *popup, *pulldown;
@property NSTextView *text;
@end

static void snapshot(NSString *name) {
    int n = 0;
    for (NSWindow *w in NSApp.windows) {
        if (w.level != 101 || !w.isVisible || ![w isKindOfClass:NSPanel.class]) continue;
        CALayer *layer = w.contentView.layer;
        CGFloat scale = w.backingScaleFactor;
        size_t width = layer.bounds.size.width * scale, height = layer.bounds.size.height * scale;
        CGColorSpaceRef space = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
        CGContextRef ctx = CGBitmapContextCreate(NULL, width, height, 8, 0, space, (CGBitmapInfo)kCGImageAlphaPremultipliedLast);
        // A light backdrop, so the shadow and the body's edge show.
        CGContextSetRGBFillColor(ctx, 0.93, 0.93, 0.95, 1);
        CGContextFillRect(ctx, CGRectMake(0, 0, width, height));
        CGContextScaleCTM(ctx, scale, scale);
        [layer renderInContext:ctx];
        CGImageRef image = CGBitmapContextCreateImage(ctx);
        NSData *png = [[[NSBitmapImageRep alloc] initWithCGImage:image] representationUsingType:NSBitmapImageFileTypePNG properties:@{}];
        NSString *path = [out stringByAppendingFormat:@"/%@-%d.png", name, n++];
        [png writeToFile:path atomically:YES];
        printf("  snapshot %s (%.0fx%.0f at %s)\n", path.UTF8String, layer.bounds.size.width, layer.bounds.size.height,
               NSStringFromRect(w.frame).UTF8String);
        CGImageRelease(image);
        CGContextRelease(ctx);
        CGColorSpaceRelease(space);
    }
    if (n == 0) printf("  snapshot %s: no menu panel on screen\n", name.UTF8String);
}

static void key(unsigned short code, unichar c, NSEventModifierFlags flags) {
    NSString *s = [NSString stringWithCharacters:&c length:1];
    NSEventType types[] = {NSEventTypeKeyDown, NSEventTypeKeyUp};
    for (int i = 0; i < 2; i++) {
        NSEventType type = types[i];
        NSEvent *e = [NSEvent keyEventWithType:type location:NSZeroPoint modifierFlags:flags timestamp:NSProcessInfo.processInfo.systemUptime
                                  windowNumber:0 context:nil characters:s charactersIgnoringModifiers:s isARepeat:NO keyCode:code];
        [NSApp postEvent:e atStart:NO];
    }
}

static void down(void) { key(125, NSDownArrowFunctionKey, 0); }
static void right(void) { key(124, NSRightArrowFunctionKey, 0); }
static void enter(void) { key(36, '\r', 0); }
static void escape(void) { key(53, 0x1b, 0); }
static void left(void) { key(123, NSLeftArrowFunctionKey, 0); }

// A timer in the common modes, so it also fires while a menu tracks (the main queue doesn't).
static void after(double seconds, dispatch_block_t block) {
    NSTimer *timer = [NSTimer timerWithTimeInterval:seconds repeats:NO block:^(NSTimer *t) { block(); }];
    [NSRunLoop.mainRunLoop addTimer:timer forMode:NSRunLoopCommonModes];
}

@implementation Host
- (void)act:(NSMenuItem *)item { printf("  ACTION %s\n", item.title.UTF8String); }
- (void)menuNeedsUpdate:(NSMenu *)m { printf("  delegate menuNeedsUpdate %s\n", m.title.UTF8String); }
- (void)menuWillOpen:(NSMenu *)m { printf("  delegate menuWillOpen %s\n", m.title.UTF8String); }
- (void)menuDidClose:(NSMenu *)m { printf("  delegate menuDidClose %s\n", m.title.UTF8String); }
- (void)menu:(NSMenu *)m willHighlightItem:(NSMenuItem *)item { printf("  delegate willHighlight %s\n", item ? item.title.UTF8String : "(none)"); }
- (BOOL)validateMenuItem:(NSMenuItem *)item { return ![item.title isEqualToString:@"Disabled by validation"]; }
- (void)tag:(NSButton *)b {
    printf("  TAG %s clicked in the item's own view\n", b.title.UTF8String);
    [b.enclosingMenuItem.menu cancelTracking];
}
- (void)popupChanged:(NSPopUpButton *)b { printf("  POPUP ACTION now %s\n", b.titleOfSelectedItem.UTF8String); }

- (NSMenuItem *)add:(NSMenu *)m title:(NSString *)title key:(NSString *)key mask:(NSEventModifierFlags)mask {
    NSMenuItem *item = [m addItemWithTitle:title action:@selector(act:) keyEquivalent:key];
    item.keyEquivalentModifierMask = mask;
    item.target = self;
    return item;
}

- (NSMenu *)richMenu {
    NSMenu *m = [[NSMenu alloc] initWithTitle:@"Rich"];
    m.delegate = self;
    [self add:m title:@"Open" key:@"o" mask:NSEventModifierFlagCommand].image = [NSImage imageWithSystemSymbolName:@"folder" accessibilityDescription:nil];
    [self add:m title:@"Copy" key:@"c" mask:NSEventModifierFlagCommand].image = [NSImage imageWithSystemSymbolName:@"doc.on.doc" accessibilityDescription:nil];
    NSMenuItem *alt = [self add:m title:@"Copy as Pathname" key:@"c" mask:NSEventModifierFlagCommand | NSEventModifierFlagOption];
    alt.alternate = YES;
    [self add:m title:@"Disabled by validation" key:@"" mask:0];
    [m addItem:NSMenuItem.separatorItem];
    if (@available(macOS 14, *)) [m addItem:[NSMenuItem sectionHeaderWithTitle:@"View"]];
    [self add:m title:@"Show Sidebar" key:@"s" mask:NSEventModifierFlagCommand | NSEventModifierFlagControl].state = NSControlStateValueOn;
    [self add:m title:@"Show Path Bar" key:@"p" mask:NSEventModifierFlagCommand | NSEventModifierFlagOption].state = NSControlStateValueMixed;
    NSMenuItem *sub = [self add:m title:@"Open With" key:@"" mask:0];
    sub.action = nil;
    sub.submenu = [[NSMenu alloc] initWithTitle:@"Open With"];
    sub.submenu.delegate = self;
    NSImage *safari = [NSWorkspace.sharedWorkspace iconForFile:@"/Applications/Safari.app"];
    safari.size = NSMakeSize(16, 16);
    [self add:sub.submenu title:@"Safari" key:@"" mask:0].image = safari;
    [self add:sub.submenu title:@"TextEdit" key:@"" mask:0];
    [m addItem:NSMenuItem.separatorItem];
    NSMenuItem *subtitled = [self add:m title:@"Move to Trash" key:@"\b" mask:NSEventModifierFlagCommand];
    if (@available(macOS 14.4, *)) subtitled.subtitle = @"Items in the Trash are deleted after 30 days";
    return m;
}

- (NSEvent *)rightClickAt:(NSPoint)p {
    return [NSEvent mouseEventWithType:NSEventTypeRightMouseDown location:p modifierFlags:0 timestamp:0 windowNumber:self.window.windowNumber
                               context:nil eventNumber:0 clickCount:1 pressure:1];
}

- (void)run:(NSArray<NSString *> *)scenarios index:(NSUInteger)i {
    if (i >= scenarios.count) { printf("done\n"); exit(0); }
    NSString *name = scenarios[i];
    printf("=== %s\n", name.UTF8String);
    double t0 = CACurrentMediaTime();
    if ([name isEqual:@"context"]) {
        after(0.5, ^{ snapshot(@"context"); down(); down(); });
        after(0.8, ^{ snapshot(@"context-highlight"); enter(); });
        [NSMenu popUpContextMenu:[self richMenu] withEvent:[self rightClickAt:NSMakePoint(60, 260)] forView:self.window.contentView];
    } else if ([name isEqual:@"submenu"]) {
        after(0.4, ^{ for (int k = 0; k < 5; k++) down(); right(); });
        after(0.9, ^{ snapshot(@"submenu"); down(); enter(); });
        [NSMenu popUpContextMenu:[self richMenu] withEvent:[self rightClickAt:NSMakePoint(60, 260)] forView:self.window.contentView];
    } else if ([name isEqual:@"positioning"] || [name isEqual:@"escape"]) {
        BOOL cancel = [name isEqual:@"escape"];
        after(0.4, ^{ snapshot(name); cancel ? escape() : enter(); });
        NSMenu *m = [self richMenu];
        BOOL chosen = [m popUpMenuPositioningItem:m.itemArray[1] atLocation:NSMakePoint(200, 200) inView:self.window.contentView];
        printf("  returned %d\n", chosen);
    } else if ([name isEqual:@"popup"]) {
        after(0.4, ^{ snapshot(@"popup"); down(); enter(); });
        [self.popup performClick:nil];
        printf("  popup shows %s\n", self.popup.titleOfSelectedItem.UTF8String);
    } else if ([name isEqual:@"pulldown"]) {
        after(0.4, ^{ snapshot(@"pulldown"); down(); enter(); });
        [self.pulldown performClick:nil];
    } else if ([name isEqual:@"textview"]) {
        after(0.5, ^{ snapshot(@"textview"); escape(); });
        NSEvent *e = [self rightClickAt:NSMakePoint(60, 120)];
        [NSMenu popUpContextMenu:[self.text menuForEvent:e] withEvent:e forView:self.text];
    } else if ([name isEqual:@"cancel"]) {
        NSMenu *m = [self richMenu];
        after(0.4, ^{ [m cancelTracking]; });
        BOOL chosen = [m popUpMenuPositioningItem:nil atLocation:NSMakePoint(200, 200) inView:self.window.contentView];
        printf("  returned %d\n", chosen);
    } else if ([name isEqual:@"menubar"]) {
        // The menu bar's own tracking, started the way a key that focuses the menu bar starts it.
        after(0.4, ^{ snapshot(@"menubar-app"); right(); });
        after(0.8, ^{ snapshot(@"menubar-file"); right(); left(); down(); down(); });
        after(1.2, ^{ enter(); });
        id bar = [NSApp.mainMenu valueForKey:@"_menuImpl"];
        [bar performSelector:@selector(_initTrackingSessionIfNeeded)];
        unichar c = NSF2FunctionKey;
        NSString *f2 = [NSString stringWithCharacters:&c length:1];
        NSEvent *focus = [NSEvent keyEventWithType:NSEventTypeKeyDown location:NSZeroPoint modifierFlags:NSEventModifierFlagControl
                                         timestamp:0 windowNumber:0 context:nil characters:f2 charactersIgnoringModifiers:f2 isARepeat:NO keyCode:120];
        [[bar performSelector:@selector(trackingSession)] performSelector:@selector(startMonitoringEvents:) withObject:focus];
        printf("  menu bar still tracking: %d\n", ((BOOL(*)(id, SEL))objc_msgSend)(bar, sel_registerName("_isTrackingAlreadyActive")));
    } else if ([name isEqual:@"menubar-click"]) {
        // A click on the File title, through the menu bar's own mouse-down handler as a real one.
        after(0.4, ^{ snapshot(@"menubar-click"); down(); down(); });
        after(0.8, ^{ enter(); });
        id bar = [NSApp.mainMenu valueForKey:@"_menuImpl"];
        [bar performSelector:@selector(_initTrackingSessionIfNeeded)];
        id representation = [bar performSelector:@selector(activeRepresentationView)];
        NSView *file = ((id (*)(id, SEL, long))objc_msgSend)(representation, sel_registerName("subviewAtMenuIndex:"), 1);
        NSRect r = [file convertRect:file.bounds toView:nil];
        // On the screen's top row, where the pointer is when it's pushed up into the menu bar.
        NSPoint at = NSMakePoint(file.window.frame.origin.x + NSMidX(r), NSMaxY(file.window.frame));
        NSEvent *click = [NSEvent mouseEventWithType:NSEventTypeLeftMouseDown location:at modifierFlags:0
                                           timestamp:0 windowNumber:0 context:nil eventNumber:0 clickCount:1 pressure:1];
        BOOL handled = ((BOOL (*)(id, SEL, id))objc_msgSend)([bar performSelector:@selector(trackingSession)], sel_registerName("_mouseDownEventHandler:"), click);
        printf("  handled %d, menu bar still tracking: %d\n", handled, ((BOOL(*)(id, SEL))objc_msgSend)(bar, sel_registerName("_isTrackingAlreadyActive")));
    } else if ([name isEqual:@"menubar-monitor"]) {
        // As in Chromium apps: AppKit's menu bar event monitor is installed, and a click arrives
        // while our menu is open. AppKit mustn't act on it.
        id bar = [NSApp.mainMenu valueForKey:@"_menuImpl"];
        [bar performSelector:@selector(_initTrackingSessionIfNeeded)];
        id session = [bar performSelector:@selector(trackingSession)];
        // What AppKit's own monitor does (-[NSMenuBarTrackingSession _addLocalEventMonitorIfNeeded]).
        static id monitor;
        if (!monitor)
            monitor = [NSEvent addLocalMonitorForEventsMatchingMask:NSEventMaskLeftMouseDown handler:^NSEvent *(NSEvent *e) {
                BOOL handled = ((BOOL (*)(id, SEL, id))objc_msgSend)(session, sel_registerName("_handleMonitorEvent:"), e);
                return handled ? nil : e;
            }];
        id representation = [bar performSelector:@selector(activeRepresentationView)];
        NSView *file = ((id (*)(id, SEL, long))objc_msgSend)(representation, sel_registerName("subviewAtMenuIndex:"), 1);
        NSRect r = [file convertRect:file.bounds toView:nil];
        NSPoint at = NSMakePoint(file.window.frame.origin.x + NSMidX(r), file.window.frame.origin.y + NSMidY(r));
        after(0.4, ^{
            // A second click on the menu bar while ours is open, as the monitor hands it over.
            NSEvent *e = [NSEvent mouseEventWithType:NSEventTypeLeftMouseDown location:at modifierFlags:0 timestamp:0
                                        windowNumber:0 context:nil eventNumber:0 clickCount:1 pressure:1];
            BOOL handled = ((BOOL (*)(id, SEL, id))objc_msgSend)(session, sel_registerName("_mouseDownEventHandler:"), e);
            printf("  second click handled by AppKit: %d\n", handled);
        });
        after(0.7, ^{
            for (NSWindow *w in NSApp.windows)
                if (w.isVisible && [w isKindOfClass:NSClassFromString(@"NSPopupMenuWindow")]) printf("  AppKit's own menu window is up!\n");
            down(); enter();
        });
        NSEvent *click = [NSEvent mouseEventWithType:NSEventTypeLeftMouseDown location:at modifierFlags:0
                                           timestamp:0 windowNumber:0 context:nil eventNumber:0 clickCount:1 pressure:1];
        BOOL handled = ((BOOL (*)(id, SEL, id))objc_msgSend)(session, sel_registerName("_handleMonitorEvent:"), click);
        printf("  handled %d\n", handled);
    } else if ([name isEqual:@"menubar-stray"]) {
        // AppKit also hands the menu bar's handler clicks that aren't on it; those aren't ours.
        id bar = [NSApp.mainMenu valueForKey:@"_menuImpl"];
        [bar performSelector:@selector(_initTrackingSessionIfNeeded)];
        id representation = [bar performSelector:@selector(activeRepresentationView)];
        NSView *file = ((id (*)(id, SEL, long))objc_msgSend)(representation, sel_registerName("subviewAtMenuIndex:"), 1);
        NSRect r = [file convertRect:file.bounds toView:nil];
        // Under the File title's column, but halfway down the screen.
        NSPoint at = NSMakePoint(file.window.frame.origin.x + NSMidX(r), 500);
        NSEvent *click = [NSEvent mouseEventWithType:NSEventTypeLeftMouseDown location:at modifierFlags:0
                                           timestamp:0 windowNumber:0 context:nil eventNumber:0 clickCount:1 pressure:1];
        BOOL handled = ((BOOL (*)(id, SEL, id))objc_msgSend)([bar performSelector:@selector(trackingSession)], sel_registerName("_mouseDownEventHandler:"), click);
        printf("  stray click opened a menu: %d\n", handled);
    } else if ([name isEqual:@"customview"]) {
        // An item that draws itself with a view, like Finder's tag colors: shown in our panel, and
        // a click on it goes to its button.
        NSMenu *m = [self richMenu];
        NSView *tags = [[NSView alloc] initWithFrame:NSMakeRect(0, 0, 240, 40)];
        NSButton *red = [NSButton buttonWithTitle:@"Red" target:self action:@selector(tag:)];
        red.frame = NSMakeRect(20, 6, 80, 28);
        [tags addSubview:red];
        NSMenuItem *row = [[NSMenuItem alloc] initWithTitle:@"Tags" action:nil keyEquivalent:@""];
        row.view = tags;
        [m insertItem:row atIndex:2];
        after(0.5, ^{
            snapshot(@"customview");
            NSWindow *panel = red.window;
            NSPoint at = [red convertPoint:NSMakePoint(NSMidX(red.bounds), NSMidY(red.bounds)) toView:nil];
            for (NSNumber *type in @[ @(NSEventTypeLeftMouseDown), @(NSEventTypeLeftMouseUp) ]) {
                NSEvent *e = [NSEvent mouseEventWithType:type.unsignedIntegerValue location:at modifierFlags:0 timestamp:NSProcessInfo.processInfo.systemUptime
                                            windowNumber:panel.windowNumber context:nil eventNumber:0 clickCount:1 pressure:1];
                [NSApp postEvent:e atStart:NO];
            }
            printf("  the Tags view is in %s\n", class_getName(panel.class));
        });
        BOOL chosen = [m popUpMenuPositioningItem:nil atLocation:NSMakePoint(200, 200) inView:self.window.contentView];
        printf("  returned %d, Tags view still finds its item: %d\n", chosen, tags.enclosingMenuItem == row);
    } else if ([name isEqual:@"frames"]) {
        // The opening animation, frame by frame.
        for (int f = 0; f < 8; f++) after(0.02 + f * 0.035, ^{ snapshot([NSString stringWithFormat:@"frames-%02d", f]); });
        after(0.6, ^{ escape(); });
        [NSMenu popUpContextMenu:[self richMenu] withEvent:[self rightClickAt:NSMakePoint(60, 260)] forView:self.window.contentView];
    }
    printf("  (%.2fs)\n", CACurrentMediaTime() - t0);
    after(0.3, ^{ [self run:scenarios index:i + 1]; });
}
@end

int main(int argc, char **argv) {
    @autoreleasepool {
        if (argc < 3) { fprintf(stderr, "usage: devhost <payload|-> <out-dir> [scenario ...]\n"); return 2; }
        setvbuf(stdout, NULL, _IONBF, 0);
        out = @(argv[2]);
        [NSFileManager.defaultManager createDirectoryAtPath:out withIntermediateDirectories:YES attributes:nil error:nil];
        [NSApplication sharedApplication];
        // Menus here are driven by keys alone; the pointer is whoever's at the desk.
        [NSUserDefaults.standardUserDefaults registerDefaults:@{@"ErisMenusIgnorePointer" : @YES}];
        [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];
        // "--injected": the installed daemon loads ErisMenus into this app (run as an app bundle);
        // wait until it has, and test what it loaded.
        BOOL injected = strcmp(argv[1], "--injected") == 0;
        if (!injected && strcmp(argv[1], "-") != 0 && !dlopen(argv[1], RTLD_NOW)) { fprintf(stderr, "dlopen: %s\n", dlerror()); return 1; }

        Host *host = [Host new];
        NSMenu *main = [[NSMenu alloc] initWithTitle:@""];
        for (NSArray *spec in @[ @[ @"devhost", @"About devhost" ], @[ @"File", @"New", @"Open", @"Close" ], @[ @"Edit", @"Undo", @"Copy" ] ]) {
            NSMenuItem *title = [main addItemWithTitle:spec[0] action:nil keyEquivalent:@""];
            title.submenu = [[NSMenu alloc] initWithTitle:spec[0]];
            title.submenu.delegate = host;
            for (NSString *item in [spec subarrayWithRange:NSMakeRange(1, spec.count - 1)])
                [host add:title.submenu title:item key:[[item substringToIndex:1] lowercaseString] mask:NSEventModifierFlagCommand];
        }
        NSApp.mainMenu = main;
        host.window = [[NSWindow alloc] initWithContentRect:NSMakeRect(300, 300, 480, 320) styleMask:NSWindowStyleMaskTitled
                                                    backing:NSBackingStoreBuffered defer:NO];
        host.popup = [[NSPopUpButton alloc] initWithFrame:NSMakeRect(20, 20, 150, 26) pullsDown:NO];
        [host.popup addItemsWithTitles:@[ @"One", @"Two", @"Three" ]];
        [host.popup selectItemAtIndex:1];
        host.popup.target = host;
        host.popup.action = @selector(popupChanged:);
        [host.window.contentView addSubview:host.popup];
        host.pulldown = [[NSPopUpButton alloc] initWithFrame:NSMakeRect(200, 20, 150, 26) pullsDown:YES];
        [host.pulldown addItemsWithTitles:@[ @"Actions", @"Rename", @"Duplicate" ]];
        for (NSMenuItem *item in host.pulldown.itemArray) { item.target = host; item.action = @selector(act:); }
        [host.window.contentView addSubview:host.pulldown];
        host.text = [[NSTextView alloc] initWithFrame:NSMakeRect(20, 70, 300, 100)];
        host.text.string = @"hello world";
        [host.window.contentView addSubview:host.text];
        [host.window makeKeyAndOrderFront:nil];
        [host.window makeFirstResponder:host.text];
        [NSApp activateIgnoringOtherApps:YES];

        NSMutableArray *scenarios = [NSMutableArray array];
        for (int i = 3; i < argc; i++) [scenarios addObject:@(argv[i])];
        if (scenarios.count == 0)
            [scenarios addObjectsFromArray:@[ @"context", @"submenu", @"positioning", @"escape", @"popup", @"pulldown", @"textview", @"cancel", @"menubar", @"menubar-click", @"menubar-monitor", @"menubar-stray", @"customview" ]];
        if (injected) {
            __block int waited = 0;
            __block void (^wait)(void);
            wait = ^{
                if (objc_getClass("ErisMenusLoaded")) {
                    printf("loaded by the daemon after %.1fs\n", waited / 10.0);
                    after(0.5, ^{ [host run:scenarios index:0]; });
                } else if (++waited > 200) {
                    printf("the daemon never loaded ErisMenus\n");
                    exit(1);
                } else {
                    after(0.1, wait);
                }
            };
            after(0.1, wait);
        } else {
            after(0.5, ^{ [host run:scenarios index:0]; });
        }
        [NSApp run];
    }
}
