//! The AppKit methods we replace (all private, checked against macOS 26.4).

use core::mem::transmute;
use core::sync::atomic::{AtomicBool, Ordering};

use core::ffi::CStr;

use crate::objc::{self, Id, Imp, Original, Sel, class, define_class, swizzle, swizzle_class};

/// A class only we define, so a second copy of the payload can tell it's not the first.
const LOADED_MARKER: &core::ffi::CStr = c"ErisMenusLoaded";
use crate::{log, msg, session};

/// `+[NSContextMenuImpl presentPopup:fromView:withContext:animated:]`: everything a popped-up menu
/// does, from filling it to performing the chosen item. `popup` is the menu's impl and `context`
/// its `NSMenuContext`, which says where the menu goes.
static PRESENT: Original = Original::new();

extern "C" fn present_popup(class: Id, cmd: Sel, popup: Id, view: Id, context: Id, animated: bool) {
    if session::present(popup, view, context) {
        return;
    }
    let original: extern "C" fn(Id, Sel, Id, Id, Id, bool) = unsafe { transmute(PRESENT.get()) };
    original(class, cmd, popup, view, context, animated)
}

/// `-[NSCocoaMenuImpl _cancelTrackingWithAnimation:]`, behind `-[NSMenu cancelTracking]`.
static CANCEL: Original = Original::new();

extern "C" fn cancel_tracking(this: Id, cmd: Sel, animated: bool) {
    if session::cancel(this) {
        return;
    }
    let original: extern "C" fn(Id, Sel, bool) = unsafe { transmute(CANCEL.get()) };
    original(this, cmd, animated)
}

/// `-[NSCocoaMenuImpl highlightedItem]`, behind `-[NSMenu highlightedItem]`.
static HIGHLIGHTED: Original = Original::new();

extern "C" fn highlighted_item(this: Id, cmd: Sel) -> Id {
    if let Some(item) = session::highlighted(this) {
        return item;
    }
    let original: extern "C" fn(Id, Sel) -> Id = unsafe { transmute(HIGHLIGHTED.get()) };
    original(this, cmd)
}

/// `-[NSMenuTrackingSession startMonitoringEvents:]`: on a `NSMenuBarTrackingSession`, the whole
/// of tracking the menu bar after a click on a title (or the key that focuses it), run as a modal
/// loop.
static MONITOR: Original = Original::new();

extern "C" fn start_monitoring_events(this: Id, cmd: Sel, event: Id) {
    let menu_bar = msg!(bool; this, c"isKindOfClass:", class(c"NSMenuBarTrackingSession") => Id);
    if menu_bar && session::track_menu_bar(event) {
        return;
    }
    let original: extern "C" fn(Id, Sel, Id) = unsafe { transmute(MONITOR.get()) };
    original(this, cmd, event)
}

/// `-[NSMenuBarTrackingSession _mouseDownEventHandler:]`: a click on a menu bar title. AppKit
/// opens the title's menu here, before `startMonitoringEvents:` runs its loop, so a click has to be
/// taken over here.
static MOUSE_DOWN: Original = Original::new();

extern "C" fn menu_bar_mouse_down(this: Id, cmd: Sel, event: Id) -> bool {
    // A click while we track (on another title, say) is ours to handle: unhandled here, it comes
    // back to our loop. AppKit would open its own menu over ours, and throw.
    if session::active() {
        return false;
    }
    if session::track_menu_bar(event) {
        return true;
    }
    let original: extern "C" fn(Id, Sel, Id) -> bool = unsafe { transmute(MOUSE_DOWN.get()) };
    original(this, cmd, event)
}

/// `-[NSMenuBarTrackingSession _handleMonitorEvent:]`: AppKit's local event monitor for the menu
/// bar, which sees every event our loop dequeues. It stands aside while we track.
static MONITOR_EVENT: Original = Original::new();

extern "C" fn menu_bar_monitor_event(this: Id, cmd: Sel, event: Id) -> bool {
    if session::active() {
        return false;
    }
    let original: extern "C" fn(Id, Sel, Id) -> bool = unsafe { transmute(MONITOR_EVENT.get()) };
    original(this, cmd, event)
}

static INSTALLED: AtomicBool = AtomicBool::new(false);

struct Hook {
    class: &'static CStr,
    method: &'static CStr,
    class_method: bool,
    imp: Imp,
    original: &'static Original,
}

fn hooks() -> [Hook; 6] {
    let hook = |class, method, class_method, imp, original| Hook { class, method, class_method, imp, original };
    [
        hook(c"NSContextMenuImpl", c"presentPopup:fromView:withContext:animated:", true, present_popup as Imp, &PRESENT),
        hook(c"NSCocoaMenuImpl", c"_cancelTrackingWithAnimation:", false, cancel_tracking as Imp, &CANCEL),
        hook(c"NSCocoaMenuImpl", c"highlightedItem", false, highlighted_item as Imp, &HIGHLIGHTED),
        hook(c"NSMenuTrackingSession", c"startMonitoringEvents:", false, start_monitoring_events as Imp, &MONITOR),
        hook(c"NSMenuBarTrackingSession", c"_mouseDownEventHandler:", false, menu_bar_mouse_down as Imp, &MOUSE_DOWN),
        hook(c"NSMenuBarTrackingSession", c"_handleMonitorEvent:", false, menu_bar_monitor_event as Imp, &MONITOR_EVENT),
    ]
}

impl Hook {
    fn name(&self) -> alloc::string::String {
        let kind = if self.class_method { '+' } else { '-' };
        alloc::format!("{kind}[{} {}]", self.class.to_str().unwrap_or("?"), self.method.to_str().unwrap_or("?"))
    }

    fn apply(&self) -> bool {
        if self.class_method {
            swizzle_class(self.class, self.method, self.imp, self.original)
        } else {
            swizzle(self.class, self.method, self.imp, self.original)
        }
    }

    fn in_place(&self) -> bool {
        objc::implementation(self.class, self.method, self.class_method) == Some(self.imp)
    }
}

/// Replaces AppKit's menu methods with ours. Run on the main thread once the app is up. False if
/// we were already installed, or AppKit lacks the menu presentation we replace.
pub fn install() -> bool {
    // Another copy of us may already be in this app (the daemon restarted, say): it has the menus.
    if INSTALLED.swap(true, Ordering::AcqRel) || define_class(LOADED_MARKER, c"NSObject", &[]).is_null() {
        log!("already running");
        return false;
    }
    let hooks = hooks();
    if !hooks[0].apply() {
        log!("no {}; leaving menus alone", hooks[0].name());
        return false;
    }
    for hook in &hooks[1..] {
        if !hook.apply() {
            log!("missing {}; that part is skipped", hook.name());
        }
    }
    true
}

/// Checks every hook is still ours, puts back any that something replaced, and says so.
pub fn verify() {
    let mut fine = 0;
    for hook in hooks() {
        if hook.in_place() {
            fine += 1;
        } else if objc::implementation(hook.class, hook.method, hook.class_method).is_some() {
            log!("{} was replaced after we hooked it; hooking it again", hook.name());
            hook.apply();
        }
    }
    log!("{fine} of 6 menu hooks in place");
}
