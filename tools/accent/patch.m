#import <AppKit/AppKit.h>
#import <mach/mach.h>
#import <ptrauth.h>
#import <libkern/OSCacheControl.h>
#import <dlfcn.h>
extern NSInteger NSColorGetUserAccentColor(void);
int main() { @autoreleasepool {
    [NSApplication sharedApplication];
    printf("before %ld\n", NSColorGetUserAccentColor());
    uint8_t *fn = ptrauth_strip(dlsym(RTLD_DEFAULT, "NSColorGetUserAccentColor"), ptrauth_key_function_pointer);
    vm_address_t page = (vm_address_t)fn & ~(vm_page_size - 1);
    kern_return_t kr = vm_protect(mach_task_self(), page, vm_page_size, FALSE, VM_PROT_READ | VM_PROT_WRITE | VM_PROT_COPY);
    printf("vm_protect RW|COPY: %d (%s)\n", kr, mach_error_string(kr)); fflush(stdout);
    if (kr) return 1;
    uint32_t code[] = { 0x92800020 /* movn x0,#1 => -2 */, 0xd65f03c0 /* ret */ };
    memcpy(fn, code, sizeof code);
    kr = vm_protect(mach_task_self(), page, vm_page_size, FALSE, VM_PROT_READ | VM_PROT_EXECUTE);
    printf("vm_protect RX: %d (%s)\n", kr, mach_error_string(kr)); fflush(stdout);
    sys_icache_invalidate(fn, sizeof code);
    printf("after %ld\n", NSColorGetUserAccentColor());
}}
