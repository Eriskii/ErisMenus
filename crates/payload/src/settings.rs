//! The settings, kept current from the state of a Darwin notification that `erismenus` publishes
//! (works in sandboxed apps too). Until something is published, the defaults.

use core::ffi::{CStr, c_int, c_void};
use core::sync::atomic::{AtomicI32, AtomicU64, Ordering};

use eris_inject_payload::sys;
use menus_core::settings::Settings;

use crate::accent;

/// Posted, with the packed settings as its state, whenever they change.
pub const NOTIFICATION: &CStr = c"io.github.eriskii.erismenus.settings";

static PACKED: AtomicU64 = AtomicU64::new(0);
static TOKEN: AtomicI32 = AtomicI32::new(-1);

pub fn current() -> Settings {
    Settings::unpack(PACKED.load(Ordering::Relaxed)).unwrap_or_default()
}

/// Reads the settings and follows them from now on. Main thread, once the app is up.
pub fn listen() {
    let mut token: c_int = -1;
    if unsafe { sys::notify_register_check(NOTIFICATION.as_ptr(), &mut token) } == 0 {
        TOKEN.store(token, Ordering::Relaxed);
        adopt_state();
    }
    sys::observe_darwin(NOTIFICATION, changed);
    accent::apply(current().accent);
}

fn adopt_state() {
    let mut state = 0;
    let token = TOKEN.load(Ordering::Relaxed);
    if token >= 0 && unsafe { sys::notify_get_state(token, &mut state) } == 0 && Settings::unpack(state).is_some() {
        PACKED.store(state, Ordering::Relaxed);
    }
}

extern "C" fn changed(_: *mut c_void, _: *const c_void, _: *const c_void, _: *const c_void, _: *const c_void) {
    adopt_state();
    sys::on_main(apply);
}

/// Menus, glass and the animation read the settings each time they're used; the accent is the
/// one thing that has to be pushed into AppKit.
extern "C" fn apply(_: *mut c_void) {
    crate::objc::autoreleasing(|| accent::apply(current().accent));
}
