#import <AppKit/AppKit.h>
#import <objc/runtime.h>
#import <dlfcn.h>
int main(int argc, char **argv) { @autoreleasepool {
    [NSApplication sharedApplication];
    dlopen("/System/Library/Frameworks/SwiftUI.framework/SwiftUI", RTLD_NOW); dlopen("/System/Library/Frameworks/WebKit.framework/WebKit", RTLD_NOW);
    int n = objc_getClassList(NULL, 0); Class *cl = (__unsafe_unretained Class *)malloc(sizeof(Class) * n); n = objc_getClassList(cl, n);
    for (int i = 0; i < n; i++) for (int meta = 0; meta < 2; meta++) {
        Class c = meta ? object_getClass(cl[i]) : cl[i]; unsigned k; Method *m = class_copyMethodList(c, &k);
        for (unsigned j = 0; j < k; j++) { const char *s = sel_getName(method_getName(m[j]));
            for (int a = 1; a < argc; a++) if (strcasestr(s, argv[a])) { Dl_info d; dladdr((void*)method_getImplementation(m[j]), &d); printf("%s %c[%s %s]\n", strrchr(d.dli_fname,'/')+1, meta?'+':'-', class_getName(cl[i]), s); break; } }
        free(m);
    }
}}
