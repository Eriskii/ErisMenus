//! Any color as the system accent (see `docs/accent.md`).
//!
//! The accent picked in System Settings is an index into Apple's presets, which AppKit turns into
//! a catalog name CoreUI draws from. In Multicolor mode (index -2) AppKit takes an arbitrary color
//! instead, the app's own. So, in this process only:
//!
//! 1. `AppleAccentColor` = -2 in the argument domain, which `NSColorGetUserAccentColor` reads,
//!    outranks the global setting and is never written anywhere, then
//!    `-[NSSystemAppearanceProxy _controlTintChanged:]`, what a change in System Settings runs:
//!    AppKit drops its cached accent, recaches its dynamic colors, refreshes appearances and icons
//!    and tells the app (`NSControlTintDidChangeNotification`);
//! 2. `-[NSApplication _setAccentColor:]` with our color, which re-themes the app only once
//!    AppKit's cached accent is Multicolor (hence 1 first);
//! 3. `_controlTintChanged:` again.
//!
//! Folders follow the accent through `-[SLSIconAppearanceConfiguration resolvedIconTintColor]`,
//! which in Multicolor mode answers nil (blue) when icons follow the accent; it answers the icon
//! tint instead (its own color, or the accent). `erismenus` also gives macOS's own icon tint that
//! color (its "other" tint), so this is the fallback, for while that's set to follow the accent.
//!
//! `+[NSColor controlAccentColor]` answers our color too. AppKit's controls are SwiftUI views
//! (`_NSCoreHostingView<AppKitButton>`); AppKit hands them the accent as
//! `EnvironmentValues._defaultAccentColor = Color(NSColor.controlAccentColor)` next to
//! `_systemAccentValue` (the index). Both stay equal from one color of ours to the next (the same
//! dynamic color object, index -2), so SwiftUI keeps the controls it drew in the first color.
//! Our own color object per color makes the environment change. Resolved, it's the same color the
//! dynamic one gives in every appearance while ours is in force.

use core::sync::atomic::{AtomicPtr, AtomicU64, Ordering};

use crate::objc::{self, Id, Imp, NIL, Original, Sel, class, nsstring, release, retain, swizzle, swizzle_class};
use crate::{log, msg};

/// Our accent, retained; nil when the user's own accent is in force.
static COLOR: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(core::ptr::null_mut());
/// What's applied: `NONE`, or 0xRRGGBB.
static APPLIED: AtomicU64 = AtomicU64::new(NONE);
const NONE: u64 = u64::MAX;
/// What the argument domain held for `AppleAccentColor` before us (retained, maybe nil), to put
/// back when the accent goes back to the user's.
static BEFORE: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(core::ptr::null_mut());
static ICON_TINT: Original = Original::new();
/// The icon tint (retained), while there's one of ours.
static ICON_COLOR: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(core::ptr::null_mut());
/// What `ICON_COLOR` is: `NONE`, or 0xRRGGBB.
static ICON_APPLIED: AtomicU64 = AtomicU64::new(NONE);
static CONTROL_ACCENT: Original = Original::new();

const ARGUMENT_DOMAIN: &core::ffi::CStr = c"NSArgumentDomain";
const KEY: &core::ffi::CStr = c"AppleAccentColor";
const MULTICOLOR: isize = -2;

/// Makes `rgb` (0xRRGGBB) the accent in this app, or puts the user's back; whether anything
/// changed. Main thread.
pub fn apply(rgb: Option<u32>) -> bool {
    let wanted = rgb.map_or(NONE, u64::from);
    if APPLIED.load(Ordering::Relaxed) == wanted {
        return false;
    }
    let proxy_class = class(c"NSSystemAppearanceProxy");
    let app = msg!(Id; class(c"NSApplication"), c"sharedApplication");
    if proxy_class.is_null() || app.is_null() || !objc::responds(app, c"_setAccentColor:") {
        log!("no way to set the accent here; it stays the one in System Settings");
        return false;
    }
    let previous = APPLIED.swap(wanted, Ordering::Relaxed);
    match rgb {
        Some(rgb) => {
            let (r, g, b) = menus_core::settings::components(rgb);
            let color = retain(msg!(Id; class(c"NSColor"), c"colorWithSRGBRed:green:blue:alpha:",
                r => f64, g => f64, b => f64, 1.0 => f64));
            let old = COLOR.swap(color, Ordering::AcqRel);
            if CONTROL_ACCENT.get().is_null() {
                swizzle_class(c"NSColor", c"controlAccentColor", control_accent as Imp, &CONTROL_ACCENT);
            }
            if previous == NONE {
                // Multicolor first, and AppKit told: `_setAccentColor:` only re-themes the app
                // (KVO on `effectiveAppearance`, window appearances invalidated, which is what
                // makes the SwiftUI-backed controls rebuild their environment) when the accent
                // it has cached is already Multicolor; otherwise it just stores the color.
                multicolor(true);
                tint_changed(proxy_class);
            }
            msg!((); app, c"_setAccentColor:", color => Id);
            if !old.is_null() {
                release(old);
            }
        }
        None => {
            let old = COLOR.swap(NIL, Ordering::AcqRel);
            msg!((); app, c"_setAccentColor:", NIL => Id);
            multicolor(false);
            if !old.is_null() {
                release(old);
            }
        }
    }
    tint_changed(proxy_class);
    log!("accent {}", if rgb.is_some() { "set" } else { "back to the system's" });
    true
}

