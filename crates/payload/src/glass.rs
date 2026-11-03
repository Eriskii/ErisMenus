//! Liquid Glass, painted over: every glass surface in the app gets ErisDock's opaque black body in
//! the glass's own shape, with no refraction, rims or sheen; tooltips get the same body.
//!
//! On macOS 26.4 all glass in an app is drawn by one class. SwiftUI's `glassEffect` and glass
//! materials, AppKit's `NSGlassEffectView` (which hosts SwiftUI), `NSPopover`, alerts, glass
//! buttons and Control Center's modules all end up as a `SwiftUI.SDFLayer` holding:
//!
//! - a `CABackdropLayer` with the `glassBackground` filter, whose `CASDFLayer` child, with a
//!   `CASDFOutputEffect`, hands the filter the shape as a distance field (the refraction);
//! - the content, portalled above it;
//! - `CASDFLayer`s with highlight effects (`CASDFKeyFillHighlightEffect`, the rim) on top, and
//!   for tinted glass a `CASDFGradientEffect` whose color matrix works on what's behind it.
//!
//! After every `-[SwiftUI.SDFLayer layoutSublayers]` (where SwiftUI builds and updates all that)
//! the filter comes off and the backdrop stops capturing; the output effect becomes a subclass
//! that the render server gets as a solid black `CASDFFillEffect`, so the shape, its corners and
//! merged glass all stay exactly SwiftUI's; the highlight layers are hidden. SwiftUI keeps reading
//! those layers, so their effects keep their classes: only what's committed to the render server
//! changes. Glass inside glass (Control Center's tiles on its panel) gets a dark gray instead, so
//! it still reads as a tile; a tint still shows, now over black. Content stays light: glass that
//! adapts to what's behind it is told it's over black, and in light mode the glass's view (or the
//! window it fills) goes dark.
//!
//! Tooltips aren't glass but an `NSVisualEffectView` blur in an `NSToolTipPanel`; after
//! `-[NSToolTipManager installContentView:forToolTip:toolTipWindow:isNew:]` the blur is turned
//! off and a black rounded body put under the text.
//!
//! `defaults write <app> ErisMenusGlassDisabled -bool YES` (or `ErisMenusDisabled`) leaves an
//! app's glass alone, from its next launch.

use core::ffi::{CStr, c_char, c_void};
use core::mem::transmute;
use core::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

use crate::objc::{self, Id, Imp, NIL, Original, Sel, class, define_class, nsstring, retain, sel, swizzle};
use crate::sys::{CGRect, gray};
use crate::{glass_panel, log, msg};

/// How deep under an `SDFLayer` its glass layers sit (three on 26.4), with room to spare.
const MAX_DEPTH: usize = 5;
/// How far up to look for glass this glass sits in.
const MAX_ANCESTORS: usize = 64;
/// Glass inside glass: ErisDock's black, lifted a little.
const INNER_GRAY: f64 = 0.11;
/// A tooltip body's corner radius.
const TOOLTIP_RADIUS: f64 = 7.0;
/// Names the view we put under a tooltip's text.
const TOOLTIP_BODY: &CStr = c"ErisMenusToolTipBody";

#[link(name = "objc")]
unsafe extern "C" {
    fn object_getClass(obj: Id) -> Id;
    fn object_setClass(obj: Id, cls: Id) -> Id;
    fn class_getSuperclass(cls: Id) -> Id;
    fn class_getInstanceMethod(cls: Id, sel: Sel) -> *mut c_void;
    fn objc_getAssociatedObject(obj: Id, key: *const c_void) -> Id;
    fn objc_setAssociatedObject(obj: Id, key: *const c_void, value: Id, policy: usize);
}

