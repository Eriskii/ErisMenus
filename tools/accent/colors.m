#import <AppKit/AppKit.h>
#import <dlfcn.h>
extern NSInteger NSColorGetUserAccentColor(void);
static void dump(const char *tag) {
    NSArray *names = @[@"controlAccentColor", @"selectedContentBackgroundColor", @"keyboardFocusIndicatorColor", @"selectedControlColor", @"alternateSelectedControlColor", @"selectedMenuItemColor", @"selectedTextBackgroundColor", @"unemphasizedSelectedContentBackgroundColor", @"linkColor", @"findHighlightColor", @"systemBlueColor"];
    for (NSString *ap in @[NSAppearanceNameAqua, NSAppearanceNameDarkAqua]) {
        [[NSAppearance appearanceNamed:ap] performAsCurrentDrawingAppearance:^{
            printf("%s %s:", tag, ap.UTF8String + 16);
            for (NSString *n in names) { NSColor *c = [[NSColor performSelector:NSSelectorFromString(n)] colorUsingColorSpace:NSColorSpace.sRGBColorSpace];
                printf(" %s=%02x%02x%02x%s", n.UTF8String, (int)lround(c.redComponent*255), (int)lround(c.greenComponent*255), (int)lround(c.blueComponent*255), c.alphaComponent < 1 ? [NSString stringWithFormat:@"@%.2f", c.alphaComponent].UTF8String : ""); }
            printf("\n"); }];
    }
}
int main() { @autoreleasepool {
    [NSApplication sharedApplication];
    dump("stock");
    void (*h)(void) = dlsym(RTLD_DEFAULT, "hooks_install"); if (h) { h(); dump("hooked"); }
    id cf = CFBridgingRelease(CFPreferencesCopyAppValue(CFSTR("AppleAccentColor"), kCFPreferencesCurrentApplication));
    printf("CFPreferencesCopyAppValue(AppleAccentColor, current app) = %s\n", [cf description].UTF8String);
}}
