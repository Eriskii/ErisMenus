#import <AppKit/AppKit.h>
#import <objc/runtime.h>
#import <dlfcn.h>
static void props(id o) {
    printf("%s: %s\n", class_getName([o class]), [[o description] UTF8String]);
    for (Class c = [o class]; c && c != NSObject.class; c = class_getSuperclass(c)) {
        unsigned n; objc_property_t *p = class_copyPropertyList(c, &n);
        for (unsigned i = 0; i < n; i++) { const char *nm = property_getName(p[i]); @try { printf("  %s.%s = %s\n", class_getName(c), nm, [[[o valueForKey:@(nm)] description] UTF8String]); } @catch (id e) { printf("  %s.%s (exc)\n", class_getName(c), nm); } }
        unsigned m; Method *ms = class_copyMethodList(c, &m); printf("  methods:"); for (unsigned i = 0; i < m; i++) printf(" %s", sel_getName(method_getName(ms[i]))); printf("\n");
    }
}
int main() { @autoreleasepool {
    [NSApplication sharedApplication];
    void (*h)(void) = dlsym(RTLD_DEFAULT, "hooks_install"); if (h) h();
    id cfg = [NSWorkspace.sharedWorkspace performSelector:@selector(currentIconAppearanceConfiguration)];
    props(cfg);
    id sls = [NSClassFromString(@"SLSIconAppearanceConfiguration") performSelector:@selector(fetchCurrentIconAppearanceConfiguration)];
    props(sls);
}}