unsafe extern "C" {
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

/// Marks an `SDFLayer` that draws glass (its filter is gone after the first pass, so it can't be
/// told by that any more). The key is this static's address.
static GLASS_KEY: u8 = 0;
const OBJC_ASSOCIATION_RETAIN_NONATOMIC: usize = 1;

/// What the hooks need, made once on the main thread before any of them can run.
pub(crate) struct Kit {
    backdrop: Id,
    sdf_layer: Id,
    pub(crate) sdf_layer_class: Id,
    output: Id,
    /// `NSGlassEffectView`, whose own glass is an `SDFLayer` in a hosting view inside it.
    glass_view: Id,
    /// Our subclasses of `CASDFOutputEffect`: the black body, and the gray one for glass in glass.
    body_effect: Id,
    inner_effect: Id,
    /// The effects whose layers get hidden: rims, sheen, refraction.
    hidden: [Id; 3],
    glass_filter: Id,
    /// The `CASDFFillEffect`s the render server gets instead of the glass.
    fill: Id,
    inner_fill: Id,
}

static KIT: AtomicPtr<Kit> = AtomicPtr::new(core::ptr::null_mut());

pub(crate) fn kit() -> Option<&'static Kit> {
    let k = KIT.load(Ordering::Acquire);
    (!k.is_null()).then(|| unsafe { &*k })
}

static INSTALLED: AtomicBool = AtomicBool::new(false);

static LAYOUT: Original = Original::new();
static LUMA: Original = Original::new();
static TOOLTIP: Original = Original::new();

struct Hook {
    class: &'static CStr,
    method: &'static CStr,
    imp: Imp,
    original: &'static Original,
    /// Useless without the glass classes.
    glass: bool,
}

const HOOKS: usize = 5;

fn hooks() -> [Hook; HOOKS] {
    let hook = |class, method, imp, original, glass| Hook { class, method, imp, original, glass };
    [
        hook(c"SwiftUI.SDFLayer", c"layoutSublayers", sdf_layout as Imp, &LAYOUT, true),
        hook(c"SwiftUI.SDFLayer", c"backdropLayer:didChangeLuma:", sdf_luma as Imp, &LUMA, true),
        hook(c"NSToolTipManager", c"installContentView:forToolTip:toolTipWindow:isNew:", install_tooltip as Imp, &TOOLTIP, false),
        // Panels of glass open like our menus (`glass_panel`).
        hook(c"NSWindow", c"_doOrderWindow:", glass_panel::do_order_window as Imp, &glass_panel::ORDER, true),
        hook(c"NSWindow", c"setAlphaValue:", glass_panel::set_alpha_value as Imp, &glass_panel::ALPHA, true),
    ]
}

impl Hook {
    /// Only a method the class defines itself: replacing an inherited one would change it for
    /// the superclass and all its other subclasses.
    fn apply(&self) -> bool {
        let cls = class(self.class);
        if cls.is_null() {
            return false;
        }
        let m = unsafe { class_getInstanceMethod(cls, sel(self.method)) };
        let superclass = unsafe { class_getSuperclass(cls) };
        if m.is_null() || (!superclass.is_null() && unsafe { class_getInstanceMethod(superclass, sel(self.method)) } == m) {
            return false;
        }
        swizzle(self.class, self.method, self.imp, self.original)
    }

    fn in_place(&self) -> bool {
        objc::implementation(self.class, self.method, false) == Some(self.imp)
    }
}

