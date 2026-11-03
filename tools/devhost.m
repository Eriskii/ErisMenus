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

@interface Host : NSObject <NSMenuDelegate, NSToolbarDelegate, NSTokenFieldDelegate>
@property NSWindow *window, *controls;
@property NSPopUpButton *popup, *pulldown;
@property NSTextView *text;
@property NSSearchField *search;
@property NSSegmentedControl *segmented;
@property NSPathControl *path;
@property NSComboBox *combo;
@property NSTokenField *tokens;
@property NSStatusItem *status;
@end

// What the menu bar last told the window server to draw highlighted (the title under the selection
// capsule), from the settings AppKit hands +[NSCGSSpace setMenuBarSettings:globalSettings:].
static NSString *barHighlight = @"(nothing sent)";
static IMP originalSetMenuBarSettings;

static void recordMenuBarSettings(id self, SEL _cmd, NSArray *settings, id global) {
    NSMutableArray *rects = [NSMutableArray array];
    for (NSDictionary *display in settings)
        for (NSData *data in display[@"menuBarMenuTitlesArray"])
            if (data.length == sizeof(CGRect)) [rects addObject:NSStringFromRect(*(const CGRect *)data.bytes)];
    barHighlight = rects.count ? [rects componentsJoinedByString:@" "] : @"none";
    ((void (*)(id, SEL, id, id))originalSetMenuBarSettings)(self, _cmd, settings, global);
}

// Which menus are up: ours (borderless panels at the menu level) or AppKit's (NSPopupMenuWindow,
// Liquid Glass).
static void which(NSString *what) {
    int ours = 0, appkits = 0;
    Class glass = NSClassFromString(@"NSPopupMenuWindow");
    for (NSWindow *w in NSApp.windows) {
        if (!w.isVisible) continue;
        if (glass && [w isKindOfClass:glass]) appkits++;
        else if (w.level == 101 && [w isMemberOfClass:NSPanel.class]) ours++;
        else {
            if (w.level >= 101) printf("  other window up: %s at level %ld\n", class_getName(w.class), w.level);
            continue;
        }
        printf("  menu window at %s\n", NSStringFromRect(w.frame).UTF8String);
    }
    printf("  %s: %d of our menus, %d of AppKit's\n", what.UTF8String, ours, appkits);
}

// A click on a view, as the window server would deliver it; or only its press or its release.
static void mouse(NSView *view, NSPoint at, NSArray<NSNumber *> *types) {
    NSPoint p = [view convertPoint:at toView:nil];
    for (NSNumber *type in types) {
        NSEvent *e = [NSEvent mouseEventWithType:type.unsignedIntegerValue location:p modifierFlags:0 timestamp:NSProcessInfo.processInfo.systemUptime
                                    windowNumber:view.window.windowNumber context:nil eventNumber:0 clickCount:1 pressure:1];
        [NSApp postEvent:e atStart:NO];
    }
}

static void click(NSView *view, NSPoint at) { mouse(view, at, @[ @(NSEventTypeLeftMouseDown), @(NSEventTypeLeftMouseUp) ]); }

static NSRect onScreen(NSView *view) { return [view.window convertRectToScreen:[view convertRect:view.bounds toView:nil]]; }

static id menuBar(void) {
    id bar = [NSApp.mainMenu valueForKey:@"_menuImpl"];
    [bar performSelector:@selector(_initTrackingSessionIfNeeded)];
    return bar;
}

static NSView *titleView(long index) {
    id representation = [menuBar() performSelector:@selector(activeRepresentationView)];
    return ((id (*)(id, SEL, long))objc_msgSend)(representation, sel_registerName("subviewAtMenuIndex:"), index);
}

// A left click on menu bar title `index` (-1: the Apple menu) on the screen's top row, through the
// menu bar's own mouse-down handler as a real one.
static BOOL clickTitle(long index) {
    NSView *title = titleView(index);
    NSRect r = [title convertRect:title.bounds toView:nil];
    NSPoint at = NSMakePoint(title.window.frame.origin.x + NSMidX(r), NSMaxY(title.window.frame));
    NSEvent *click = [NSEvent mouseEventWithType:NSEventTypeLeftMouseDown location:at modifierFlags:0
                                       timestamp:0 windowNumber:0 context:nil eventNumber:0 clickCount:1 pressure:1];
    return ((BOOL (*)(id, SEL, id))objc_msgSend)([menuBar() performSelector:@selector(trackingSession)], sel_registerName("_mouseDownEventHandler:"), click);
}

