#import <AppKit/AppKit.h>
extern NSInteger NSColorGetUserAccentColor(void);
extern NSInteger NSColorDefaultUserAccentColor(void);
extern NSInteger NSColorEffectiveUserAccentColor(void);
extern void NSColorEnumerateUserAccentColors(void (^)(NSInteger));
extern NSString *NSColorLocalizedNameForUserAccentKey(NSInteger);
extern BOOL NSColorControlAccentIsMulticolor(void);
int main() { @autoreleasepool {
    [NSApplication sharedApplication];
    printf("get=%ld default=%ld effective=%ld multicolor=%d\n", NSColorGetUserAccentColor(), NSColorDefaultUserAccentColor(), NSColorEffectiveUserAccentColor(), NSColorControlAccentIsMulticolor());
    NSColorEnumerateUserAccentColors(^(NSInteger k) { printf("  %ld %s\n", k, NSColorLocalizedNameForUserAccentKey(k).UTF8String); });
    NSColor *a = [NSColor.controlAccentColor colorUsingColorSpace:NSColorSpace.sRGBColorSpace];
    printf("controlAccent %.3f %.3f %.3f\n", a.redComponent, a.greenComponent, a.blueComponent);
}}
