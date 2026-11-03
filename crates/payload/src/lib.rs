//! Loaded into GUI apps: AppKit still decides what goes in a menu and what choosing an item does,
//! but ErisMenus draws the menu and tracks it.
//!
//! Every NSMenu that pops up (context menus, `popUpMenuPositioningItem:`, pop-up and pull-down
//! buttons) goes through `+[NSContextMenuImpl presentPopup:fromView:withContext:animated:]`, which
//! fills the menu, shows it, runs its tracking loop and sends the chosen item's action. [`hooks`]
//! replaces it, [`session`] does the same job with our own panels: the menu's delegate hears
//! everything it would from AppKit, and the chosen item goes back through AppKit to be performed.
//!
//! The `erismenus` daemon maps this dylib into apps with eris-inject; during development it's
//! just `dlopen`ed (see `tools/devhost.m`). Either way it runs on eris-inject-payload's `no_std`
//! runtime, which is what makes it safe inside arm64e apps.
#![no_std]

extern crate alloc;

pub(crate) use eris_inject_payload::{log, msg, objc, rt};

mod glass;
mod glass_panel;
mod hooks;
mod model;
mod panel;
mod session;
mod style;
mod sys;

eris_inject_payload::payload!(name: "erismenus", log: "ErisMenus", start: start);

fn start(injected: bool) {
    let pid = unsafe { rt::getpid() };
    // The daemon loads us as soon as an app starts, which can be before AppKit is up.
    let wait_for_appkit_ms = if injected { 10_000 } else { 0 };
    let mut waited = 0;
    while objc::class(c"NSContextMenuImpl").is_null() {
        if waited >= wait_for_appkit_ms {
            log!("no AppKit menus in pid {pid}; nothing to do");
            return;
        }
        unsafe { rt::usleep(50_000) };
        waited += 50;
    }
    // Only processes that become apps show menus; plenty of others link AppKit and never do. The
    // daemon starts us while the app is still starting up, so give it a while.
    // (Loaded by dyld, we're on the main thread before the app exists; the main queue waits for it.)
    let mut waited = 0;
    while injected && !has_app() {
        if waited >= 30_000 {
            log!("no NSApplication in pid {pid}; nothing to do");
            return;
        }
        unsafe { rt::usleep(100_000) };
        waited += 100;
    }
    // Hooks go in on the main thread, once the app is running.
    eris_inject_payload::sys::on_main(install);
}

/// Whether AppKit's `NSApp` has been made.
fn has_app() -> bool {
    unsafe extern "C" {
        fn dlsym(handle: *mut core::ffi::c_void, symbol: *const core::ffi::c_char) -> *mut core::ffi::c_void;
    }
    const RTLD_DEFAULT: *mut core::ffi::c_void = -2isize as *mut core::ffi::c_void;
    let app = unsafe { dlsym(RTLD_DEFAULT, c"NSApp".as_ptr()) } as *const objc::Id;
    !app.is_null() && !unsafe { core::ptr::read_volatile(app) }.is_null()
}

extern "C" fn install(_: *mut core::ffi::c_void) {
    if hooks::install() {
        log!("drawing menus in pid {}", unsafe { rt::getpid() });
        eris_inject_payload::sys::on_main_after(5.0, verify);
    }
    glass::install();
}

extern "C" fn verify(_: *mut core::ffi::c_void) {
    hooks::verify();
}
