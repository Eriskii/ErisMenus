//! Showing a menu and tracking it until something is chosen or it's dismissed: what
//! `presentPopup:` does in AppKit, with our panels.
//!
//! The menu's lifecycle runs through AppKit's own calls on its impl, in AppKit's order, so the
//! delegate and the app see what they'd see with AppKit's menus: `_sendMenuPopulate`
//! (`menuNeedsUpdate:`), `_sendMenuOpening` (`menuWillOpen:`), `_sendMenuEnableItems`
//! (validation), then on the way out `_sendMenuClosed` (`menuDidClose:`) and the chosen item
//! through `+[NSCocoaMenuImpl _performActionForMenuItem:invocationType:]`, which is also what lets
//! `popUpMenuPositioningItem:` report that something was chosen.

use alloc::string::String;
use alloc::vec::Vec;
use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

use menus_core::layout::Anchor;
use menus_core::nav;
use menus_core::spring::{Animation, Style};
use menus_core::{Point, Rect};

use crate::model::{self, Row};
use crate::{log, msg};
use eris_inject_payload::sys::SpinLock;
use crate::objc::{Id, NIL, autoreleasing, class, nsstring, release, responds, retain, string};
use crate::panel::Panel;
use crate::sys::{self, CGPoint, CGRect};

/// The impl of the menu we're tracking, NIL when none.
static ACTIVE: AtomicPtr<c_void> = AtomicPtr::new(core::ptr::null_mut());
static CANCELLED: AtomicBool = AtomicBool::new(false);
/// The tracked menu's highlighted item, for `-[NSMenu highlightedItem]`.
static HIGHLIGHTED: AtomicPtr<c_void> = AtomicPtr::new(core::ptr::null_mut());

/// How long a submenu waits before opening under the pointer, and before closing once the
/// pointer has moved to another row of its parent.
const OPEN_DELAY: f64 = 0.15;
const CLOSE_DELAY: f64 = 0.25;
/// Releasing the button doesn't choose until the pointer has moved this far or this long has
/// passed, so the click that opened a menu doesn't also choose from it.
const ARM_DISTANCE: f64 = 4.0;
const ARM_TIME: f64 = 0.35;
const TYPE_SELECT_RESET: f64 = 1.0;

/// `NSMenu.systemAttributes` bit AppKit sets on an opened menu; `menuDidClose:` only goes out if
/// it's set.
const OPENED: u64 = 1 << 2;

/// Whether we're tracking a menu right now.
pub fn active() -> bool {
    !ACTIVE.load(Ordering::Acquire).is_null()
}

/// `cancelTracking` on one of the menus we have open ends our tracking; on any other menu (the
/// menu bar's own, say) it's left to AppKit.
pub fn cancel(impl_: Id) -> bool {
    if !OPEN.with(|open| open.contains(&(impl_ as usize))) {
        return false;
    }
    CANCELLED.store(true, Ordering::Release);
    true
}

/// The system's request that every menu close (HIToolbox's cancel-tracking notification).
extern "C" fn close_all(_: *const c_void, _: *const c_void, _: *const c_void, _: *const c_void, _: *const c_void) {
    if active() {
        log!("the system asked every menu to close");
        CANCELLED.store(true, Ordering::Release);
    }
}

/// The impls of the menus we have open.
static OPEN: SpinLock<Vec<usize>> = SpinLock::new(Vec::new());

pub fn highlighted(impl_: Id) -> Option<Id> {
    let active = ACTIVE.load(Ordering::Acquire);
    (!active.is_null() && active == impl_).then(|| HIGHLIGHTED.load(Ordering::Acquire))
}

/// Shows and tracks the menu `context` describes; false to leave it to AppKit.
pub fn present(impl_: Id, _view: Id, context: Id) -> bool {
    if !ACTIVE.load(Ordering::Acquire).is_null() {
        log!("leaving a menu opened from inside another to AppKit");
        return false;
    }
    if disabled() {
        log!("menus are off in this app (ErisMenusDisabled)");
        return false;
    }
    let menu = msg!(Id; context, c"menu");
    if menu.is_null() {
        return false;
    }
    log!("drawing a menu of {} items", msg!(isize; menu, c"numberOfItems"));
    ACTIVE.store(impl_, Ordering::Release);
    CANCELLED.store(false, Ordering::Release);
    let menu = retain(menu);
    let tracking = sys::Tracking::begin(close_all);
    autoreleasing(|| run(impl_, menu, context));
    drop(tracking);
    release(menu);
    HIGHLIGHTED.store(core::ptr::null_mut(), Ordering::Release);
    ACTIVE.store(core::ptr::null_mut(), Ordering::Release);
    true
}

