//! Glass that's a menu-like panel hanging from the menu bar (Control Center's modules, a menu bar
//! extra's popover-ish window) looks and opens like our menus.
//!
//! A panel is top-level glass (not on other glass) whose shape has a rounded rect the size of the
//! glass, in a borderless window at the floating level or above, no wider than most of the screen,
//! with its top edge just under the menu bar. Such glass gets:
//!
//! - our menu body under it: a `CAShapeLayer` with the menus' black, smoothed corners
//!   (`shape::rounded_rect` at the menus' radius and smoothing) and shadow, added to the
//!   `SDFLayer` last (so SwiftUI's own sublayers keep their indices) with a `zPosition` below them;
//! - a mask of the same shape on each of SwiftUI's sublayers, so the glass's own outline and its
//!   content end where our body does;
//! - when its window is ordered in, our menus' opening: the body unrolls from the edge or corner
//!   under the status item it hangs from, with the menus' springs, and the content fades in. The
//!   springs are sampled into `CAKeyframeAnimation`s, so the render server plays them, with no
//!   timer of ours in the app. A fade-in the app runs on the window at the same time is cut
//!   short, so the two don't fight, and the window's own shadow and open animation are turned
//!   off (the body has the shadow).
//!
//! Masks and paths follow the glass's size on every layout. All of it is on the main thread.

use alloc::vec::Vec;
use core::ffi::{CStr, c_void};

use eris_inject_payload::sys::SpinLock;
use menus_core::Rect;
use menus_core::layout::Metrics;
use menus_core::shape;
use menus_core::spring::Spring;

use crate::glass::{Kit, is_kind, kit};
use crate::objc::{self, Id, NIL, Original, Sel, class, nsstring, release};
use crate::style::{self, palette};
use crate::sys::{self, CGPathRelease, CGPathRef, CGPoint, CGRect, CGSize, now};
use crate::msg;

#[link(name = "objc")]
unsafe extern "C" {
    fn objc_getAssociatedObject(obj: Id, key: *const c_void) -> Id;
    fn objc_setAssociatedObject(obj: Id, key: *const c_void, value: Id, policy: usize);
}

const RETAIN: usize = 1;
/// Our body layer under a panel's glass.
static BODY_KEY: u8 = 0;
/// An `NSMutableArray` of the masks we put on the glass's sublayers.
static MASKS_KEY: u8 = 0;

/// How long after its window is ordered in a panel still opens with our animation, when SwiftUI
/// lays it out only then.
const OPEN_WINDOW: f64 = 0.6;
/// Frames per second the springs are sampled at.
const FPS: f64 = 60.0;
/// The status bar's level and the menu bar's (`NSStatusWindowLevel`, `NSMainMenuWindowLevel`).
const MENU_BAR_LEVEL: isize = 24;
const FLOATING_LEVEL: isize = 3;
/// How far under the menu bar a panel's top may be and still hang from it.
const HANG: f64 = 48.0;

/// Windows ordered in a moment ago, waiting for their glass: (window, when).
static PENDING: SpinLock<Vec<(usize, f64)>> = SpinLock::new(Vec::new());
/// Windows we're opening: (window, until when, the alpha the app asked for last). A fade-in the
/// app starts goes straight to opaque until then.
static OPENING: SpinLock<Vec<(usize, f64, f64)>> = SpinLock::new(Vec::new());

fn key(k: &'static u8) -> *const c_void {
    (k as *const u8).cast()
}

fn associated(obj: Id, k: &'static u8) -> Id {
    unsafe { objc_getAssociatedObject(obj, key(k)) }
}

fn on_main() -> bool {
    msg!(bool; class(c"NSThread"), c"isMainThread")
}

/// How a panel sits: which way its layer's y grows, and where along its top it hangs from (0 the
/// left corner, 1 the right, ½ the middle of the edge).
#[derive(Clone, Copy)]
struct Hang {
    up: bool,
    anchor: f64,
}

