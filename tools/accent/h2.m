#import <AppKit/AppKit.h>
extern NSInteger NSColorGetUserAccentColor(void);
void hooks_install(void) {
    NSUserDefaults *d = NSUserDefaults.standardUserDefaults;
    fprintf(stderr, "before: %ld; arg domain=%s\n", NSColorGetUserAccentColor(), [[d volatileDomainForName:NSArgumentDomain] description].UTF8String);
    NSMutableDictionary *arg = [[d volatileDomainForName:NSArgumentDomain] mutableCopy] ?: [NSMutableDictionary dictionary];
    arg[@"AppleAccentColor"] = @(-2);
    @try { [d setVolatileDomain:arg forName:NSArgumentDomain]; } @catch (id e) { fprintf(stderr, "exc %s\n", [e description].UTF8String); }
    fprintf(stderr, "objectForKey now: %s; cached index: %ld\n", [[d objectForKey:@"AppleAccentColor"] description].UTF8String, NSColorGetUserAccentColor());
    for (NSString *n in @[@"AppleColorPreferencesChangedNotification", @"NSSystemColorPreferencesChangedNotification", NSUserDefaultsDidChangeNotification]) {
        [NSNotificationCenter.defaultCenter postNotificationName:n object:nil];
        fprintf(stderr, "after local %s: %ld\n", n.UTF8String, NSColorGetUserAccentColor());
    }
}