/// The app's menu bar while we track it: its titles on screen, and which one is open.
struct MenuBar {
    impl_: Id,
    titles: Vec<Title>,
    current: usize,
}

struct Title {
    /// The item's index in the main menu; -1 for the Apple menu, which isn't one of its items.
    index: isize,
    /// The main menu's item, or the Apple menu's (`+[NSMenuBarImpl appleMenuItem]`).
    item: Id,
    /// On screen.
    rect: Rect,
}

impl Title {
    /// Whether `p` is on this title. The top edge counts: the pointer sits on the screen's top row,
    /// y = the screen's height, when it's pushed up against the menu bar.
    fn hit(&self, p: Point) -> bool {
        let r = self.rect;
        p.x >= r.x && p.x < r.max_x() && p.y >= r.y && p.y <= r.max_y() + 1.0
    }
}

/// Tracks the menu bar from `event` (the click on a title, or the key that focused the menu bar)
/// instead of `NSMenuBarTrackingSession`; false to leave it to AppKit, as for the Apple menu, which
/// isn't one of the main menu's items.
pub fn track_menu_bar(event: Id) -> bool {
    if !ACTIVE.load(Ordering::Acquire).is_null() || disabled() {
        return false;
    }
    track_menu_bar_now(event)
}

fn track_menu_bar_now(event: Id) -> bool {
    let app = msg!(Id; class(c"NSApplication"), c"sharedApplication");
    let menu = msg!(Id; app, c"mainMenu");
    if menu.is_null() {
        return false;
    }
    let bar = msg!(Id; menu, c"_menuImpl");
    if bar.is_null() || !msg!(bool; bar, c"isKindOfClass:", class(c"NSMenuBarImpl") => Id) {
        return false;
    }
    let mouse: Point = msg!(CGPoint; class(c"NSEvent"), c"mouseLocation").into();
    let keyboard = !event.is_null() && msg!(usize; event, c"type") == KEY_DOWN;
    let titles = titles(bar, menu);
    let start = if keyboard {
        // Control-F2 highlights the Apple menu first; with nothing highlighted, start at the app's
        // menu.
        let highlighted = msg!(isize; bar, c"_indexOfHighlightedViewInMenu");
        let first = titles.iter().position(|t| t.index >= 0).unwrap_or(0);
        Some(titles.iter().position(|t| t.index == highlighted).unwrap_or(first))
    } else {
        // Only a click that's really on a title. AppKit hands this handler other clicks too, and
        // its own idea of the title under the pointer only goes by x.
        let clicked = (!event.is_null()).then(|| click_location(event)).flatten();
        [clicked, Some(mouse)].into_iter().flatten().find_map(|p| titles.iter().position(|t| t.hit(p)))
    };
    let Some(start) = start.filter(|_| !titles.is_empty()) else {
        if keyboard || titles.iter().any(|t| t.rect.x <= mouse.x && mouse.x < t.rect.max_x()) {
            log!("menu bar event at {:.0},{:.0} isn't on any of {} titles; leaving it to AppKit", mouse.x, mouse.y, titles.len());
        }
        return false;
    };
    log!(
        "menu bar {} at {:.0},{:.0}: opening {}",
        if keyboard { "focused" } else { "clicked" },
        mouse.x,
        mouse.y,
        match titles[start].index {
            -1 => String::from("the Apple menu"),
            index => alloc::format!("title {index}"),
        },
    );
    // On some paths AppKit has already opened the title's menu by now.
    if !msg!(Id; bar, c"deepestPresentedSubmenu").is_null() {
        msg!((); bar, c"dismissSubmenuAnimated:", false => bool);
    }
    ACTIVE.store(bar, Ordering::Release);
    CANCELLED.store(false, Ordering::Release);
    let menu = retain(menu);
    let tracking = sys::Tracking::begin(close_all);
    autoreleasing(|| run_menu_bar(bar, menu, titles, start, keyboard));
    drop(tracking);
    release(menu);
    HIGHLIGHTED.store(core::ptr::null_mut(), Ordering::Release);
    ACTIVE.store(core::ptr::null_mut(), Ordering::Release);
    true
}

