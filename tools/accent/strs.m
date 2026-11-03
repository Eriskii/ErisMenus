#import <AppKit/AppKit.h>
#import <mach-o/dyld.h>
#import <mach-o/getsect.h>
#import <dlfcn.h>
int main(int argc, char **argv) { @autoreleasepool {
    [NSApplication sharedApplication];
    dlopen("/System/Library/Frameworks/SwiftUI.framework/SwiftUI", RTLD_NOW);
    for (uint32_t i = 0; i < _dyld_image_count(); i++) {
        const char *name = _dyld_get_image_name(i);
        if (argc > 2 && !strstr(name, argv[2])) continue;
        const struct mach_header_64 *h = (void *)_dyld_get_image_header(i);
        const char *secs[] = {"__cstring", "__oslogstring", "__swift5_reflstr"};
        for (int s = 0; s < 3; s++) {
        unsigned long sz; uint8_t *p = getsectiondata(h, "__TEXT", secs[s], &sz);
        if (!p) continue;
        for (unsigned long o = 0; o < sz; ) {
            const char *str = (const char *)p + o; size_t l = strnlen(str, sz - o);
            if (strcasestr(str, argv[1])) printf("%s: %s\n", strrchr(name, '/') + 1, str);
            o += l + 1;
        }}
    }
}}