/// After the glass pass on top-level glass: dresses it as a panel if it is one, undresses it if it
/// was and isn't any more. Main thread only; elsewhere it's left as it is.
pub fn restyle(kit: &Kit, sdf: Id) {
    if !on_main() {
        return;
    }
    match hang(kit, sdf) {
        Some(h) => {
            dress(sdf);
            let window = window_of(sdf);
            if !window.is_null() && take_pending(window) {
                open(sdf, h, window);
            }
        }
        None => undress(sdf),
    }
}

/// Glass on other glass, or glass that has stopped being glass: nothing of ours stays on it.
pub fn undress(sdf: Id) {
    if !on_main() {
        return;
    }
    let body = associated(sdf, &BODY_KEY);
    if !body.is_null() {
        msg!((); body, c"removeFromSuperlayer");
        unsafe { objc_setAssociatedObject(sdf, key(&BODY_KEY), NIL, RETAIN) };
    }
    let masks = associated(sdf, &MASKS_KEY);
    if !masks.is_null() {
        for_each_child(sdf, |child| {
            let mask = msg!(Id; child, c"mask");
            if !mask.is_null() && msg!(bool; masks, c"containsObject:", mask => Id) {
                msg!((); child, c"setMask:", NIL => Id);
            }
        });
        unsafe { objc_setAssociatedObject(sdf, key(&MASKS_KEY), NIL, RETAIN) };
    }
}

fn for_each_child(layer: Id, mut f: impl FnMut(Id)) {
    let sublayers = msg!(Id; layer, c"sublayers");
    if sublayers.is_null() {
        return;
    }
    // A copy: `f` may add to the array.
    let sublayers = msg!(Id; sublayers, c"copy");
    for i in 0..msg!(usize; sublayers, c"count") {
        f(msg!(Id; sublayers, c"objectAtIndex:", i => usize));
    }
    release(sublayers);
}

/// The view `layer` draws for, a few layers up at most.
fn owner_view(layer: Id) -> Id {
    let ns_view = class(c"NSView");
    let mut l = msg!(Id; layer, c"superlayer");
    for _ in 0..12 {
        if l.is_null() {
            return NIL;
        }
        let delegate = if objc::responds(l, c"delegate") { msg!(Id; l, c"delegate") } else { NIL };
        if is_kind(delegate, ns_view) {
            return delegate;
        }
        l = msg!(Id; l, c"superlayer");
    }
    NIL
}

fn window_of(sdf: Id) -> Id {
    let view = owner_view(sdf);
    if view.is_null() { NIL } else { msg!(Id; view, c"window") }
}

/// A point in `sdf`'s space, on screen.
fn to_screen(sdf: Id, view: Id, window: Id, p: CGPoint) -> CGPoint {
    let view_layer = msg!(Id; view, c"layer");
    let in_view = msg!(CGPoint; sdf, c"convertPoint:toLayer:", p => CGPoint, view_layer => Id);
    let in_window = msg!(CGPoint; view, c"convertPoint:toView:", in_view => CGPoint, NIL => Id);
    let r = msg!(CGRect; window, c"convertRectToScreen:", CGRect { origin: in_window, size: CGSize::default() } => CGRect);
    r.origin
}