/// Where on screen a mouse event happened.
fn click_location(event: Id) -> Option<Point> {
    let kind = msg!(usize; event, c"type");
    if !MOUSE_DOWN.contains(&kind) && !MOUSE_UP.contains(&kind) {
        return None;
    }
    let p = msg!(CGPoint; event, c"locationInWindow");
    let window = msg!(Id; event, c"window");
    let origin = if window.is_null() { CGPoint::default() } else { msg!(CGRect; window, c"frame").origin };
    Some(Point::new(origin.x + p.x, origin.y + p.y))
}

/// The menu bar's titles that open a menu, where the menu bar shows them on screen, left to right:
/// the Apple menu and the main menu's items.
fn titles(bar: Id, menu: Id) -> Vec<Title> {
    let representation = msg!(Id; bar, c"activeRepresentationView");
    if representation.is_null() {
        return Vec::new();
    }
    let apple = if responds(class(c"NSMenuBarImpl"), c"appleMenuItem") {
        msg!(Id; class(c"NSMenuBarImpl"), c"appleMenuItem")
    } else {
        NIL
    };
    let count = msg!(isize; menu, c"numberOfItems");
    let mut titles: Vec<Title> = (-1..count)
        .filter_map(|index| {
            let item = if index < 0 { apple } else { msg!(Id; menu, c"itemAtIndex:", index => isize) };
            if item.is_null() || msg!(bool; item, c"isHidden") || msg!(Id; item, c"submenu").is_null() {
                return None;
            }
            // The titles are views in the menu bar's own window (NSMenuBarReplicantWindow); the
            // Apple menu's is at index -1.
            let view = msg!(Id; representation, c"subviewAtMenuIndex:", index => isize);
            let window = if view.is_null() { NIL } else { msg!(Id; view, c"window") };
            if window.is_null() {
                return None;
            }
            let bounds = msg!(CGRect; view, c"bounds");
            let r = msg!(CGRect; view, c"convertRect:toView:", bounds => CGRect, NIL => Id);
            // The menu bar window's convertRectToScreen: answers as if it sat at the bottom of the
            // screen, but its frame is right.
            let origin = msg!(CGRect; window, c"frame").origin;
            let rect = Rect::new(origin.x + r.origin.x, origin.y + r.origin.y, r.size.width, r.size.height);
            (rect.width > 0.0 && rect.height > 0.0).then_some(Title { index, item, rect })
        })
        .collect();
    titles.sort_by(|a, b| a.rect.x.total_cmp(&b.rect.x));
    titles
}

fn run_menu_bar(bar: Id, menu: Id, titles: Vec<Title>, start: usize, keyboard: bool) {
    let center = msg!(Id; class(c"NSNotificationCenter"), c"defaultCenter");
    msg!((); center, c"postNotificationName:object:", nsstring(c"NSMenuDidBeginTrackingNotification") => Id, menu => Id);
    let modifiers = msg!(u64; class(c"NSEvent"), c"modifierFlags");
    let mut session = Session::new(Style::default().animation(), modifiers);
    session.bar = Some(MenuBar { impl_: bar, titles, current: start });
    if session.open_title(start, keyboard) {
        session.track();
    }
    let chosen = retain(session.chosen);
    while !session.levels.is_empty() {
        session.pop_level(true);
    }
    msg!((); bar, c"unhighlightItemIfNeeded");
    msg!((); center, c"postNotificationName:object:", nsstring(c"NSMenuDidEndTrackingNotification") => Id, menu => Id);
    if !chosen.is_null() {
        msg!((); class(c"NSCocoaMenuImpl"), c"_performActionForMenuItem:invocationType:", chosen => Id, 0u64 => u64);
        release(chosen);
    }
}

/// `defaults write <app> ErisMenusDisabled -bool YES` leaves an app's menus to AppKit.
fn disabled() -> bool {
    default(c"ErisMenusDisabled")
}

fn default(key: &core::ffi::CStr) -> bool {
    let defaults = msg!(Id; class(c"NSUserDefaults"), c"standardUserDefaults");
    msg!(bool; defaults, c"boolForKey:", nsstring(key) => Id)
}

struct Level {
    panel: Panel,
    impl_: Id,
    /// The row of the level above that this submenu belongs to.
    parent_row: Option<usize>,
}

