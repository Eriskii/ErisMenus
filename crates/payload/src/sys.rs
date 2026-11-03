//! CoreGraphics and QuartzCore bits: geometry structs, paths, colors, the clock.

use core::ffi::c_void;

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