/// Paints glass over in this app. Main thread, once the app is up.
pub fn install() {
    if INSTALLED.swap(true, Ordering::AcqRel) {
        return;
    }
    if default(c"ErisMenusDisabled") || default(c"ErisMenusGlassDisabled") {
        log!("glass is left alone in this app (ErisMenusGlassDisabled)");
        return;
    }
    // Same marker scheme as the menus: this build's glass is already here, or an older one's is
    // and ours goes over it (both only ever paint black, so a doubled pass is harmless).
    let marker = concat!("ErisMenusGlassBuild", env!("ERISMENUS_BUILD"), "\0");
    let marker = CStr::from_bytes_with_nul(marker.as_bytes()).unwrap_or(c"ErisMenusGlassBuild");
    if define_class(marker, c"NSObject", &[]).is_null() {
        return;
    }
    if let Some(kit) = make_kit() {
        KIT.store(alloc::boxed::Box::leak(alloc::boxed::Box::new(kit)), Ordering::Release);
    } else {
        log!("no SwiftUI glass classes here; glass is left alone");
    }
    let mut placed = 0;
    for hook in hooks() {
        if hook.glass && kit().is_none() {
            continue;
        }
        if hook.apply() {
            placed += 1;
        } else {
            log!("no -[{} {}]; that part of the glass is skipped", name(hook.class), name(hook.method));
        }
    }
    log!("painting over glass in pid {} ({placed} of {HOOKS} hooks)", unsafe { crate::rt::getpid() });
    // Glass already on screen was laid out before we were here, and may not be again for a while.
    if let Some(kit) = kit() {
        objc::autoreleasing(|| repaint_windows(kit));
    }
    eris_inject_payload::sys::on_main_after(5.0, verify);
}

/// Every `SDFLayer` in the app's windows, painted over now.
fn repaint_windows(kit: &Kit) {
    const MAX_LAYERS: usize = 20_000;
    let windows = msg!(Id; msg!(Id; class(c"NSApplication"), c"sharedApplication"), c"windows");
    let mut stack = alloc::vec::Vec::new();
    for i in 0..msg!(usize; windows, c"count") {
        let content = msg!(Id; msg!(Id; windows, c"objectAtIndex:", i => usize), c"contentView");
        if content.is_null() {
            continue;
        }
        let frame = msg!(Id; content, c"superview");
        let layer = msg!(Id; if frame.is_null() { content } else { frame }, c"layer");
        if !layer.is_null() {
            stack.push(layer);
        }
    }
    let (mut seen, mut painted) = (0, 0);
    while let Some(layer) = stack.pop() {
        seen += 1;
        if seen > MAX_LAYERS {
            break;
        }
        if is_kind(layer, kit.sdf_layer_class) {
            restyle(kit, layer);
            painted += 1;
        }
        let sublayers = msg!(Id; layer, c"sublayers");
        if !sublayers.is_null() {
            for j in 0..msg!(usize; sublayers, c"count") {
                stack.push(msg!(Id; sublayers, c"objectAtIndex:", j => usize));
            }
        }
    }
    if painted > 0 {
        log!("painted over {painted} glass layers already on screen");
    }
}

extern "C" fn verify(_: *mut c_void) {
    for hook in hooks() {
        if !hook.in_place() && !hook.original.get().is_null() {
            log!("-[{} {}] was replaced after we hooked it; hooking it again", name(hook.class), name(hook.method));
            hook.apply();
        }
    }
}

fn name(s: &CStr) -> &str {
    s.to_str().unwrap_or("?")
}

fn default(key: &CStr) -> bool {
    let defaults = msg!(Id; class(c"NSUserDefaults"), c"standardUserDefaults");
    msg!(bool; defaults, c"boolForKey:", nsstring(key) => Id)
}

