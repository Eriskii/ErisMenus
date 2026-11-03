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
    if hooks::install() {
        log!("drawing menus in pid {pid}");
    }
}