/// Whether `sdf` is a panel hanging from the menu bar, and how.
fn hang(kit: &Kit, sdf: Id) -> Option<Hang> {
    let bounds = msg!(CGRect; sdf, c"bounds");
    let (w, h) = (bounds.size.width, bounds.size.height);
    if w < 40.0 || h < 40.0 || !has_body_rect(kit, sdf, bounds.size, 0) {
        return None;
    }
    let view = owner_view(sdf);
    if view.is_null() || msg!(Id; view, c"layer").is_null() {
        return None;
    }
    let window = msg!(Id; view, c"window");
    if window.is_null() {
        return None;
    }
    const TITLED: usize = 1;
    if msg!(usize; window, c"styleMask") & TITLED != 0 || msg!(isize; window, c"level") < FLOATING_LEVEL {
        return None;
    }
    let mut screen = msg!(Id; window, c"screen");
    if screen.is_null() {
        screen = msg!(Id; class(c"NSScreen"), c"mainScreen");
    }
    if screen.is_null() {
        return None;
    }
    let frame = msg!(CGRect; screen, c"frame");
    let visible = msg!(CGRect; screen, c"visibleFrame");
    if msg!(CGRect; window, c"frame").size.width > frame.size.width * 0.6 {
        return None;
    }
    let top = frame.origin.y + frame.size.height;
    let visible_top = visible.origin.y + visible.size.height;
    let bar_bottom = if visible_top < top - 1.0 { visible_top } else { top - 24.0 };

    let p0 = to_screen(sdf, view, window, CGPoint { x: 0.0, y: 0.0 });
    let p1 = to_screen(sdf, view, window, CGPoint { x: w, y: h });
    let up = p1.y > p0.y;
    let (min_x, max_x) = (p0.x.min(p1.x), p0.x.max(p1.x));
    let glass_top = p0.y.max(p1.y);
    if glass_top > bar_bottom + 1.0 || glass_top < bar_bottom - HANG {
        return None;
    }
    let anchor = anchor(bar_bottom, min_x, max_x, frame);
    // Our anchor is along the screen's x; flip it if the layer's x runs the other way.
    let anchor = if p1.x >= p0.x { anchor } else { 1.0 - anchor };
    Some(Hang { up, anchor })
}

/// Whether the glass's shape has a rounded rect as big as the glass (its body, other shapes
/// merged into it or not).
fn has_body_rect(kit: &Kit, layer: Id, size: CGSize, depth: usize) -> bool {
    if depth > 0 && is_kind(layer, class(c"CASDFElementLayer")) {
        let s = msg!(CGRect; layer, c"bounds").size;
        if (s.width - size.width).abs() <= 1.5 && (s.height - size.height).abs() <= 1.5 {
            return true;
        }
    }
    if depth >= 6 {
        return false;
    }
    let mut found = false;
    for_each_child(layer, |sub| {
        if !found && !is_kind(sub, kit.sdf_layer_class) {
            found = has_body_rect(kit, sub, size, depth + 1);
        }
    });
    found
}

/// Where along the panel's top it opens from: under the status item it hangs from (the menu bar
/// window over it nearest its middle), snapped to a corner or the middle; the corner on the
/// screen's nearer side when there's none.
fn anchor(bar_bottom: f64, min_x: f64, max_x: f64, screen: CGRect) -> f64 {
    let windows = msg!(Id; msg!(Id; class(c"NSApplication"), c"sharedApplication"), c"windows");
    let mid = (min_x + max_x) / 2.0;
    let mut best: Option<f64> = None;
    for i in 0..msg!(usize; windows, c"count") {
        let w = msg!(Id; windows, c"objectAtIndex:", i => usize);
        if !msg!(bool; w, c"isVisible") || msg!(isize; w, c"level") < MENU_BAR_LEVEL {
            continue;
        }
        let f = msg!(CGRect; w, c"frame");
        let (fx, fmax) = (f.origin.x, f.origin.x + f.size.width);
        if f.origin.y < bar_bottom - 1.0 || f.size.width > 400.0 || fmax <= min_x || fx >= max_x {
            continue;
        }
        let item = (fx + fmax) / 2.0;
        if best.is_none_or(|b| (item - mid).abs() < (b - mid).abs()) {
            best = Some(item);
        }
    }
    let width = (max_x - min_x).max(1.0);
    match best {
        Some(item) => {
            let t = ((item - min_x) / width).clamp(0.0, 1.0);
            if t < 1.0 / 3.0 {
                0.0
            } else if t > 2.0 / 3.0 {
                1.0
            } else {
                0.5
            }
        }
        None if mid > screen.origin.x + screen.size.width / 2.0 => 1.0,
        None => 0.0,
    }
}

/// Our body's path for a `w`×`h` rect hanging from the top of a `full` one, in the glass's space.
fn body_path(full: CGSize, w: f64, h: f64, hang: Hang, dx: f64, dy: f64) -> CGPathRef {
    let m = Metrics::default();
    let x = (full.width - w) * hang.anchor;
    let y = if hang.up { full.height - h } else { 0.0 };
    let elements = shape::rounded_rect(Rect::new(x, y, w, h), m.corner_radius, m.smoothing);
    sys::path(&elements, dx, dy)
}