extern int CGSMainConnectionID(void);
extern int CGSGetBackgroundEventMask(int cid, uint32_t *mask);
extern int CPSGetKeyFocusProcess(ProcessSerialNumber *psn, uint8_t *unused);

// Whether this app has the keyboard, and which mouse events it gets from outside its windows.
static void focusState(NSString *when) {
    uint32_t mask = 0;
    CGSGetBackgroundEventMask(CGSMainConnectionID(), &mask);
    ProcessSerialNumber psn = {0, 0}, current = {0, kCurrentProcess};
    uint8_t unused = 0;
    Boolean same = 0;
    CPSGetKeyFocusProcess(&psn, &unused);
#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
    SameProcess(&psn, &current, &same);
#pragma clang diagnostic pop
    printf("  %s: active %d, has the keyboard %d, background events 0x%x\n", when.UTF8String, NSApp.isActive, same, mask);
}

static long highlightedTitle(void) { return ((long (*)(id, SEL))objc_msgSend)(menuBar(), sel_registerName("_indexOfHighlightedViewInMenu")); }

static void snapshot(NSString *name) {
    int n = 0;
    Class glass = NSClassFromString(@"NSPopupMenuWindow");
    for (NSWindow *w in NSApp.windows) {
        if (w.isVisible && glass && [w isKindOfClass:glass]) {
            // AppKit's own menu (with "-"): its views, without the glass behind them.
            NSView *view = w.contentView;
            NSBitmapImageRep *rep = [view bitmapImageRepForCachingDisplayInRect:view.bounds];
            NSGraphicsContext *context = [NSGraphicsContext graphicsContextWithBitmapImageRep:rep];
            [NSGraphicsContext saveGraphicsState];
            NSGraphicsContext.currentContext = context;
            [[NSColor colorWithWhite:0.35 alpha:1] setFill];
            NSRectFill(view.bounds);
            [NSGraphicsContext restoreGraphicsState];
            [view displayRectIgnoringOpacity:view.bounds inContext:context];
            NSString *path = [out stringByAppendingFormat:@"/%@-appkit-%d.png", name, n++];
            [[rep representationUsingType:NSBitmapImageFileTypePNG properties:@{}] writeToFile:path atomically:YES];
            printf("  snapshot %s (AppKit's, at %s)\n", path.UTF8String, NSStringFromRect(w.frame).UTF8String);
            continue;
        }
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
- (void)appleAction:(NSMenuItem *)item { printf("  APPLE MENU ACTION %s (swallowed)\n", item.title.UTF8String); }
- (void)segment:(NSSegmentedControl *)c { printf("  SEGMENT ACTION %ld\n", c.selectedSegment); }
- (void)pathChosen:(NSPathControl *)c { printf("  PATH ACTION %s\n", c.clickedPathItem.title.UTF8String); }
- (NSArray<NSToolbarItemIdentifier> *)toolbarAllowedItemIdentifiers:(NSToolbar *)t { return @[ @"menu" ]; }
- (NSArray<NSToolbarItemIdentifier> *)toolbarDefaultItemIdentifiers:(NSToolbar *)t { return @[ @"menu" ]; }
- (NSToolbarItem *)toolbar:(NSToolbar *)t itemForItemIdentifier:(NSToolbarItemIdentifier)identifier willBeInsertedIntoToolbar:(BOOL)flag {
    NSMenuToolbarItem *item = [[NSMenuToolbarItem alloc] initWithItemIdentifier:identifier];
    item.label = @"Menu";
    item.image = [NSImage imageWithSystemSymbolName:@"ellipsis.circle" accessibilityDescription:nil];
    item.menu = [self smallMenu:@"Toolbar"];
    return item;
}
- (BOOL)tokenField:(NSTokenField *)f hasMenuForRepresentedObject:(id)o { return YES; }
- (NSMenu *)tokenField:(NSTokenField *)f menuForRepresentedObject:(id)o { return [self smallMenu:@"Token"]; }

- (NSMenu *)smallMenu:(NSString *)title {
    NSMenu *m = [[NSMenu alloc] initWithTitle:title];
    m.delegate = self;
    for (NSString *t in @[ @"Alpha", @"Beta", @"Gamma" ]) [self add:m title:t key:@"" mask:0];
    return m;
}

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
    // How long to wait before the next scenario: the ones that only post events return before
    // their menu has even opened.
    double settle = 0.3;
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
    } else if ([name isEqual:@"apple"]) {
        // The Apple menu from the key that focuses the menu bar (Control-F2): AppKit highlights the
        // Apple title first. ← wraps around to Edit, → comes back, Return picks About This Mac.
        after(0.4, ^{
            which(@"apple");
            snapshot(@"apple");
            printf("  highlighted title %ld, the bar draws selection at %s\n", highlightedTitle(), barHighlight.UTF8String);
            left();
        });
        after(0.7, ^{ printf("  after ←, highlighted title %ld\n", highlightedTitle()); right(); });
        after(1.0, ^{ printf("  after →, highlighted title %ld\n", highlightedTitle()); enter(); });
        unichar c = NSF2FunctionKey;
        NSString *f2 = [NSString stringWithCharacters:&c length:1];
        NSEvent *focus = [NSEvent keyEventWithType:NSEventTypeKeyDown location:NSZeroPoint modifierFlags:NSEventModifierFlagControl
                                         timestamp:0 windowNumber:0 context:nil characters:f2 charactersIgnoringModifiers:f2 isARepeat:NO keyCode:120];
        [[menuBar() performSelector:@selector(trackingSession)] performSelector:@selector(handleMenuBarFocusSymbolicHotKey:) withObject:focus];
        printf("  menu bar still tracking: %d\n", ((BOOL (*)(id, SEL))objc_msgSend)(menuBar(), sel_registerName("_isTrackingAlreadyActive")));
        printf("  AppKit's tracking session left over: %d\n", [NSClassFromString(@"NSMenuTrackingSession") performSelector:@selector(currentSession)] != nil);
        focusState(@"after");
    } else if ([name isEqual:@"apple-click"]) {
        settle = 1.5;
        // A click on the Apple title; → moves on to the app menu, ← back. Escape.
        after(0.4, ^{ which(@"apple-click"); snapshot(@"apple-click"); right(); });
        after(0.7, ^{ printf("  after →, highlighted title %ld\n", highlightedTitle()); left(); });
        after(1.0, ^{ printf("  after ←, highlighted title %ld\n", highlightedTitle()); escape(); });
        BOOL handled = clickTitle(-1);
        printf("  handled %d\n", handled);
    } else if ([name isEqual:@"menubar-highlight"]) {
        settle = 1.5;
        // The title under an open menu is highlighted: AppKit tells the window server, which draws
        // the menu bar, which title rects to draw the selection behind.
        NSView *file = titleView(1);
        printf("  File title at %s in the bar\n", NSStringFromRect(file.frame).UTF8String);
        after(0.4, ^{ printf("  open: highlighted title %ld, the bar draws selection at %s\n", highlightedTitle(), barHighlight.UTF8String); escape(); });
        after(0.6, ^{ printf("  closed: highlighted title %ld, the bar draws selection at %s\n", highlightedTitle(), barHighlight.UTF8String); });
        clickTitle(1);
    } else if ([name isEqual:@"statusitem"]) {
        // A menu bar extra with a menu: hangs from the item, the item highlighted while it's open.
        NSRect item = onScreen(self.status.button);
        printf("  status item at %s\n", NSStringFromRect(item).UTF8String);
        after(0.4, ^{
            which(@"statusitem");
            snapshot(@"statusitem");
            printf("  button highlighted while open: %d\n", self.status.button.isHighlighted);
            down(); down(); enter();
        });
        [self.status.button performClick:nil];
        printf("  button highlighted after: %d\n", self.status.button.isHighlighted);
    } else if ([name isEqual:@"inactive"]) {
        settle = 1.5;
        // A menu bar extra's menu in an app that isn't frontmost, as an agent's always is: the
        // menu takes the keyboard and mouse events from elsewhere while it's open, and gives them
        // back.
        [NSApp hide:nil];
        after(0.6, ^{
            focusState(@"before");
            after(0.4, ^{ which(@"inactive"); focusState(@"open"); down(); enter(); });
            [self.status.button performClick:nil];
            focusState(@"after");
            [NSApp unhide:nil];
            [NSApp activateIgnoringOtherApps:YES];
        });
    } else if ([name isEqual:@"searchfield"]) {
        settle = 1.5;
        // A search field's recent searches menu, from a click on its magnifier.
        after(0.5, ^{ which(@"searchfield"); snapshot(@"searchfield"); escape(); });
        NSRect r = [self.search.cell searchButtonRectForBounds:self.search.bounds];
        printf("  search field at %s\n", NSStringFromRect(onScreen(self.search)).UTF8String);
        click(self.search, NSMakePoint(NSMidX(r), NSMidY(r)));
    } else if ([name isEqual:@"segmented"]) {
        settle = 1.8;
        // A segment with a menu: holding the button down on it shows the menu below the segment.
        NSPoint at = NSMakePoint(self.segmented.bounds.size.width * 0.75, NSMidY(self.segmented.bounds));
        after(1.0, ^{ which(@"segmented"); snapshot(@"segmented"); down(); enter(); });
        after(1.3, ^{ mouse(self.segmented, at, @[ @(NSEventTypeLeftMouseUp) ]); });
        printf("  segmented control at %s\n", NSStringFromRect(onScreen(self.segmented)).UTF8String);
        mouse(self.segmented, at, @[ @(NSEventTypeLeftMouseDown) ]);
    } else if ([name isEqual:@"pathcontrol"]) {
        settle = 1.5;
        // A pop-up path control.
        after(0.5, ^{ which(@"pathcontrol"); snapshot(@"pathcontrol"); escape(); });
        printf("  path control at %s\n", NSStringFromRect(onScreen(self.path)).UTF8String);
        click(self.path, NSMakePoint(20, NSMidY(self.path.bounds)));
    } else if ([name isEqual:@"combobox"]) {
        settle = 1.5;
        // A combo box's list (not an NSMenu: NSComboBoxWindow).
        after(0.5, ^{
            for (NSWindow *w in NSApp.windows)
                if (w.isVisible && w != self.window && w != self.controls && ![w isKindOfClass:NSClassFromString(@"NSStatusBarWindow")])
                {
                    printf("  window up: %s level %ld at %s:", class_getName(w.class), w.level, NSStringFromRect(w.frame).UTF8String);
                    for (NSView *v = w.contentView; v; v = v.subviews.firstObject) printf(" %s", class_getName(v.class));
                    printf("\n");
                }
            which(@"combobox");
            escape();
        });
        after(0.9, ^{ [self.window makeKeyAndOrderFront:nil]; });
        [self.controls makeKeyAndOrderFront:nil];
        [self.controls makeFirstResponder:self.combo];
        [(NSComboBoxCell *)self.combo.cell performSelector:@selector(popUp:) withObject:nil];
    } else if ([name isEqual:@"tokenfield"]) {
        settle = 1.5;
        // A token's menu, from a click on the token.
        after(0.5, ^{ which(@"tokenfield"); snapshot(@"tokenfield"); escape(); });
        after(0.9, ^{ [self.window makeKeyAndOrderFront:nil]; [self.window makeFirstResponder:self.text]; });
        [self.controls makeKeyAndOrderFront:nil];
        [self.controls makeFirstResponder:self.tokens];
        // The token's pull-down arrow, in the field editor.
        NSTextView *editor = (NSTextView *)self.tokens.currentEditor;
        NSRect glyph = [editor.layoutManager boundingRectForGlyphRange:NSMakeRange(0, 1) inTextContainer:editor.textContainer];
        glyph = NSOffsetRect(glyph, editor.textContainerOrigin.x, editor.textContainerOrigin.y);
        NSTextAttachment *token = [editor.textStorage attribute:NSAttachmentAttributeName atIndex:0 effectiveRange:NULL];
        NSRect arrow = ((NSRect (*)(id, SEL, NSRect))objc_msgSend)(token.attachmentCell, sel_registerName("pullDownRectForBounds:"), glyph);
        click(editor, NSMakePoint(NSMidX(arrow), NSMidY(arrow)));
    } else if ([name isEqual:@"toolbar"]) {
        settle = 1.5;
        // An NSMenuToolbarItem.
        after(0.5, ^{ which(@"toolbar"); snapshot(@"toolbar"); down(); enter(); });
        NSToolbarItem *item = self.controls.toolbar.items.firstObject;
        NSView *view = [item valueForKey:@"_itemViewer"] ?: item.view;
        printf("  toolbar item view %s\n", class_getName([view class]));
        click(view, NSMakePoint(NSMidX(view.bounds), NSMidY(view.bounds)));
    } else if ([name isEqual:@"rightclick"]) {
        settle = 1.5;
        // A real right click on the text view: NSView's rightMouseDown: → menuForEvent:.
        after(0.5, ^{ which(@"rightclick"); snapshot(@"rightclick"); escape(); });
        NSPoint p = [self.text convertPoint:NSMakePoint(30, 10) toView:nil];
        for (NSNumber *type in @[ @(NSEventTypeRightMouseDown), @(NSEventTypeRightMouseUp) ]) {
            NSEvent *e = [NSEvent mouseEventWithType:type.unsignedIntegerValue location:p modifierFlags:0 timestamp:NSProcessInfo.processInfo.systemUptime
                                        windowNumber:self.window.windowNumber context:nil eventNumber:0 clickCount:1 pressure:1];
            [NSApp postEvent:e atStart:NO];
        }
    } else if ([name isEqual:@"palette"]) {
        // A palette (inline, horizontal) submenu, as in Finder's tags.
        NSMenu *m = [self smallMenu:@"Palette"];
        NSMenuItem *colors = [[NSMenuItem alloc] initWithTitle:@"Colors" action:nil keyEquivalent:@""];
        colors.submenu = [[NSMenu alloc] initWithTitle:@"Colors"];
        for (NSString *symbol in @[ @"circle.fill", @"square.fill", @"triangle.fill" ]) {
            NSMenuItem *item = [self add:colors.submenu title:symbol key:@"" mask:0];
            item.image = [NSImage imageWithSystemSymbolName:symbol accessibilityDescription:nil];
        }
        if (@available(macOS 14, *)) colors.submenu.presentationStyle = NSMenuPresentationStylePalette;
        [m insertItem:colors atIndex:1];
        // Ours shows it as a submenu: ↓ ↓ → opens it, Return picks its first item.
        after(0.4, ^{ down(); down(); right(); });
        after(0.8, ^{ which(@"palette"); snapshot(@"palette"); enter(); });
        BOOL chosen = [m popUpMenuPositioningItem:nil atLocation:NSMakePoint(200, 200) inView:self.window.contentView];
        printf("  returned %d\n", chosen);
    } else if ([name isEqual:@"services"]) {
        settle = 1.5;
        // The app menu's Services submenu, filled by the system.
        after(0.4, ^{ for (int k = 0; k < 2; k++) down(); right(); });
        after(0.9, ^{ which(@"services"); snapshot(@"services"); escape(); });
        clickTitle(0);
    } else if ([name isEqual:@"share"]) {
        settle = 1.5;
        // The share menu (NSSharingServicePicker).
        after(0.8, ^{ which(@"share"); snapshot(@"share"); escape(); });
        NSSharingServicePicker *picker = [[NSSharingServicePicker alloc] initWithItems:@[ @"hello" ]];
        [picker showRelativeToRect:self.popup.bounds ofView:self.popup preferredEdge:NSRectEdgeMinY];
    } else if ([name isEqual:@"frames"]) {
        // The opening animation, frame by frame.
        for (int f = 0; f < 8; f++) after(0.02 + f * 0.035, ^{ snapshot([NSString stringWithFormat:@"frames-%02d", f]); });
        after(0.6, ^{ escape(); });
        [NSMenu popUpContextMenu:[self richMenu] withEvent:[self rightClickAt:NSMakePoint(60, 260)] forView:self.window.contentView];
    }
    printf("  (%.2fs)\n", CACurrentMediaTime() - t0);
    after(settle, ^{ [self run:scenarios index:i + 1]; });
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
        NSMenuItem *services = [[NSMenuItem alloc] initWithTitle:@"Services" action:nil keyEquivalent:@""];
        services.submenu = [[NSMenu alloc] initWithTitle:@"Services"];
        NSApp.servicesMenu = services.submenu;
        [main.itemArray[0].submenu addItem:NSMenuItem.separatorItem];
        [main.itemArray[0].submenu addItem:services];
        NSApp.mainMenu = main;
        Method settings = class_getClassMethod(NSClassFromString(@"NSCGSSpace"), sel_registerName("setMenuBarSettings:globalSettings:"));
        if (settings) originalSetMenuBarSettings = method_setImplementation(settings, (IMP)recordMenuBarSettings);
        // The Apple menu's items would really sleep, restart or log out.
        Class appleMenu = NSClassFromString(@"_NSAppleMenu");
        unsigned count;
        Method *methods = class_copyMethodList(appleMenu, &count);
        for (unsigned k = 0; k < count; k++) {
            const char *name = sel_getName(method_getName(methods[k]));
            if (name[0] == '_' && strstr(name, "Requested:")) method_setImplementation(methods[k], method_getImplementation(class_getInstanceMethod(Host.class, @selector(appleAction:))));
        }
        free(methods);
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
        // The other controls that show menus, in a window of their own with a toolbar.
        host.controls = [[NSWindow alloc] initWithContentRect:NSMakeRect(820, 300, 480, 220) styleMask:NSWindowStyleMaskTitled
                                                      backing:NSBackingStoreBuffered defer:NO];
        NSToolbar *toolbar = [[NSToolbar alloc] initWithIdentifier:@"devhost"];
        toolbar.delegate = host;
        host.controls.toolbar = toolbar;
        host.search = [[NSSearchField alloc] initWithFrame:NSMakeRect(20, 170, 200, 24)];
        host.search.recentsAutosaveName = nil;
        NSMenu *template = [[NSMenu alloc] initWithTitle:@"Search"];
        [template addItemWithTitle:@"Recent Searches" action:nil keyEquivalent:@""].tag = NSSearchFieldRecentsTitleMenuItemTag;
        [template addItemWithTitle:@"" action:nil keyEquivalent:@""].tag = NSSearchFieldRecentsMenuItemTag;
        [template addItem:NSMenuItem.separatorItem];
        [template addItemWithTitle:@"Clear Recents" action:nil keyEquivalent:@""].tag = NSSearchFieldClearRecentsMenuItemTag;
        [(NSSearchFieldCell *)host.search.cell setSearchMenuTemplate:template];
        host.search.recentSearches = @[ @"menus", @"glass" ];
        [host.controls.contentView addSubview:host.search];
        host.segmented = [NSSegmentedControl segmentedControlWithLabels:@[ @"Plain", @"With Menu" ] trackingMode:NSSegmentSwitchTrackingMomentary
                                                                 target:host action:@selector(segment:)];
        host.segmented.frame = NSMakeRect(240, 170, 200, 24);
        [host.segmented setMenu:[host smallMenu:@"Segment"] forSegment:1];
        [host.segmented setShowsMenuIndicator:YES forSegment:1];
        [host.controls.contentView addSubview:host.segmented];
        host.path = [[NSPathControl alloc] initWithFrame:NSMakeRect(20, 120, 200, 26)];
        host.path.pathStyle = NSPathStylePopUp;
        host.path.URL = [NSURL fileURLWithPath:@"/System/Library"];
        host.path.target = host;
        host.path.action = @selector(pathChosen:);
        [host.controls.contentView addSubview:host.path];
        host.combo = [[NSComboBox alloc] initWithFrame:NSMakeRect(240, 120, 200, 26)];
        [host.combo addItemsWithObjectValues:@[ @"One", @"Two", @"Three" ]];
        [host.controls.contentView addSubview:host.combo];
        host.tokens = [[NSTokenField alloc] initWithFrame:NSMakeRect(20, 70, 300, 26)];
        host.tokens.delegate = host;
        host.tokens.objectValue = @[ @"token" ];
        [host.controls.contentView addSubview:host.tokens];
        [host.controls orderFront:nil];
        host.status = [NSStatusBar.systemStatusBar statusItemWithLength:NSVariableStatusItemLength];
        host.status.button.title = @"devhost";
        host.status.menu = [host smallMenu:@"Status"];

        [host.window makeKeyAndOrderFront:nil];
        [host.window makeFirstResponder:host.text];
        [NSApp activateIgnoringOtherApps:YES];

        NSMutableArray *scenarios = [NSMutableArray array];
        for (int i = 3; i < argc; i++) [scenarios addObject:@(argv[i])];
        if (scenarios.count == 0)
            [scenarios addObjectsFromArray:@[ @"context", @"submenu", @"positioning", @"escape", @"popup", @"pulldown", @"textview", @"cancel", @"menubar", @"menubar-click", @"menubar-monitor", @"menubar-stray", @"customview",
                                                        @"apple", @"apple-click", @"menubar-highlight", @"statusitem", @"searchfield", @"segmented",
                                                        @"pathcontrol", @"tokenfield", @"toolbar", @"rightclick", @"palette", @"services", @"inactive" ]];
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
