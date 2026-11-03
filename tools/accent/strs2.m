#import <Foundation/Foundation.h>
#import <mach-o/dyld.h>
#import <mach-o/getsect.h>
#import <dlfcn.h>
int main(int argc, char **argv) { @autoreleasepool {
    dlopen(argv[1], RTLD_NOW);
    for (uint32_t i = 0; i < _dyld_image_count(); i++) {
        const char *name = _dyld_get_image_name(i); if (!strstr(name, argv[2])) continue;
        const struct mach_header_64 *h = (void *)_dyld_get_image_header(i);
        unsigned long sz; uint8_t *p = getsectiondata(h, "__TEXT", "__cstring", &sz); if (!p) continue;
        for (unsigned long o = 0; o < sz; ) { const char *s = (const char *)p + o; size_t l = strnlen(s, sz - o); if (strcasestr(s, argv[3])) printf("%s: %s\n", strrchr(name,'/')+1, s); o += l + 1; }
    }
}}