const FULL: Hang = Hang { up: true, anchor: 0.0 };

/// Puts our body under the glass and our masks on it, at its current size.
fn dress(sdf: Id) {
    let size = msg!(CGRect; sdf, c"bounds").size;
    let catransaction = class(c"CATransaction");
    msg!((); catransaction, c"begin");
    msg!((); catransaction, c"setDisableActions:", true => bool);

    let mut body = associated(sdf, &BODY_KEY);
    if body.is_null() {
        body = msg!(Id; class(c"CAShapeLayer"), c"layer");
        let p = palette();
        msg!((); body, c"setFillColor:", p.body => *mut c_void);
        msg!((); body, c"setShadowColor:", p.shadow => *mut c_void);
        msg!((); body, c"setShadowOpacity:", style::SHADOW_OPACITY => f32);
        msg!((); body, c"setShadowRadius:", style::SHADOW_RADIUS => f64);
        // Below every one of SwiftUI's sublayers, though it's added after them.
        msg!((); body, c"setZPosition:", -1.0 => f64);
        msg!((); body, c"setName:", nsstring(c"ErisMenusPanelBody") => Id);
        unsafe { objc_setAssociatedObject(sdf, key(&BODY_KEY), body, RETAIN) };
    }
    if msg!(Id; body, c"superlayer") != sdf {
        msg!((); sdf, c"addSublayer:", body => Id);
    }
    let view = owner_view(sdf);
    let window = if view.is_null() { NIL } else { msg!(Id; view, c"window") };
    // Down on screen, whichever way the layer's y runs.
    let up = window.is_null()
        || to_screen(sdf, view, window, CGPoint { x: 0.0, y: 1.0 }).y > to_screen(sdf, view, window, CGPoint { x: 0.0, y: 0.0 }).y;
    let drop = if up { -style::SHADOW_DROP } else { style::SHADOW_DROP };
    msg!((); body, c"setShadowOffset:", CGSize { width: 0.0, height: drop } => CGSize);
    msg!((); body, c"setFrame:", CGRect { origin: CGPoint::default(), size } => CGRect);
    let path = body_path(size, size.width, size.height, FULL, 0.0, 0.0);
    msg!((); body, c"setPath:", path => CGPathRef);
    msg!((); body, c"setShadowPath:", path => CGPathRef);
    unsafe { CGPathRelease(path) };

    let mut masks = associated(sdf, &MASKS_KEY);
    if masks.is_null() {
        masks = msg!(Id; class(c"NSMutableArray"), c"array");
        unsafe { objc_setAssociatedObject(sdf, key(&MASKS_KEY), masks, RETAIN) };
    }
    for_each_child(sdf, |child| {
        if child == body || msg!(CGRect; child, c"bounds").size.width <= 0.0 {
            return;
        }
        let mut mask = msg!(Id; child, c"mask");
        if mask.is_null() {
            mask = msg!(Id; class(c"CAShapeLayer"), c"layer");
            msg!((); mask, c"setFillColor:", palette().body => *mut c_void);
            msg!((); masks, c"addObject:", mask => Id);
            msg!((); child, c"setMask:", mask => Id);
        } else if !msg!(bool; masks, c"containsObject:", mask => Id) {
            // The app's own mask: leave it.
            return;
        }
        let o = msg!(CGPoint; sdf, c"convertPoint:toLayer:", CGPoint::default() => CGPoint, child => Id);
        let path = body_path(size, size.width, size.height, FULL, o.x, o.y);
        msg!((); mask, c"setPath:", path => CGPathRef);
        unsafe { CGPathRelease(path) };
    });

    if !window.is_null() {
        if msg!(bool; window, c"hasShadow") {
            msg!((); window, c"setHasShadow:", false => bool);
        }
        const NO_ANIMATION: isize = 2;
        if msg!(isize; window, c"animationBehavior") != NO_ANIMATION {
            msg!((); window, c"setAnimationBehavior:", NO_ANIMATION => isize);
        }
    }
    msg!((); catransaction, c"commit");
}

