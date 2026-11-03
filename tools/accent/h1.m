#import <AppKit/AppKit.h>
void hooks_install(void) {
    NSColor *c = [NSColor colorWithSRGBRed:1.0 green:0x6B/255.0 blue:0 alpha:1];
    [NSApp performSelector:NSSelectorFromString(@"_setAccentColor:") withObject:c];
    NSColor *a = [NSColor.controlAccentColor colorUsingColorSpace:NSColorSpace.sRGBColorSpace];
    fprintf(stderr, "after _setAccentColor: controlAccent %.3f %.3f %.3f\n", a.redComponent, a.greenComponent, a.blueComponent);
}
