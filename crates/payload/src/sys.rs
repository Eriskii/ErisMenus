//! CoreGraphics and QuartzCore bits: geometry structs, paths, colors, the clock; and the window
//! server's side of tracking a menu.

use core::ffi::c_void;

use crate::objc::{Id, nsstring};

use menus_core::shape::Element;
use menus_core::{Point, Rect};

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CGPoint {
    pub x: f64,
    pub y: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CGSize {
    pub width: f64,
    pub height: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CGRect {
    pub origin: CGPoint,
    pub size: CGSize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NSRange {
    pub location: usize,
    pub length: usize,
}

impl From<CGRect> for Rect {
    fn from(r: CGRect) -> Self {
        Rect::new(r.origin.x, r.origin.y, r.size.width, r.size.height)
    }
}

impl From<Rect> for CGRect {
    fn from(r: Rect) -> Self {
        CGRect { origin: CGPoint { x: r.x, y: r.y }, size: CGSize { width: r.width, height: r.height } }
    }
}

impl From<CGPoint> for Point {
    fn from(p: CGPoint) -> Self {
        Point::new(p.x, p.y)
    }
}

pub fn rect(x: f64, y: f64, width: f64, height: f64) -> CGRect {
    Rect::new(x, y, width, height).into()
}

pub type CGPathRef = *mut c_void;
pub type CGColorRef = *mut c_void;

#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGPathCreateMutable() -> CGPathRef;
    fn CGPathMoveToPoint(path: CGPathRef, m: *const c_void, x: f64, y: f64);
    fn CGPathAddLineToPoint(path: CGPathRef, m: *const c_void, x: f64, y: f64);
    fn CGPathAddCurveToPoint(path: CGPathRef, m: *const c_void, c1x: f64, c1y: f64, c2x: f64, c2y: f64, x: f64, y: f64);
    fn CGPathCloseSubpath(path: CGPathRef);
    pub fn CGPathRelease(path: CGPathRef);
    fn CGColorCreateSRGB(r: f64, g: f64, b: f64, a: f64) -> CGColorRef;
}

#[link(name = "QuartzCore", kind = "framework")]
unsafe extern "C" {
    fn CACurrentMediaTime() -> f64;
}

/// Seconds on the media clock.
pub fn now() -> f64 {
    unsafe { CACurrentMediaTime() }
}

/// A +1 CGPath of `elements`, moved by (dx, dy).
pub fn path(elements: &[Element], dx: f64, dy: f64) -> CGPathRef {
    let p = unsafe { CGPathCreateMutable() };
    let null = core::ptr::null();
    for e in elements {
        unsafe {
            match *e {
                Element::Move(a) => CGPathMoveToPoint(p, null, a.x + dx, a.y + dy),
                Element::Line(a) => CGPathAddLineToPoint(p, null, a.x + dx, a.y + dy),
                Element::Curve { control1: a, control2: b, end: c } => {
                    CGPathAddCurveToPoint(p, null, a.x + dx, a.y + dy, b.x + dx, b.y + dy, c.x + dx, c.y + dy)
                }
                Element::Close => CGPathCloseSubpath(p),
            }
        }
    }
    p
}

/// A +1 gray sRGB color; ours live as long as the app, so nothing releases them.
pub fn gray(white: f64, alpha: f64) -> CGColorRef {
    unsafe { CGColorCreateSRGB(white, white, white, alpha) }
}

unsafe extern "C" {
    fn dlsym(handle: *mut c_void, symbol: *const core::ffi::c_char) -> *mut c_void;
}

/// A function the app has loaded, by name; private ones aren't linked against.
fn function(name: &core::ffi::CStr) -> Option<*const c_void> {
    const RTLD_DEFAULT: *mut c_void = -2isize as *mut c_void;
    let f = unsafe { dlsym(RTLD_DEFAULT, name.as_ptr()) };
    // Signed in arm64e apps; we call it with a plain branch.
    (!f.is_null()).then(|| eris_inject_payload::pac::strip(f))
}

macro_rules! function {
    ($name:literal as $ty:ty) => {
        function($name).map(|f| unsafe { core::mem::transmute::<*const c_void, $ty>(f) })
    };
}

#[repr(C)]
struct ProcessSerialNumber {
    high: u32,
    low: u32,
}

/// The distributed notifications menus post and listen for (HIToolbox's, which AppKit keeps).
const BEGIN_TRACKING: &core::ffi::CStr = c"com.apple.HIToolbox.beginMenuTrackingNotification";
const END_TRACKING: &core::ffi::CStr = c"com.apple.HIToolbox.endMenuTrackingNotification";
const CANCEL_TRACKING: &core::ffi::CStr = c"com.apple.HIToolbox.cancelMenuTrackingNotification";

pub type NotificationCallback = extern "C" fn(*const c_void, *const c_void, *const c_void, *const c_void, *const c_void);

/// What AppKit sets up while a menu tracks (`-[NSMenuTrackingSession _setupMenuTracking]`), so a
/// menu works in an app that isn't frontmost, as a menu bar extra's or a context menu in a window
/// behind others is: the app gets mouse events from outside its windows (a click elsewhere closes
/// the menu) and the keyboard, other apps close their menus, and the system's request to close
/// every menu reaches `cancel`. Undone when dropped.
pub struct Tracking {
    connection: i32,
    saved_mask: Option<u32>,
    focus: Option<u32>,
    observing: bool,
}

impl Tracking {
    pub fn begin(cancel: NotificationCallback) -> Tracking {
        let mut tracking = Tracking { connection: 0, saved_mask: None, focus: None, observing: false };
        let Some(connection) = function!(c"CGSMainConnectionID" as extern "C" fn() -> i32) else { return tracking };
        tracking.connection = connection();
        // Mouse downs, ups, moves and drags, scrolling and other buttons, from anywhere.
        const BACKGROUND_EVENTS: u32 = 0x0240_00fe;
        if let (Some(request), Some(get), Some(set)) = (
            function!(c"_RequestBackgroundMouseMovedEvents" as extern "C" fn()),
            function!(c"CGSGetBackgroundEventMask" as extern "C" fn(i32, *mut u32) -> i32),
            function!(c"CGSSetBackgroundEventMask" as extern "C" fn(i32, u32) -> i32),
        ) {
            request();
            let mut mask = 0;
            if get(tracking.connection, &mut mask) == 0 {
                set(tracking.connection, mask | BACKGROUND_EVENTS);
                tracking.saved_mask = Some(mask);
            }
        }
        // The keyboard, unless we have it already.
        if let (Some(focused), Some(same_process), Some(steal)) = (
            function!(c"CPSGetKeyFocusProcess" as extern "C" fn(*mut ProcessSerialNumber, *mut u8) -> i32),
            function!(c"SameProcess" as extern "C" fn(*const ProcessSerialNumber, *const ProcessSerialNumber, *mut u8) -> i16),
            function!(c"CPSStealKeyFocusReturningID" as extern "C" fn(i32, u32, *mut u32) -> i32),
        ) {
            let mut focus = ProcessSerialNumber { high: 0, low: 0 };
            let mut unused = 0;
            const CURRENT_PROCESS: ProcessSerialNumber = ProcessSerialNumber { high: 0, low: 2 };
            let mut same = 1;
            if focused(&mut focus, &mut unused) == 0 && (same_process(&focus, &CURRENT_PROCESS, &mut same) != 0 || same == 0) {
                let mut id = 0;
                if steal(tracking.connection, 0xc80, &mut id) == 0 {
                    tracking.focus = Some(id);
                }
            }
        }
        if let (Some(center), Some(post), Some(observe)) = (
            function!(c"CFNotificationCenterGetDistributedCenter" as extern "C" fn() -> *const c_void),
            function!(c"CFNotificationCenterPostNotification" as extern "C" fn(*const c_void, Id, *const c_void, *const c_void, bool)),
            function!(c"CFNotificationCenterAddObserver" as extern "C" fn(*const c_void, *const c_void, *const c_void, Id, *const c_void, isize)),
        ) {
            const DELIVER_IMMEDIATELY: isize = 4;
            let signed = eris_inject_payload::pac::sign(cancel as *const c_void);
            observe(center(), observer(), signed, nsstring(CANCEL_TRACKING), core::ptr::null(), DELIVER_IMMEDIATELY);
            tracking.observing = true;
            post(center(), nsstring(BEGIN_TRACKING), core::ptr::null(), core::ptr::null(), true);
        }
        tracking
    }
}

/// Who observes the cancel notification: any address of ours, as long as it's the same one.
fn observer() -> *const c_void {
    static OBSERVER: u8 = 0;
    &OBSERVER as *const u8 as *const c_void
}

impl Drop for Tracking {
    fn drop(&mut self) {
        if let Some(mask) = self.saved_mask {
            if let Some(set) = function!(c"CGSSetBackgroundEventMask" as extern "C" fn(i32, u32) -> i32) {
                set(self.connection, mask);
            }
            if let Some(decline) = function!(c"_DeclineBackgroundMouseMovedEvents" as extern "C" fn()) {
                decline();
            }
        }
        if let (Some(id), Some(release)) = (self.focus, function!(c"CPSReleaseKeyFocusWithID" as extern "C" fn(u32) -> i32)) {
            release(id);
        }
        if self.observing {
            if let (Some(center), Some(post), Some(remove)) = (
                function!(c"CFNotificationCenterGetDistributedCenter" as extern "C" fn() -> *const c_void),
                function!(c"CFNotificationCenterPostNotification" as extern "C" fn(*const c_void, Id, *const c_void, *const c_void, bool)),
                function!(c"CFNotificationCenterRemoveObserver" as extern "C" fn(*const c_void, *const c_void, Id, *const c_void)),
            ) {
                remove(center(), observer(), nsstring(CANCEL_TRACKING), core::ptr::null());
                post(center(), nsstring(END_TRACKING), core::ptr::null(), core::ptr::null(), true);
            }
        }
    }
}