/// The springs of our menus' opening, sampled: (width, height, reveal) at each frame.
fn samples(size: CGSize) -> Vec<(f64, f64, f32)> {
    let a = crate::settings::current().style.animation();
    let mut width = Spring::new(size.width * a.collapsed_width, a.widen).with_epsilon(0.05);
    let mut height = Spring::new(size.height * a.collapsed_height, a.unroll).with_epsilon(0.05);
    let mut reveal = Spring::new(0.0, a.reveal);
    width.animate(size.width);
    height.animate(size.height);
    reveal.animate(1.0);
    let mut out = Vec::new();
    for _ in 0..(FPS as usize * 2) {
        out.push((width.value.max(0.0), height.value.max(0.0), reveal.value.clamp(0.0, 1.0) as f32));
        if width.is_resting() && height.is_resting() && reveal.is_resting() {
            break;
        }
        for s in [&mut width, &mut height, &mut reveal] {
            s.step(1.0 / FPS);
        }
    }
    out
}

fn keyframes(key_path: &CStr, values: Id, duration: f64) -> Id {
    let animation = msg!(Id; class(c"CAKeyframeAnimation"), c"animationWithKeyPath:", nsstring(key_path) => Id);
    msg!((); animation, c"setValues:", values => Id);
    msg!((); animation, c"setDuration:", duration => f64);
    animation
}

/// The opening's paths, offset by (dx, dy).
fn path_values(size: CGSize, frames: &[(f64, f64, f32)], hang: Hang, dx: f64, dy: f64) -> Id {
    let values = msg!(Id; class(c"NSMutableArray"), c"arrayWithCapacity:", frames.len() => usize);
    for &(w, h, _) in frames {
        let path = body_path(size, w, h, hang, dx, dy);
        msg!((); values, c"addObject:", path as Id => Id);
        unsafe { CGPathRelease(path) };
    }
    values
}

/// Plays our menus' opening on a dressed panel.
fn open(sdf: Id, hang: Hang, window: Id) {
    let body = associated(sdf, &BODY_KEY);
    let masks = associated(sdf, &MASKS_KEY);
    if body.is_null() || masks.is_null() {
        return;
    }
    let size = msg!(CGRect; sdf, c"bounds").size;
    let frames = samples(size);
    if frames.len() < 2 {
        return;
    }
    let duration = (frames.len() - 1) as f64 / FPS;
    objc::autoreleasing(|| {
        let values = path_values(size, &frames, hang, 0.0, 0.0);
        msg!((); body, c"addAnimation:forKey:", keyframes(c"path", values, duration) => Id, nsstring(c"ErisMenusOpen") => Id);
        msg!((); body, c"addAnimation:forKey:", keyframes(c"shadowPath", values, duration) => Id, nsstring(c"ErisMenusShadow") => Id);
        let reveal = msg!(Id; class(c"NSMutableArray"), c"arrayWithCapacity:", frames.len() => usize);
        for &(_, _, r) in &frames {
            msg!((); reveal, c"addObject:", msg!(Id; class(c"NSNumber"), c"numberWithFloat:", r => f32) => Id);
        }
        let portal_class = class(c"CAPortalLayer");
        for_each_child(sdf, |child| {
            let mask = msg!(Id; child, c"mask");
            if !mask.is_null() && msg!(bool; masks, c"containsObject:", mask => Id) {
                let o = msg!(CGPoint; sdf, c"convertPoint:toLayer:", CGPoint::default() => CGPoint, child => Id);
                let values = path_values(size, &frames, hang, o.x, o.y);
                msg!((); mask, c"addAnimation:forKey:", keyframes(c"path", values, duration) => Id, nsstring(c"ErisMenusOpen") => Id);
            }
            // The content shows through a portal over the glass; it fades in as our rows do.
            let fade = |portal: Id| {
                msg!((); portal, c"addAnimation:forKey:", keyframes(c"opacity", reveal, duration) => Id, nsstring(c"ErisMenusReveal") => Id);
            };
            if is_kind(child, portal_class) {
                fade(child);
            } else {
                for_each_child(child, |grandchild| {
                    if is_kind(grandchild, portal_class) && msg!(CGRect; grandchild, c"bounds").size.width > 0.0 {
                        fade(grandchild);
                    }
                });
            }
        });
    });
    // The app's own fade-in, if it runs one, would play over ours.
    let alpha = msg!(f64; window, c"alphaValue");
    OPENING.with(|o| {
        o.retain(|&(_, until, _)| until > now());
        o.push((window as usize, now() + duration, alpha));
    });
    if alpha < 1.0 {
        set_alpha(window, 1.0);
    }
}