struct Session {
    levels: Vec<Level>,
    /// The app's menu bar, when it's what we're tracking.
    bar: Option<MenuBar>,
    /// For driving menus from tests by keys alone (`ErisMenusIgnorePointer`).
    ignore_pointer: bool,
    animation: Animation,
    modifiers: u64,
    chosen: Id,
    done: bool,
    opened_at: f64,
    armed: bool,
    start_mouse: Point,
    mouse: Point,
    typed: String,
    typed_at: f64,
    /// (level, row, when): a submenu about to open.
    pending_open: Option<(usize, usize, f64)>,
    /// (levels to keep, when): submenus about to close.
    pending_close: Option<(usize, f64)>,
}

fn run(impl_: Id, menu: Id, context: Id) {
    open_lifecycle(impl_, menu);
    let center = class(c"NSNotificationCenter");
    let center = msg!(Id; center, c"defaultCenter");
    msg!((); center, c"postNotificationName:object:", nsstring(c"NSMenuDidBeginTrackingNotification") => Id, menu => Id);

    let modifiers = msg!(u64; class(c"NSEvent"), c"modifierFlags");
    let pulldown = msg!(u64; context, c"flags") & 8 != 0;
    let rows = model::rows(menu, pulldown, modifiers);
    let checked = msg!(isize; context, c"checkedItemIndex");
    let checked_row = (checked >= 0)
        .then(|| msg!(Id; menu, c"itemAtIndex:", checked => isize))
        .and_then(|item| rows.iter().position(|r| r.items().any(|i| i == item)));
    let align: Point = msg!(CGPoint; context, c"frameAlignmentPoint").into();
    let button = unflip(msg!(CGRect; context, c"popupButtonBounds").into());
    // What the menu mustn't cover. A menu bar extra's menu comes with the item's rect and a point
    // a little way up inside it, which AppKit's own menu window is drawn down from.
    let keep_clear: Rect = msg!(CGRect; context, c"exclusionRect").into();
    let anchor = match checked_row {
        Some(row) if !button.is_empty() => Anchor::Button { bounds: button, row },
        Some(row) => Anchor::Row { point: align, row },
        None if !keep_clear.is_empty() && keep_clear.contains(align) => Anchor::Below(keep_clear),
        None => Anchor::Corner(align),
    };
    let at = if button.is_empty() { align } else { Point::new(button.x, button.mid_y()) };
    let min_width = msg!(f64; context, c"minWidth").max(0.0);
    let animation = Style::default().animation();

    let Some(mut root) = Panel::open(menu, rows, anchor, min_width, visible_frame(at), animation) else {
        msg!((); context, c"setMenuCanceled:", true => bool);
        close_lifecycle(impl_);
        msg!((); center, c"postNotificationName:object:", nsstring(c"NSMenuDidEndTrackingNotification") => Id, menu => Id);
        return;
    };
    msg!((); context, c"setMenuOpened:", true => bool);
    if let Some(row) = checked_row {
        root.set_highlight(Some(row));
        HIGHLIGHTED.store(root.rows[row].item(), Ordering::Release);
    }

    let mut session = Session::new(animation, modifiers);
    session.levels.push(Level { panel: root, impl_, parent_row: None });
    session.track();

    let chosen = retain(session.chosen);
    while session.levels.len() > 1 {
        session.close_deepest(true);
    }
    let root = session.levels.pop().expect("the root level");
    root.panel.close(true);
    msg!((); context, c"setMenuCanceled:", chosen.is_null() => bool);
    close_lifecycle(impl_);
    msg!((); center, c"postNotificationName:object:", nsstring(c"NSMenuDidEndTrackingNotification") => Id, menu => Id);
    if !chosen.is_null() {
        msg!((); class(c"NSCocoaMenuImpl"), c"_performActionForMenuItem:invocationType:", chosen => Id, 0u64 => u64);
        release(chosen);
    }
}

fn open_lifecycle(impl_: Id, menu: Id) {
    OPEN.with(|open| open.push(impl_ as usize));
    for step in [c"_sendMenuPopulate", c"_sendMenuOpening", c"_sendMenuEnableItems"] {
        if responds(impl_, step) {
            msg!((); impl_, step);
        }
    }
    let attributes = msg!(u64; menu, c"systemAttributes");
    msg!((); menu, c"setSystemAttributes:", attributes | OPENED => u64);
    msg!((); impl_, c"setIsVisible:", true => bool);
}

