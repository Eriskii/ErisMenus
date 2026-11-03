#import <AppKit/AppKit.h>
// dynamic variant: different orange for light vs dark, mimicking how presets vary per appearance
extern NSInteger NSColorGetUserAccentColor(void);
void hooks_install(void) {
    NSUserDefaults *d = NSUserDefaults.standardUserDefaults;
    NSMutableDictionary *arg = [[d volatileDomainForName:NSArgumentDomain] mutableCopy] ?: [NSMutableDictionary dictionary];
    arg[@"AppleAccentColor"] = @(-2);
    [d setVolatileDomain:arg forName:NSArgumentDomain];
    NSColor *c = [NSColor colorWithName:@"ErisAccent" dynamicProvider:^NSColor *(NSAppearance *ap) {
        BOOL dark = [[ap bestMatchFromAppearancesWithNames:@[NSAppearanceNameAqua, NSAppearanceNameDarkAqua]] isEqual:NSAppearanceNameDarkAqua];
        return dark ? [NSColor colorWithSRGBRed:1.0 green:0x6B/255.0 blue:0 alpha:1] : [NSColor colorWithSRGBRed:0.0 green:0.6 blue:0.5 alpha:1];
    }];
    [NSApp performSelector:NSSelectorFromString(@"_setAccentColor:") withObject:c];
    [[NSClassFromString(@"NSSystemAppearanceProxy") performSelector:NSSelectorFromString(@"systemProxy")] performSelector:NSSelectorFromString(@"_controlTintChanged:") withObject:nil];
}