/// Looks up every private class and constant the glass pass touches; None if any is missing
/// (another macOS, say), and then the glass is left alone.
fn make_kit() -> Option<Kit> {
    let backdrop = class(c"CABackdropLayer");
    let sdf_layer = class(c"CASDFLayer");
    let sdf_layer_class = class(c"SwiftUI.SDFLayer");
    let output = class(c"CASDFOutputEffect");
    let fill_class = class(c"CASDFFillEffect");
    let hidden = [class(c"CASDFKeyFillHighlightEffect"), class(c"CASDFGlassHighlightEffect"), class(c"CASDFGlassDisplacementEffect")];
    if [backdrop, sdf_layer, sdf_layer_class, output, fill_class].iter().any(|c| c.is_null()) {
        return None;
    }
    // The effect must take a color and configure a render layer the way every effect does.
    let configure = c"configureLayer:transaction:";
    if !instances_respond(fill_class, c"setColor:") || !instances_respond(fill_class, configure) || !instances_respond(output, configure) {
        return None;
    }
    let glass_filter = {
        let p = unsafe { dlsym(-2isize as *mut c_void, c"kCAFilterGlassBackground".as_ptr()) } as *const Id;
        let named = if p.is_null() { NIL } else { unsafe { *p } };
        retain(if named.is_null() { nsstring(c"glassBackground") } else { named })
    };
    let make_fill = |white: f64| {
        let fill = msg!(Id; msg!(Id; fill_class, c"alloc"), c"init");
        if !fill.is_null() {
            msg!((); fill, c"setColor:", gray(white, 1.0) => *mut c_void);
        }
        fill
    };
    let (fill, inner_fill) = (make_fill(0.0), make_fill(INNER_GRAY));
    let subclass = |name: &CStr, imp: Imp| {
        let cls = define_class(name, c"CASDFOutputEffect", &[(configure, imp, c"v32@0:8^v16^v24")]);
        // Null: an older copy of us made it; its method does the same.
        if cls.is_null() { class(name) } else { cls }
    };
    let body_effect = subclass(c"ErisMenusGlassBody", configure_body as Imp);
    let inner_effect = subclass(c"ErisMenusGlassInner", configure_inner as Imp);
    if [fill, inner_fill, body_effect, inner_effect].iter().any(|c| c.is_null()) {
        return None;
    }
    let glass_view = class(c"NSGlassEffectView");
    Some(Kit { backdrop, sdf_layer, sdf_layer_class, output, glass_view, body_effect, inner_effect, hidden, glass_filter, fill, inner_fill })
}

fn instances_respond(cls: Id, method: &CStr) -> bool {
    msg!(bool; cls, c"instancesRespondToSelector:", sel(method) => Sel)
}

/// `-[ErisMenusGlassBody configureLayer:transaction:]`: what goes to the render server for the
/// shape under a glass backdrop. A solid fill instead of the distance field the glass filter
/// would have refracted with. Runs on whichever thread commits.
extern "C" fn configure_body(_this: Id, cmd: Sel, layer: *mut c_void, transaction: *mut c_void) {
    if let Some(kit) = kit() {
        configure_as(kit.fill, cmd, layer, transaction);
    }
}

/// `-[ErisMenusGlassInner configureLayer:transaction:]`: the same for glass inside glass.
extern "C" fn configure_inner(_this: Id, cmd: Sel, layer: *mut c_void, transaction: *mut c_void) {
    if let Some(kit) = kit() {
        configure_as(kit.inner_fill, cmd, layer, transaction);
    }
}

fn configure_as(fill: Id, cmd: Sel, layer: *mut c_void, transaction: *mut c_void) {
    let send: extern "C" fn(Id, Sel, *mut c_void, *mut c_void) = unsafe { transmute(objc::objc_msgSend as *const ()) };
    send(fill, cmd, layer, transaction);
}

pub(crate) fn is_kind(obj: Id, cls: Id) -> bool {
    !obj.is_null() && !cls.is_null() && msg!(bool; obj, c"isKindOfClass:", cls => Id)
}

/// `-[SwiftUI.SDFLayer layoutSublayers]`: SwiftUI has just (re)built the glass; paint it over.
extern "C" fn sdf_layout(this: Id, cmd: Sel) {
    let original: extern "C" fn(Id, Sel) = unsafe { transmute(LAYOUT.get()) };
    original(this, cmd);
    if let Some(kit) = kit() {
        objc::autoreleasing(|| restyle(kit, this));
    }
}