fn close_lifecycle(impl_: Id) {
    OPEN.with(|open| open.retain(|&i| i != impl_ as usize));
    msg!((); impl_, c"setIsVisible:", false => bool);
    if responds(impl_, c"_sendMenuClosed") {
        msg!((); impl_, c"_sendMenuClosed");
    }
}

/// `popupButtonBounds` is in flipped screen space, from the top of the main display.
fn unflip(r: Rect) -> Rect {
    if r.is_empty() {
        return r;
    }
    let screens = msg!(Id; class(c"NSScreen"), c"screens");
    let main = msg!(Id; screens, c"firstObject");
    if main.is_null() {
        return r;
    }
    let height = msg!(CGRect; main, c"frame").size.height;
    Rect::new(r.x, height - r.y - r.height, r.width, r.height)
}

fn screen_at(p: Point) -> Option<Id> {
    let screens = msg!(Id; class(c"NSScreen"), c"screens");
    let count = msg!(usize; screens, c"count");
    (0..count).map(|i| msg!(Id; screens, c"objectAtIndex:", i => usize)).find(|&screen| {
        let frame: Rect = msg!(CGRect; screen, c"frame").into();
        frame.contains(p)
    })
}

/// The visible frame of the screen `p` is on.
fn visible_frame(p: Point) -> Rect {
    let screen = screen_at(p).unwrap_or_else(|| msg!(Id; class(c"NSScreen"), c"mainScreen"));
    msg!(CGRect; screen, c"visibleFrame").into()
}

const MOUSE_DOWN: [usize; 3] = [1, 3, 25];
const MOUSE_UP: [usize; 3] = [2, 4, 26];
const KEY_DOWN: usize = 10;
const FLAGS_CHANGED: usize = 12;
const APPKIT_DEFINED: usize = 13;
const SYSTEM_DEFINED: usize = 14;
const APPLICATION_DEFINED: usize = 15;
const SCROLL_WHEEL: usize = 22;
const APPLICATION_DEACTIVATED: i16 = 2;

impl Session {
    fn new(animation: Animation, modifiers: u64) -> Self {
        let mouse: Point = msg!(CGPoint; class(c"NSEvent"), c"mouseLocation").into();
        Session {
            levels: Vec::new(),
            bar: None,
            ignore_pointer: default(c"ErisMenusIgnorePointer"),
            animation,
            modifiers,
            chosen: NIL,
            done: false,
            opened_at: sys::now(),
            armed: false,
            start_mouse: mouse,
            mouse,
            typed: String::new(),
            typed_at: 0.0,
            pending_open: None,
            pending_close: None,
        }
    }

    /// The menu bar title under `p`.
    fn title_at(&self, p: Point) -> Option<usize> {
        self.bar.as_ref()?.titles.iter().position(|t| t.hit(p))
    }

    /// Opens the menu of menu bar title `t` in place of whatever's open; false if it has nothing
    /// to show.
    fn open_title(&mut self, t: usize, keyboard: bool) -> bool {
        let Some(bar) = &self.bar else { return false };
        let (bar_impl, title) = (bar.impl_, &bar.titles[t]);
        let (index, rect) = (title.index, title.rect);
        let submenu = msg!(Id; title.item, c"submenu");
        if submenu.is_null() {
            return false;
        }
        let impl_ = msg!(Id; submenu, c"_menuImpl");
        open_lifecycle(impl_, submenu);
        let rows = model::rows(submenu, false, self.modifiers);
        // Hang from the bottom of the title, right up against the menu bar.
        let screen = visible_frame(Point::new(rect.x, rect.y - 1.0));
        let Some(panel) = Panel::open(submenu, rows, Anchor::Below(rect), 0.0, screen, self.animation) else {
            log!("menu bar title {index} has nothing to show");
            close_lifecycle(impl_);
            return false;
        };
        while !self.levels.is_empty() {
            self.pop_level(false);
        }
        self.levels.push(Level { panel, impl_, parent_row: None });
        self.pending_open = None;
        self.pending_close = None;
        if let Some(bar) = &mut self.bar {
            bar.current = t;
        }
        let representation = msg!(Id; bar_impl, c"activeRepresentationView");
        if !representation.is_null() {
            let view = msg!(Id; representation, c"subviewAtMenuIndex:", index => isize);
            msg!((); bar_impl, c"setHighlightedView:", view => Id);
        }
        if keyboard {
            self.to_end(0, 1);
        }
        true
    }

