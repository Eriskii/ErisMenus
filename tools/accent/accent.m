// Prototype payload: arbitrary system accent color for one AppKit/SwiftUI process.
// Build: clang -fobjc-arc -framework AppKit -dynamiclib accent.m -o accent.dylib
// Use:   ERIS_ACCENT=FF6B00 DYLD_INSERT_LIBRARIES=accent.dylib ./board out.png   (board calls hooks_install)
// Everything here is ObjC-level: no text patching, no C-function hooks.
#import <AppKit/AppKit.h>
#import <objc/runtime.h>
#import <objc/message.h>

static NSColor *gAccent;
static IMP gOrigResolvedIconTint;

// -[SLSIconAppearanceConfiguration resolvedIconTintColor] (AppKit category). With iconTintColorName == 0
// ("follow accent") it returns nil when the accent index is -2 (multicolor), so folders would go blue.
static id resolvedIconTintHook(id self, SEL _cmd) {
    if (((NSInteger (*)(id, SEL))objc_msgSend)(self, sel_registerName("iconTintColorName")) == 0) return gAccent;
    return ((id (*)(id, SEL))gOrigResolvedIconTint)(self, _cmd);
}

static void apply(void) {
    // 1. Make NSColorGetUserAccentColor() see -2 (Multicolor) in this process only. It reads
    //    [[NSUserDefaults standardUserDefaults] objectForKey:@"AppleAccentColor"]; the argument domain
    //    outranks the global domain and is never written to disk. CFPreferences sees it too.
    NSUserDefaults *d = NSUserDefaults.standardUserDefaults;
    NSMutableDictionary *arg = [[d volatileDomainForName:NSArgumentDomain] mutableCopy] ?: [NSMutableDictionary dictionary];
    arg[@"AppleAccentColor"] = @(-2);
    [d setVolatileDomain:arg forName:NSArgumentDomain];

    // 2. Folder-icon tint follows the accent too; keep it on ours instead of multicolor blue.
    if (!gOrigResolvedIconTint) {
        Method m = class_getInstanceMethod(objc_getClass("SLSIconAppearanceConfiguration"), sel_registerName("resolvedIconTintColor"));
        if (m) gOrigResolvedIconTint = method_setImplementation(m, (IMP)resolvedIconTintHook);
    }

    // 3. The app accent color (what an Info.plist NSAccentColorName gives an app in Multicolor mode,
    //    and what AppKit's own debug "Custom Accent Color" panel sets). Main thread only.
    ((void (*)(id, SEL, id))objc_msgSend)(NSApp, sel_registerName("_setAccentColor:"), gAccent);

    // 4. Drop AppKit's cached accent index (globalCachedAccentColor), recache NSDynamicSystemColors,
    //    refresh system appearances + icon appearance, post NSControlTintDidChangeNotification.
    //    Same path AppKit runs when the user changes the accent in System Settings.
    id proxy = ((id (*)(id, SEL))objc_msgSend)((id)objc_getClass("NSSystemAppearanceProxy"), sel_registerName("systemProxy"));
    ((void (*)(id, SEL, id))objc_msgSend)(proxy, sel_registerName("_controlTintChanged:"), nil);
}

void hooks_install(void) {
    const char *hex = getenv("ERIS_ACCENT") ?: "FF6B00";
    unsigned v = (unsigned)strtoul(hex, NULL, 16);
    gAccent = [NSColor colorWithSRGBRed:((v >> 16) & 255) / 255.0 green:((v >> 8) & 255) / 255.0 blue:(v & 255) / 255.0 alpha:1];
    if (NSThread.isMainThread) apply(); else dispatch_async(dispatch_get_main_queue(), ^{ apply(); });
}