/// `-[SwiftUI.SDFLayer backdropLayer:didChangeLuma:]`: glass that adapts to what's behind it
/// switches its content dark over light backgrounds. Ours is black whatever is behind.
extern "C" fn sdf_luma(this: Id, cmd: Sel, backdrop: Id, luma: f64) {
    let original: extern "C" fn(Id, Sel, Id, f64) = unsafe { transmute(LUMA.get()) };
    let glass = !unsafe { objc_getAssociatedObject(this, (&raw const GLASS_KEY).cast()) }.is_null();
    original(this, cmd, backdrop, if glass { 0.0 } else { luma })
}

fn restyle(kit: &Kit, sdf: Id) {
    let key = (&raw const GLASS_KEY).cast::<c_void>();
    let mut glass = !unsafe { objc_getAssociatedObject(sdf, key) }.is_null();
    let backdrop = strip_filters(kit, sdf, 0);
    if !backdrop.is_null() && !glass {
        unsafe { objc_setAssociatedObject(sdf, key, kit.body_effect, OBJC_ASSOCIATION_RETAIN_NONATOMIC) };
        glass = true;
        // The backdrop is about to stop capturing, so it won't report luma again: adaptive glass
        // hears once (through our hook) that it's over black, rather than keep whatever it saw last.
        if objc::responds(sdf, c"backdropLayer:didChangeLuma:") {
            msg!((); sdf, c"backdropLayer:didChangeLuma:", backdrop => Id, 0.0 => f64);
        }
    }
    if glass {
        let inner = nested(kit, sdf);
        let body = if inner { kit.inner_effect } else { kit.body_effect };
        paint(kit, sdf, body, 0);
        if inner {
            glass_panel::undress(sdf);
        } else {
            glass_panel::restyle(kit, sdf);
        }
        // Views and windows are the main thread's; layers can be laid out on others.
        if msg!(bool; class(c"NSThread"), c"isMainThread") {
            darken_owner(kit, sdf);
        }
    }
}

/// Glass content takes dark colors in light mode, for glass that lets light through; on black it
/// has to be light. The `NSGlassEffectView` the glass belongs to goes dark, and so does a window
/// the glass fills (popovers, alerts, sheets, panels made of glass). SwiftUI glass that's only part
/// of a window takes its colors from the SwiftUI environment, which a layer can't reach: in light
/// mode its text stays dark.
fn darken_owner(kit: &Kit, sdf: Id) {
    let ns_view = class(c"NSView");
    let mut layer = msg!(Id; sdf, c"superlayer");
    let mut view = NIL;
    for level in 1..=8 {
        if layer.is_null() {
            break;
        }
        let delegate = if objc::responds(layer, c"delegate") { msg!(Id; layer, c"delegate") } else { NIL };
        if level <= 3 && is_kind(delegate, kit.glass_view) {
            make_dark(delegate);
        }
        if view.is_null() && is_kind(delegate, ns_view) {
            view = delegate;
        }
        layer = msg!(Id; layer, c"superlayer");
    }
    if view.is_null() {
        return;
    }
    let window = msg!(Id; view, c"window");
    if window.is_null() {
        return;
    }
    let size = msg!(CGRect; window, c"frame").size;
    let bounds = msg!(CGRect; sdf, c"bounds").size;
    if bounds.width >= size.width * 0.9 && bounds.height >= size.height * 0.9 {
        // On the content view: AppKit sets some of these windows' own appearance after showing
        // them, and a popover's content isn't inside its glass view but beside it.
        let content = msg!(Id; window, c"contentView");
        if !content.is_null() {
            make_dark(content);
        }
    }
}

/// Gives a view or window the dark appearance, unless it has it already.
fn make_dark(thing: Id) {
    let dark = nsstring(c"NSAppearanceNameDarkAqua");
    let names = [nsstring(c"NSAppearanceNameAqua"), dark];
    let names = msg!(Id; class(c"NSArray"), c"arrayWithObjects:count:", names.as_ptr() => *const Id, 2usize => usize);
    let appearance = msg!(Id; thing, c"effectiveAppearance");
    let current = if appearance.is_null() { NIL } else { msg!(Id; appearance, c"bestMatchFromAppearancesWithNames:", names => Id) };
    if current.is_null() || !msg!(bool; current, c"isEqualToString:", dark => Id) {
        msg!((); thing, c"setAppearance:", msg!(Id; class(c"NSAppearance"), c"appearanceNamed:", dark => Id) => Id);
    }
}