    /// Opens the menu bar title `step` titles along from the open one, wrapping around.
    fn step_title(&mut self, step: isize) {
        let Some(bar) = &self.bar else { return };
        let n = bar.titles.len() as isize;
        let t = (bar.current as isize + step).rem_euclid(n) as usize;
        self.open_title(t, true);
    }

    fn track(&mut self) {
        let app = msg!(Id; class(c"NSApplication"), c"sharedApplication");
        let mode = nsstring(c"NSEventTrackingRunLoopMode");
        let mut last = sys::now();
        let mut moving = true;
        while !self.done {
            autoreleasing(|| {
                let wait = if moving || self.pending_open.is_some() || self.pending_close.is_some() {
                    1.0 / 120.0
                } else {
                    1.0 / 60.0
                };
                let until = msg!(Id; class(c"NSDate"), c"dateWithTimeIntervalSinceNow:", wait => f64);
                let event = msg!(Id; app, c"nextEventMatchingMask:untilDate:inMode:dequeue:",
                    u64::MAX => u64, until => Id, mode => Id, true => bool);
                if !event.is_null() {
                    self.handle(app, event);
                }
                let mouse: Point = msg!(CGPoint; class(c"NSEvent"), c"mouseLocation").into();
                if mouse != self.mouse && !self.ignore_pointer {
                    self.mouse = mouse;
                    self.pointer_moved();
                }
                self.run_timers();
                if CANCELLED.load(Ordering::Acquire) {
                    self.finish("the app cancelled it");
                }
                let now = sys::now();
                let dt = (now - last).min(1.0 / 30.0);
                last = now;
                moving = false;
                for level in &mut self.levels {
                    moving |= level.panel.tick(dt);
                }
            });
        }
    }

    fn handle(&mut self, app: Id, event: Id) {
        let kind = msg!(usize; event, c"type");
        if self.forward_to_item_view(kind, event) {
            return;
        }
        match kind {
            k if MOUSE_DOWN.contains(&k) && !self.ignore_pointer => self.mouse_down(),
            k if MOUSE_UP.contains(&k) && !self.ignore_pointer => self.mouse_up(),
            KEY_DOWN => self.key_down(event),
            FLAGS_CHANGED => {
                self.modifiers = msg!(u64; event, c"modifierFlags");
                for level in &mut self.levels {
                    level.panel.show_variants_for(self.modifiers);
                }
            }
            SCROLL_WHEEL => {
                let mut dy = msg!(f64; event, c"scrollingDeltaY");
                if !msg!(bool; event, c"hasPreciseScrollingDeltas") {
                    dy *= 16.0;
                }
                if let Some(level) = self.level_at(self.mouse) {
                    self.levels[level].panel.scroll_by(-dy);
                }
            }
            APPKIT_DEFINED => {
                if msg!(i16; event, c"subtype") == APPLICATION_DEACTIVATED {
                    self.finish("the app lost focus");
                }
                msg!((); app, c"sendEvent:", event => Id);
            }
            SYSTEM_DEFINED | APPLICATION_DEFINED => msg!((); app, c"sendEvent:", event => Id),
            _ => {}
        }
    }

    /// Hands a mouse event on an item that draws itself with a view (Finder's tag colors, say) to
    /// that view, as AppKit's menus do.
    fn forward_to_item_view(&mut self, kind: usize, event: Id) -> bool {
        const MOUSE_MOVED: usize = 5;
        const DRAGGED: [usize; 3] = [6, 7, 27];
        if !MOUSE_DOWN.contains(&kind) && !MOUSE_UP.contains(&kind) && kind != MOUSE_MOVED && !DRAGGED.contains(&kind) {
            return false;
        }
        let window = msg!(Id; event, c"window");
        let Some(level) = self.levels.iter().find(|l| l.panel.window() == window) else { return false };
        let target = level.panel.item_view_hit(msg!(CGPoint; event, c"locationInWindow"));
        if target.is_null() {
            return false;
        }
        // Straight to the view, as AppKit's menu windows do: our panel never becomes key, and a
        // window that isn't key wouldn't pass a first click on.
        let selector = match kind {
            1 => c"mouseDown:",
            2 => c"mouseUp:",
            3 => c"rightMouseDown:",
            4 => c"rightMouseUp:",
            25 => c"otherMouseDown:",
            26 => c"otherMouseUp:",
            5 => c"mouseMoved:",
            6 => c"mouseDragged:",
            7 => c"rightMouseDragged:",
            _ => c"otherMouseDragged:",
        };
        msg!((); target, selector, event => Id);
        true
    }