/// Icons redrawn, for an icon tint change on its own: AppKit's accent refresh only redraws them
/// when the accent changes, and they follow the icon appearance notification.
pub fn refresh() {
    let proxy_class = class(c"NSSystemAppearanceProxy");
    if !proxy_class.is_null() {
        tint_changed(proxy_class);
    }
    let workspace = msg!(Id; class(c"NSWorkspace"), c"sharedWorkspace");
    let center = msg!(Id; workspace, c"notificationCenter");
    msg!((); center, c"postNotificationName:object:", nsstring(c"NSWorkspaceIconAppearanceConfigurationDidChangeNotification") => Id, workspace => Id);
}

/// What a change in System Settings runs: AppKit forgets its accent and takes it in again.
fn tint_changed(proxy_class: Id) {
    let proxy = msg!(Id; proxy_class, c"systemProxy");
    if !proxy.is_null() && objc::responds(proxy, c"_controlTintChanged:") {
        msg!((); proxy, c"_controlTintChanged:", NIL => Id);
    }
}

/// Multicolor in this process's argument domain, or what was there before.
fn multicolor(on: bool) {
    let defaults = msg!(Id; class(c"NSUserDefaults"), c"standardUserDefaults");
    let domain = msg!(Id; defaults, c"volatileDomainForName:", nsstring(ARGUMENT_DOMAIN) => Id);
    let arguments = if domain.is_null() {
        msg!(Id; class(c"NSMutableDictionary"), c"dictionary")
    } else {
        let copy = msg!(Id; domain, c"mutableCopy");
        msg!((); copy, c"autorelease");
        copy
    };
    let key = nsstring(KEY);
    if on {
        let before = msg!(Id; arguments, c"objectForKey:", key => Id);
        let stale = BEFORE.swap(if before.is_null() { NIL } else { retain(before) }, Ordering::AcqRel);
        if !stale.is_null() {
            release(stale);
        }
        let value = msg!(Id; class(c"NSNumber"), c"numberWithInteger:", MULTICOLOR => isize);
        msg!((); arguments, c"setObject:forKey:", value => Id, key => Id);
    } else {
        let before = BEFORE.swap(NIL, Ordering::AcqRel);
        if before.is_null() {
            msg!((); arguments, c"removeObjectForKey:", key => Id);
        } else {
            msg!((); arguments, c"setObject:forKey:", before => Id, key => Id);
            release(before);
        }
    }
    msg!((); defaults, c"setVolatileDomain:forName:", arguments => Id, nsstring(ARGUMENT_DOMAIN) => Id);
}

/// The icon tint, 0xRRGGBB (its own color or the accent's), or none of ours; whether it changed.
/// Main thread.
pub fn set_icon_tint(rgb: Option<u32>) -> bool {
    let wanted = rgb.map_or(NONE, u64::from);
    if ICON_APPLIED.swap(wanted, Ordering::Relaxed) == wanted {
        return false;
    }
    let color = match rgb {
        Some(rgb) => {
            let (r, g, b) = menus_core::settings::components(rgb);
            retain(msg!(Id; class(c"NSColor"), c"colorWithSRGBRed:green:blue:alpha:", r => f64, g => f64, b => f64, 1.0 => f64))
        }
        None => NIL,
    };
    // Never released: a getter's caller may still hold the old one (see `autoreleased`).
    let _ = ICON_COLOR.swap(color, Ordering::AcqRel);
    if !color.is_null() && ICON_TINT.get().is_null() {
        swizzle(c"SLSIconAppearanceConfiguration", c"resolvedIconTintColor", icon_tint as Imp, &ICON_TINT);
    }
    true
}

/// `-[SLSIconAppearanceConfiguration resolvedIconTintColor]`: icons set to follow the accent
/// (`iconTintColorName` 0) take the icon tint.
extern "C" fn icon_tint(this: Id, cmd: Sel) -> Id {
    let color = ICON_COLOR.load(Ordering::Acquire);
    if !color.is_null() && objc::responds(this, c"iconTintColorName") && msg!(isize; this, c"iconTintColorName") == 0 {
        return autoreleased(color);
    }
    let original: extern "C" fn(Id, Sel) -> Id = unsafe { core::mem::transmute(ICON_TINT.get()) };
    original(this, cmd)
}

/// `+[NSColor controlAccentColor]`: ours while one is set (see the top of the file).
extern "C" fn control_accent(this: Id, cmd: Sel) -> Id {
    let color = COLOR.load(Ordering::Acquire);
    if !color.is_null() {
        return autoreleased(color);
    }
    let original: extern "C" fn(Id, Sel) -> Id = unsafe { core::mem::transmute(CONTROL_ACCENT.get()) };
    original(this, cmd)
}

/// `color` as a getter should hand it out, so a caller keeps it alive across the next change of
/// color (which releases ours).
fn autoreleased(color: Id) -> Id {
    msg!(Id; retain(color), c"autorelease")
}

/// The accent as a `CGColor` for our own drawing, while one is set.
pub fn cg_color() -> Option<crate::sys::CGColorRef> {
    let color = COLOR.load(Ordering::Acquire);
    (!color.is_null()).then(|| msg!(crate::sys::CGColorRef; color, c"CGColor"))
}