/// Whether this glass sits on other glass: SwiftUI's has the outer `SDFLayer` above it; AppKit's
/// (or SwiftUI's in an `NSGlassEffectView`) has an `NSGlassEffectView` above it other than the one
/// it's the glass of, which holds it two layers up (the view's layer, then its hosting view's).
fn nested(kit: &Kit, sdf: Id) -> bool {
    let mut layer = msg!(Id; sdf, c"superlayer");
    for level in 1..=MAX_ANCESTORS {
        if layer.is_null() {
            return false;
        }
        if is_kind(layer, kit.sdf_layer_class) {
            return true;
        }
        if level > 2 && objc::responds(layer, c"delegate") && is_kind(msg!(Id; layer, c"delegate"), kit.glass_view) {
            return true;
        }
        layer = msg!(Id; layer, c"superlayer");
    }
    false
}

/// The layers under `layer` that belong to it, not to glass nested inside its content (which has
/// its own `SDFLayer` and its own pass).
fn for_each_sublayer(kit: &Kit, layer: Id, mut f: impl FnMut(Id)) {
    let sublayers = msg!(Id; layer, c"sublayers");
    if sublayers.is_null() {
        return;
    }
    let n = msg!(usize; sublayers, c"count");
    for i in 0..n {
        let sub = msg!(Id; sublayers, c"objectAtIndex:", i => usize);
        if !sub.is_null() && !is_kind(sub, kit.sdf_layer_class) {
            f(sub);
        }
    }
}

/// Takes the glass filter off every backdrop under `layer`; the first that had one, or nil.
fn strip_filters(kit: &Kit, layer: Id, depth: usize) -> Id {
    let mut found = NIL;
    if depth > 0 && is_kind(layer, kit.backdrop) && strip_glass_filter(kit, layer) {
        found = layer;
    }
    if depth < MAX_DEPTH {
        for_each_sublayer(kit, layer, |sub| {
            let under = strip_filters(kit, sub, depth + 1);
            if found.is_null() {
                found = under;
            }
        });
    }
    found
}

fn strip_glass_filter(kit: &Kit, backdrop: Id) -> bool {
    let filters = msg!(Id; backdrop, c"filters");
    if filters.is_null() {
        return false;
    }
    let n = msg!(usize; filters, c"count");
    let kept = msg!(Id; class(c"NSMutableArray"), c"arrayWithCapacity:", n => usize);
    let mut stripped = false;
    for i in 0..n {
        let filter = msg!(Id; filters, c"objectAtIndex:", i => usize);
        let kind = if objc::responds(filter, c"type") { msg!(Id; filter, c"type") } else { NIL };
        if !kind.is_null() && msg!(bool; kind, c"isEqual:", kit.glass_filter => Id) {
            stripped = true;
        } else {
            msg!((); kept, c"addObject:", filter => Id);
        }
    }
    if stripped {
        msg!((); backdrop, c"setFilters:", kept => Id);
    }
    stripped
}

