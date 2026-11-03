//! macOS's own icon tint, the color tinted app icons and folders take everywhere (the Dock, Finder,
//! widgets), set to ours. SkyLight's `SLSIconAppearanceConfiguration` has `iconTintColorName` 0 for
//! "follow the accent", 1 to 9 for Apple's presets, and 10 for "other": `otherIconTintColor`, any
//! color, which System Settings never offers. `-save` writes it to the user's global preferences
//! (`AppleIconAppearanceTintColor`, `AppleIconAppearanceCustomTintColor`) and tells every process.
//! Runs as the user, from `erismenus set` and the settings window.

use objc2::encode::{Encoding, RefEncode};
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{msg_send, sel};
use objc2_app_kit::NSWorkspace;
use objc2_foundation::NSObjectProtocol;

const FOLLOW_ACCENT: u32 = 0;
const OTHER: u32 = 10;

#[repr(C)]
struct CGColor {
    _private: [u8; 0],
}

// What objc2 checks message arguments against (`^{CGColor=}`).
unsafe impl RefEncode for CGColor {
    const ENCODING_REF: Encoding = Encoding::Pointer(&Encoding::Struct("CGColor", &[]));
}

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGColorCreateSRGB(red: f64, green: f64, blue: f64, alpha: f64) -> *const CGColor;
    fn CGColorRelease(color: *const CGColor);
    fn CGColorGetNumberOfComponents(color: *const CGColor) -> usize;
    fn CGColorGetComponents(color: *const CGColor) -> *const f64;
}

#[derive(Debug, PartialEq)]
enum Change {
    Tint(u32),
    FollowAccent,
}

/// What to change, given macOS's tint now (`name`, and the other color if it's set) and the one
/// wanted. Only a tint of ours (other) is ever taken back: a preset picked in System Settings,
/// or following the accent, is left alone when there's no tint of our own to give.
fn decide(name: u32, other: Option<u32>, wanted: Option<u32>) -> Option<Change> {
    match wanted {
        Some(rgb) if name == OTHER && other == Some(rgb) => None,
        Some(rgb) => Some(Change::Tint(rgb)),
        None if name == OTHER => Some(Change::FollowAccent),
        None => None,
    }
}

/// Gives macOS's icon tint `wanted` (0xRRGGBB), or, with `None`, takes back a tint we gave it.
pub fn apply(wanted: Option<u32>) -> Result<(), String> {
    // The class comes with AppKit's icon appearance category, loaded on first use.
    let workspace = NSWorkspace::sharedWorkspace();
    let sel = sel!(currentIconAppearanceConfiguration);
    if !workspace.respondsToSelector(sel) {
        return Ok(());
    }
    let _: Option<Retained<AnyObject>> = unsafe { msg_send![&*workspace, currentIconAppearanceConfiguration] };
    let Some(class) = AnyClass::get(c"SLSIconAppearanceConfiguration") else {
        return Ok(());
    };
    let current: Option<Retained<AnyObject>> = unsafe { msg_send![class, fetchCurrentIconAppearanceConfiguration] };
    let Some(current) = current else { return Ok(()) };
    let name: u32 = unsafe { msg_send![&*current, iconTintColorName] };
    let other: *const CGColor = unsafe { msg_send![&*current, otherIconTintColor] };
    let Some(change) = decide(name, rgb_of(other), wanted) else {
        return Ok(());
    };
    match change {
        Change::Tint(rgb) => {
            let (r, g, b) = menus_core::settings::components(rgb);
            let color = unsafe { CGColorCreateSRGB(r, g, b, 1.0) };
            unsafe {
                let _: () = msg_send![&*current, setOtherIconTintColor: color];
                let _: () = msg_send![&*current, setIconTintColorName: OTHER];
                CGColorRelease(color);
            }
        }
        Change::FollowAccent => unsafe {
            let _: () = msg_send![&*current, setIconTintColorName: FOLLOW_ACCENT];
        },
    }
    let _: () = unsafe { msg_send![&*current, save] };
    Ok(())
}

fn rgb_of(color: *const CGColor) -> Option<u32> {
    if color.is_null() || unsafe { CGColorGetNumberOfComponents(color) } < 3 {
        return None;
    }
    let c = unsafe { std::slice::from_raw_parts(CGColorGetComponents(color), 3) };
    let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    Some(byte(c[0]) << 16 | byte(c[1]) << 8 | byte(c[2]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gives_macos_our_tint() {
        assert_eq!(decide(FOLLOW_ACCENT, None, Some(0xFF6B00)), Some(Change::Tint(0xFF6B00)));
        assert_eq!(decide(6, None, Some(0xFF6B00)), Some(Change::Tint(0xFF6B00)));
        assert_eq!(decide(OTHER, Some(0x00A0A0), Some(0xFF6B00)), Some(Change::Tint(0xFF6B00)));
    }

    #[test]
    fn leaves_it_when_it_has_it_already() {
        assert_eq!(decide(OTHER, Some(0xFF6B00), Some(0xFF6B00)), None);
    }

    #[test]
    fn takes_back_only_its_own() {
        assert_eq!(decide(OTHER, Some(0xFF6B00), None), Some(Change::FollowAccent));
        // A preset from System Settings, or following the accent, stays.
        assert_eq!(decide(6, None, None), None);
        assert_eq!(decide(FOLLOW_ACCENT, None, None), None);
    }
}