    /// The deepest level whose body is under `p`.
    fn level_at(&self, p: Point) -> Option<usize> {
        (0..self.levels.len()).rev().find(|&l| self.levels[l].panel.contains(p))
    }

    fn pointer_moved(&mut self) {
        let p = self.mouse;
        let (dx, dy) = (p.x - self.start_mouse.x, p.y - self.start_mouse.y);
        if dx * dx + dy * dy > ARM_DISTANCE * ARM_DISTANCE {
            self.armed = true;
        }
        if let Some(t) = self.title_at(p) {
            if self.bar.as_ref().is_some_and(|b| b.current != t) {
                self.open_title(t, false);
            }
            return;
        }
        let now = sys::now();
        match self.level_at(p) {
            Some(l) => {
                let panel = &self.levels[l].panel;
                let row = panel.row_at(p).filter(|&r| panel.selectable(r));
                if self.pending_close.is_some_and(|(keep, _)| l >= keep) {
                    self.pending_close = None;
                }
                if l + 1 < self.levels.len() {
                    if row.is_some() && row == self.levels[l + 1].parent_row {
                        self.pending_close = None;
                    } else if self.pending_close.is_none() {
                        self.pending_close = Some((l + 1, now + CLOSE_DELAY));
                    }
                }
                self.highlight(l, row);
                self.pending_open = match row {
                    Some(r) if !self.levels[l].panel.rows[r].submenu().is_null() && !self.is_open(l, r) => {
                        Some((l, r, now + OPEN_DELAY))
                    }
                    _ => None,
                };
            }
            None => {
                let deepest = self.levels.len() - 1;
                self.highlight(deepest, None);
                self.pending_open = None;
            }
        }
    }

    fn is_open(&self, level: usize, row: usize) -> bool {
        self.levels.get(level + 1).is_some_and(|child| child.parent_row == Some(row))
    }

    fn run_timers(&mut self) {
        let now = sys::now();
        if !self.armed && now - self.opened_at > ARM_TIME {
            self.armed = true;
        }
        if let Some((keep, at)) = self.pending_close {
            if now >= at {
                self.pending_close = None;
                while self.levels.len() > keep {
                    self.close_deepest(false);
                }
            }
        }
        if let Some((level, row, at)) = self.pending_open {
            if now >= at && self.pending_close.is_none() {
                self.pending_open = None;
                self.open_submenu(level, row, false);
            }
        }
    }

    fn mouse_down(&mut self) {
        if let Some(t) = self.title_at(self.mouse) {
            // Clicking the open title closes its menu, as in AppKit.
            if self.bar.as_ref().is_some_and(|b| b.current == t) {
                self.finish("its title was clicked again");
            } else {
                self.open_title(t, false);
            }
            return;
        }
        match self.level_at(self.mouse) {
            None => self.finish("a click outside it"),
            Some(l) => {
                if let Some(r) = self.levels[l].panel.row_at(self.mouse) {
                    if self.levels[l].panel.selectable(r) && !self.levels[l].panel.rows[r].submenu().is_null() {
                        self.open_submenu(l, r, false);
                    }
                }
            }
        }
    }

    fn mouse_up(&mut self) {
        if !self.armed || self.title_at(self.mouse).is_some() {
            return;
        }
        if let Some(l) = self.level_at(self.mouse) {
            if let Some(r) = self.levels[l].panel.row_at(self.mouse) {
                self.activate(l, r, false);
            }
        }
    }

    fn key_down(&mut self, event: Id) {
        const COMMAND: u64 = 1 << 20;
        let active = self.levels.len() - 1;
        let code = msg!(u16; event, c"keyCode");
        let highlighted = self.levels[active].panel.highlighted;
        match code {
            125 => self.step(active, 1),
            126 => self.step(active, -1),
            115 | 116 => self.to_end(active, 1),
            119 | 121 => self.to_end(active, -1),
            124 => match highlighted {
                Some(r) if !self.levels[active].panel.rows[r].submenu().is_null() => self.open_submenu(active, r, true),
                _ => self.step_title(1),
            },
            123 => {
                if self.levels.len() > 1 {
                    self.close_deepest(false);
                } else {
                    self.step_title(-1);
                }
            }
            36 | 76 => {
                if let Some(r) = highlighted {
                    self.activate(active, r, true);
                }
            }
            53 => self.finish("Escape"),
            49 if self.typed.is_empty() || sys::now() - self.typed_at > TYPE_SELECT_RESET => {
                if let Some(r) = highlighted {
                    self.activate(active, r, true);
                }
            }
            _ if msg!(u64; event, c"modifierFlags") & COMMAND == 0 => {
                let typed = string(msg!(Id; event, c"characters"));
                if typed.chars().any(|c| !c.is_control()) {
                    self.type_select(active, &typed);
                }
            }
            _ => {}
        }
    }