/// Under a glass `SDFLayer`: the backdrop stops capturing, the shape's output effect becomes
/// `body`, highlights go.
fn paint(kit: &Kit, layer: Id, body: Id, depth: usize) {
    // Without its filter the backdrop would still show its capture (taken at a fraction of the
    // resolution, so blurred) in the corners of its rect around the shape.
    if depth > 0 && is_kind(layer, kit.backdrop) && objc::responds(layer, c"setEnabled:") && msg!(bool; layer, c"isEnabled") {
        msg!((); layer, c"setEnabled:", false => bool);
    }
    if depth > 0 && is_kind(layer, kit.sdf_layer) && objc::responds(layer, c"effect") {
        let effect = msg!(Id; layer, c"effect");
        if !effect.is_null() {
            let cls = unsafe { object_getClass(effect) };
            if cls == kit.output || ((cls == kit.body_effect || cls == kit.inner_effect) && cls != body) {
                unsafe { object_setClass(effect, body) };
                // Changing the class doesn't tell Core Animation the effect changed.
                if objc::responds(layer, c"reloadValueForKeyPath:") {
                    msg!((); layer, c"reloadValueForKeyPath:", nsstring(c"effect") => Id);
                }
            } else if kit.hidden.iter().any(|&h| is_kind(effect, h)) && !msg!(bool; layer, c"isHidden") {
                msg!((); layer, c"setHidden:", true => bool);
            }
        }
    }
    if depth < MAX_DEPTH {
        for_each_sublayer(kit, layer, |sub| paint(kit, sub, body, depth + 1));
    }
}

/// `-[NSToolTipManager installContentView:forToolTip:toolTipWindow:isNew:]`: a tooltip about to
/// show in `window`.
extern "C" fn install_tooltip(this: Id, cmd: Sel, view: Id, tooltip: Id, window: Id, new: bool) {
    let original: extern "C" fn(Id, Sel, Id, Id, Id, bool) = unsafe { transmute(TOOLTIP.get()) };
    original(this, cmd, view, tooltip, window, new);
    objc::autoreleasing(|| restyle_tooltip(window));
}

fn restyle_tooltip(window: Id) {
    if window.is_null() || !objc::responds(window, c"contentView") {
        return;
    }
    let effect = msg!(Id; window, c"contentView");
    if !is_kind(effect, class(c"NSVisualEffectView")) || !objc::responds(effect, c"_setClear:") {
        return;
    }
    msg!((); window, c"setOpaque:", false => bool);
    msg!((); window, c"setBackgroundColor:", msg!(Id; class(c"NSColor"), c"clearColor") => Id);
    let dark = msg!(Id; class(c"NSAppearance"), c"appearanceNamed:", nsstring(c"NSAppearanceNameDarkAqua") => Id);
    msg!((); window, c"setAppearance:", dark => Id);
    // The blur goes; what's left of the effect view is a plain container for the text.
    msg!((); effect, c"_setClear:", true => bool);

    let subviews = msg!(Id; effect, c"subviews");
    let first = msg!(Id; subviews, c"firstObject");
    let ours = !first.is_null()
        && msg!(bool; msg!(Id; first, c"identifier"), c"isEqualToString:", nsstring(TOOLTIP_BODY) => Id);
    if !ours {
        let bounds = msg!(CGRect; effect, c"bounds");
        let body = msg!(Id; msg!(Id; class(c"NSView"), c"alloc"), c"initWithFrame:", bounds => CGRect);
        msg!((); body, c"setIdentifier:", nsstring(TOOLTIP_BODY) => Id);
        msg!((); body, c"setWantsLayer:", true => bool);
        const WIDTH_AND_HEIGHT_SIZABLE: usize = 2 | 16;
        msg!((); body, c"setAutoresizingMask:", WIDTH_AND_HEIGHT_SIZABLE => usize);
        let layer = msg!(Id; body, c"layer");
        msg!((); layer, c"setBackgroundColor:", crate::style::palette().body => *mut c_void);
        msg!((); layer, c"setCornerRadius:", TOOLTIP_RADIUS => f64);
        if objc::responds(layer, c"setCornerCurve:") {
            msg!((); layer, c"setCornerCurve:", nsstring(c"continuous") => Id);
        }
        const BELOW: isize = -1;
        msg!((); effect, c"addSubview:positioned:relativeTo:", body => Id, BELOW => isize, NIL => Id);
        objc::release(body);
    }
    msg!((); window, c"invalidateShadow");
}