fn take_pending(window: Id) -> bool {
    PENDING.with(|p| {
        let t = now();
        p.retain(|&(_, when)| t - when < OPEN_WINDOW);
        match p.iter().position(|&(w, _)| w == window as usize) {
            Some(i) => {
                p.remove(i);
                true
            }
            None => false,
        }
    })
}

/// `-[NSWindow _doOrderWindow:]`, where every way of ordering a window in or out ends up.
pub static ORDER: Original = Original::new();

pub extern "C" fn do_order_window(this: Id, cmd: Sel, options: Id) {
    let was_visible = msg!(bool; this, c"isVisible");
    let original: extern "C" fn(Id, Sel, Id) = unsafe { core::mem::transmute(ORDER.get()) };
    original(this, cmd, options);
    if !was_visible && msg!(bool; this, c"isVisible") && on_main() && crate::settings::current().glass {
        objc::autoreleasing(|| shown(this));
    }
}

/// A window just came on screen: open the panels in it that are dressed already, or wait a
/// moment for SwiftUI to lay them out.
fn shown(window: Id) {
    const TITLED: usize = 1;
    if msg!(usize; window, c"styleMask") & TITLED != 0 || msg!(isize; window, c"level") < FLOATING_LEVEL {
        return;
    }
    if !open_dressed(window) {
        PENDING.with(|p| p.push((window as usize, now())));
    }
}

/// Opens the dressed panels in `window`; whether there were any.
fn open_dressed(window: Id) -> bool {
    let Some(kit) = kit() else { return false };
    let content = msg!(Id; window, c"contentView");
    if content.is_null() {
        return false;
    }
    let frame_view = msg!(Id; content, c"superview");
    // A window's layers can come only with its first display.
    let root = msg!(Id; if frame_view.is_null() { content } else { frame_view }, c"layer");
    if root.is_null() {
        return false;
    }
    let mut opened = false;
    let mut stack = alloc::vec![root];
    let mut seen = 0;
    while let Some(layer) = stack.pop() {
        seen += 1;
        if seen > 4000 {
            break;
        }
        if is_kind(layer, kit.sdf_layer_class) && !associated(layer, &BODY_KEY).is_null() {
            if let Some(h) = hang(kit, layer) {
                dress(layer);
                open(layer, h, window);
                opened = true;
            }
            continue;
        }
        for_each_child(layer, |sub| stack.push(sub));
    }
    opened
}

/// `-[NSWindow setAlphaValue:]`: while one of ours opens, a fade-in goes straight to opaque.
pub static ALPHA: Original = Original::new();

pub extern "C" fn set_alpha_value(this: Id, cmd: Sel, alpha: f64) {
    let mut alpha = alpha;
    if !OPENING.with(|o| o.is_empty()) {
        OPENING.with(|o| {
            let t = now();
            if let Some(entry) = o.iter_mut().find(|e| e.0 == this as usize && e.1 > t) {
                let asked = alpha;
                if asked > entry.2 && asked > 0.0 {
                    alpha = 1.0;
                }
                entry.2 = asked;
            }
        });
    }
    let original: extern "C" fn(Id, Sel, f64) = unsafe { core::mem::transmute(ALPHA.get()) };
    original(this, cmd, alpha)
}

/// Sets a window's alpha past our own hook.
fn set_alpha(window: Id, alpha: f64) {
    if ALPHA.get().is_null() {
        msg!((); window, c"setAlphaValue:", alpha => f64);
    } else {
        let original: extern "C" fn(Id, Sel, f64) = unsafe { core::mem::transmute(ALPHA.get()) };
        original(window, objc::sel(c"setAlphaValue:"), alpha);
    }
}
