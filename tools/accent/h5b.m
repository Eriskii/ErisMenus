#import <AppKit/AppKit.h>
#import <objc/runtime.h>
#import <objc/message.h>
static NSColor *accent;
static IMP origResolved;
static id resolvedHook(id self, SEL _cmd) {
    // iconTintColorName 0 = "follow accent color"; theme numbers left alone
    if (((NSInteger (*)(id, SEL))objc_msgSend)(self, sel_registerName("iconTintColorName")) == 0) return accent;
    return ((id (*)(id, SEL))origResolved)(self, _cmd);
}
void hooks_install_h5(void) {
    NSUserDefaults *d = NSUserDefaults.standardUserDefaults;
    NSMutableDictionary *arg = [[d volatileDomainForName:NSArgumentDomain] mutableCopy] ?: [NSMutableDictionary dictionary];
    arg[@"AppleAccentColor"] = @(-2);
    [d setVolatileDomain:arg forName:NSArgumentDomain];
    accent = [NSColor colorWithSRGBRed:1.0 green:0x6B/255.0 blue:0 alpha:1];
    Method m = class_getInstanceMethod(NSClassFromString(@"SLSIconAppearanceConfiguration"), sel_registerName("resolvedIconTintColor"));
    origResolved = method_setImplementation(m, (IMP)resolvedHook);
    [NSApp performSelector:NSSelectorFromString(@"_setAccentColor:") withObject:accent];
    [[NSClassFromString(@"NSSystemAppearanceProxy") performSelector:NSSelectorFromString(@"systemProxy")] performSelector:NSSelectorFromString(@"_controlTintChanged:") withObject:nil];
}
