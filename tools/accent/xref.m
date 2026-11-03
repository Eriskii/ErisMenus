// usage: xref <image substring> <symbol or 0xaddr> : finds adrp+ldr/str/add referencing the target address within the image text
#import <Foundation/Foundation.h>
#import <AppKit/AppKit.h>
#import <mach-o/dyld.h>
#import <mach-o/getsect.h>
#import <dlfcn.h>
int main(int argc, char **argv) { @autoreleasepool {
    [NSApplication sharedApplication];
    dlopen("/System/Library/Frameworks/SwiftUI.framework/SwiftUI", RTLD_NOW);
    uint64_t target = strncmp(argv[2], "0x", 2) == 0 ? strtoull(argv[2], 0, 16) : (uint64_t)dlsym(RTLD_DEFAULT, argv[2]);
    if (argc > 3) target += strtoull(argv[3], 0, 16);
    printf("target %llx\n", target);
    for (uint32_t i = 0; i < _dyld_image_count(); i++) {
        if (!strstr(_dyld_get_image_name(i), argv[1])) continue;
        const struct mach_header_64 *h = (void *)_dyld_get_image_header(i);
        unsigned long sz; uint32_t *p = (uint32_t *)getsectiondata(h, "__TEXT", "__text", &sz);
        uint64_t page[32] = {0};
        for (unsigned long k = 0; k < sz / 4; k++) {
            uint32_t ins = p[k]; uint64_t pc = (uint64_t)&p[k];
            if ((ins & 0x9f000000) == 0x90000000) { // adrp
                int rd = ins & 31; int64_t imm = ((ins >> 29) & 3) | (((int64_t)((ins >> 5) & 0x7ffff)) << 2);
                if (imm & (1 << 20)) imm -= (1 << 21);
                page[rd] = (pc & ~0xfffULL) + (imm << 12); continue;
            }
            int rn = (ins >> 5) & 31; uint64_t addr = 0; const char *kind = 0;
            if ((ins & 0xff800000) == 0x91000000) { addr = page[rn] + ((ins >> 10) & 0xfff); kind = "add"; }
            else if ((ins & 0x3b000000) == 0x39000000) { int size = ins >> 30; addr = page[rn] + (((ins >> 10) & 0xfff) << size); kind = (ins & (1 << 22)) ? "ldr" : "str"; }
            if (kind && page[rn] && addr == target) { Dl_info d; dladdr((void *)pc, &d); printf("%s at %llx in %s+%lld\n", kind, pc, d.dli_sname, (long long)(pc - (uint64_t)d.dli_saddr)); }
        }
    }
}}
