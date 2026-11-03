#import <AppKit/AppKit.h>
#import <objc/runtime.h>
int main() { @autoreleasepool {
    [NSApplication sharedApplication];
    NSOpenPanel *p = [NSOpenPanel openPanel];
    printf("open panel class %s super %s\n", class_getName(p.class), class_getName(class_getSuperclass(p.class)));
}}
