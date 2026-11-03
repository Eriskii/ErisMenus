#import <AppKit/AppKit.h>
extern NSInteger NSColorGetUserAccentColor(void);
void hooks_install(void) {
    NSUserDefaults *d = NSUserDefaults.standardUserDefaults;
    NSMutableDictionary *arg = [[d volatileDomainForName:NSArgumentDomain] mutableCopy] ?: [NSMutableDictionary dictionary];
    arg[@"AppleAccentColor"] = @(-2);
    [d setVolatileDomain:arg forName:NSArgumentDomain];
    NSColor *c = [NSColor colorWithSRGBRed:1.0 green:0x6B/255.0 blue:0 alpha:1];
    [NSApp performSelector:NSSelectorFromString(@"_setAccentColor:") withObject:c];
    id proxy = [NSClassFromString(@"NSSystemAppearanceProxy") performSelector:NSSelectorFromString(@"systemProxy")];
    [proxy performSelector:NSSelectorFromString(@"_controlTintChanged:") withObject:nil];
    NSColor *a = [NSColor.controlAccentColor colorUsingColorSpace:NSColorSpace.sRGBColorSpace];
    fprintf(stderr, "index %ld controlAccent %.3f %.3f %.3f\n", NSColorGetUserAccentColor(), a.redComponent, a.greenComponent, a.blueComponent);
}