    fn type_select(&mut self, level: usize, typed: &str) {
        let now = sys::now();
        if now - self.typed_at > TYPE_SELECT_RESET {
            self.typed.clear();
        }
        self.typed.push_str(typed);
        self.typed_at = now;
        let panel = &self.levels[level].panel;
        let titles: Vec<String> = panel.rows.iter().map(|r| title(r)).collect();
        if let Some(r) = nav::type_select(titles.iter().map(String::as_str), &self.typed, |i| panel.selectable(i)) {
            self.highlight(level, Some(r));
        }
    }

    fn step(&mut self, level: usize, direction: isize) {
        let panel = &self.levels[level].panel;
        let next = nav::step(panel.rows.len(), panel.highlighted, direction, |i| panel.selectable(i));
        self.highlight(level, next);
        self.pending_open = None;
    }

    fn to_end(&mut self, level: usize, direction: isize) {
        let panel = &self.levels[level].panel;
        let row = nav::end(panel.rows.len(), direction, |i| panel.selectable(i));
        self.highlight(level, row);
    }

    /// Chooses row `r` of `level`, or opens its submenu.
    fn activate(&mut self, level: usize, r: usize, keyboard: bool) {
        let panel = &self.levels[level].panel;
        if !panel.selectable(r) {
            return;
        }
        if !panel.rows[r].submenu().is_null() {
            if !self.is_open(level, r) {
                self.open_submenu(level, r, keyboard);
            }
            return;
        }
        self.chosen = panel.rows[r].item();
        self.finish("an item was chosen");
    }

    fn finish(&mut self, why: &str) {
        if !self.done {
            log!("menu closed: {why}");
        }
        self.done = true;
    }

    fn highlight(&mut self, level: usize, row: Option<usize>) {
        let panel = &mut self.levels[level].panel;
        if panel.highlighted == row {
            return;
        }
        panel.set_highlight(row);
        let item = row.map_or(NIL, |r| panel.rows[r].item());
        if level == 0 {
            HIGHLIGHTED.store(item, Ordering::Release);
        }
        if responds(panel.menu, c"_informDelegateOfHighlightedItem:") {
            msg!((); panel.menu, c"_informDelegateOfHighlightedItem:", item => Id);
        }
    }

    fn open_submenu(&mut self, level: usize, row: usize, keyboard: bool) {
        self.pending_close = None;
        while self.levels.len() > level + 1 {
            self.close_deepest(false);
        }
        self.highlight(level, Some(row));
        let parent = &self.levels[level].panel;
        let submenu = parent.rows[row].submenu();
        if submenu.is_null() {
            return;
        }
        let impl_ = msg!(Id; submenu, c"_menuImpl");
        open_lifecycle(impl_, submenu);
        let rows = model::rows(submenu, false, self.modifiers);
        let row_rect = parent.row_rect(row);
        let anchor = Anchor::Beside { row: row_rect, parent: parent.frame };
        let screen = visible_frame(Point::new(row_rect.x, row_rect.mid_y()));
        match Panel::open(submenu, rows, anchor, 0.0, screen, self.animation) {
            Some(panel) => {
                self.levels.push(Level { panel, impl_, parent_row: Some(row) });
                if keyboard {
                    self.to_end(level + 1, 1);
                }
            }
            None => close_lifecycle(impl_),
        }
    }

    fn close_deepest(&mut self, fade: bool) {
        if self.levels.len() > 1 {
            self.pop_level(fade);
        }
    }

    /// Closes the deepest open menu, even the root (the menu bar's menus own theirs).
    fn pop_level(&mut self, fade: bool) {
        if let Some(level) = self.levels.pop() {
            level.panel.close(fade);
            close_lifecycle(level.impl_);
        }
    }
}

fn title(row: &Row) -> String {
    string(msg!(Id; row.item(), c"title"))
}
