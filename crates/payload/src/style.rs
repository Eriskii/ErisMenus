//! The look: ErisDock's black body and white text, and the fonts, colors and symbols for it.

use alloc::boxed::Box;
use core::ffi::CStr;
use core::sync::atomic::{AtomicPtr, Ordering};

use crate::msg;
use crate::objc::{Id, NIL, class, ns, nsstring, retain};
use crate::sys::{CGColorRef, CGSize, NSRange, gray};

pub struct Palette {
    pub body: CGColorRef,
    pub shadow: CGColorRef,
    pub highlight: CGColorRef,
    pub separator: CGColorRef,
    /// For tinting template images.
    pub glyph: CGColorRef,
    pub dim_glyph: CGColorRef,
    pub text: Id,
    pub disabled_text: Id,
    pub secondary_text: Id,
}

static PALETTE: AtomicPtr<Palette> = AtomicPtr::new(core::ptr::null_mut());

/// Made once, on the main thread, the first time a menu opens.
pub fn palette() -> &'static Palette {
    let p = PALETTE.load(Ordering::Acquire);
    if !p.is_null() {
        return unsafe { &*p };
    }
    let palette = Box::leak(Box::new(Palette {
        body: gray(0.0, 1.0),
        shadow: gray(0.0, 1.0),
        highlight: gray(1.0, 0.16),
        separator: gray(1.0, 0.12),
        glyph: gray(1.0, 1.0),
        dim_glyph: gray(1.0, 0.45),
        text: nscolor(1.0, 1.0),
        disabled_text: nscolor(1.0, 0.32),
        secondary_text: nscolor(1.0, 0.5),
    }));
    PALETTE.store(palette, Ordering::Release);
    palette
}

fn nscolor(white: f64, alpha: f64) -> Id {
    retain(msg!(Id; class(c"NSColor"), c"colorWithSRGBRed:green:blue:alpha:", white => f64, white => f64, white => f64, alpha => f64))
}

/// The font a menu asked for, or the menu font.
pub fn menu_font(menu: Id) -> Id {
    let font = msg!(Id; menu, c"font");
    if !font.is_null() {
        return font;
    }
    msg!(Id; class(c"NSFont"), c"menuFontOfSize:", 0.0 => f64)
}

pub fn system_font(size: f64, weight: f64) -> Id {
    msg!(Id; class(c"NSFont"), c"systemFontOfSize:weight:", size => f64, weight => f64)
}

pub const SEMIBOLD: f64 = 0.3;

/// The height a line of `font` takes, which is what a text layer draws into.
pub fn line_height(font: Id) -> f64 {
    let ascender = msg!(f64; font, c"ascender");
    let descender = msg!(f64; font, c"descender");
    let leading = msg!(f64; font, c"leading");
    menus_core::math::ceil(ascender - descender + leading)
}

/// A +1 attributed string of `text` in `font` and `color`.
pub fn attributed(text: Id, font: Id, color: Id) -> Id {
    let keys = [nsstring(c"NSFont"), nsstring(c"NSColor")];
    let values = [font, color];
    let attributes = msg!(Id; class(c"NSDictionary"), c"dictionaryWithObjects:forKeys:count:",
        values.as_ptr() => *const Id, keys.as_ptr() => *const Id, 2usize => usize);
    let text = if text.is_null() { ns("") } else { text };
    let string = msg!(Id; class(c"NSAttributedString"), c"alloc");
    msg!(Id; string, c"initWithString:attributes:", text => Id, attributes => Id)
}

/// A +1 copy of `string` all in `color`, keeping its fonts; `font` where it has none.
pub fn recolored(string: Id, font: Id, color: Id) -> Id {
    let copy = msg!(Id; string, c"mutableCopy");
    let range = NSRange { location: 0, length: msg!(usize; copy, c"length") };
    msg!((); copy, c"addAttribute:value:range:", nsstring(c"NSColor") => Id, color => Id, range => NSRange);
    let no_range = core::ptr::null_mut::<NSRange>();
    if range.length > 0
        && msg!(Id; copy, c"attribute:atIndex:effectiveRange:", nsstring(c"NSFont") => Id, 0usize => usize, no_range => *mut NSRange)
            .is_null()
    {
        msg!((); copy, c"addAttribute:value:range:", nsstring(c"NSFont") => Id, font => Id, range => NSRange);
    }
    copy
}

pub fn text_size(string: Id) -> CGSize {
    msg!(CGSize; string, c"size")
}

/// An SF Symbol at `size` points, or nil if this system lacks it.
pub fn symbol(name: &CStr, size: f64, weight: f64) -> Id {
    let image = msg!(Id; class(c"NSImage"), c"imageWithSystemSymbolName:accessibilityDescription:", nsstring(name) => Id, NIL => Id);
    if image.is_null() {
        return NIL;
    }
    let config = msg!(Id; class(c"NSImageSymbolConfiguration"), c"configurationWithPointSize:weight:", size => f64, weight => f64);
    msg!(Id; image, c"imageWithSymbolConfiguration:", config => Id)
}
