#import <AppKit/AppKit.h>
#import <objc/runtime.h>
// h5 + WebKit: pretend the main bundle declares NSAccentColorName so WebKit's UI process
// forwards NSApp._effectiveAccentColor to the WebContent process and WebCore uses it.
extern void hooks_install_h5(void);
static IMP origNamed;
static NSColor *namedHook(id self, SEL _cmd, NSString *name, NSBundle *bundle) {
    if ([name isEqualToString:@"ErisAccent"]) return [NSApp performSelector:NSSelectorFromString(@"_effectiveAccentColor")];
    return ((NSColor *(*)(id, SEL, id, id))origNamed)(self, _cmd, name, bundle);
}
void hooks_install(void) {
    hooks_install_h5();
    NSDictionary *info = NSBundle.mainBundle.infoDictionary;
    fprintf(stderr, "infoDictionary class %s mutable=%d\n", object_getClassName(info), [info isKindOfClass:NSMutableDictionary.class]);
    if (!info[@"NSAccentColorName"] && [info isKindOfClass:NSMutableDictionary.class]) ((NSMutableDictionary *)info)[@"NSAccentColorName"] = @"ErisAccent";
    origNamed = method_setImplementation(class_getClassMethod(NSColor.class, @selector(colorNamed:bundle:)), (IMP)namedHook);
    fprintf(stderr, "NSAccentColorName now %s\n", [NSBundle.mainBundle.infoDictionary[@"NSAccentColorName"] UTF8String]);
}
