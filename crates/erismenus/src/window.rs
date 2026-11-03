//! The settings window: what `erismenus settings` and the ErisMenus app in /Applications open.
//! Every change is saved to the settings file and reaches every app at once.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use block2::RcBlock;
use menus_core::settings::{STYLES, Settings, components, parse_hex};
use menus_core::spring::Style;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, ProtocolObject, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAppearanceCustomization, NSApplication, NSApplicationActivationPolicy, NSBeep, NSBitmapImageFileType, NSApplicationDelegate, NSBackingStoreType, NSButton, NSColor,
    NSColorSpace, NSColorWell, NSColorWellStyle, NSControl, NSControlStateValueOff, NSControlStateValueOn, NSFont,
    NSGridCell, NSGridCellPlacement, NSGridRowAlignment, NSGridView, NSMenu, NSMenuItem, NSPopUpButton, NSStackView,
    NSTextField, NSView, NSWindow, NSWindowStyleMask, NSWorkspace,
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::{NSArray, NSDictionary, NSObject, NSObjectProtocol, NSString, NSTimer, NSURL, ns_string};

use crate::config;

define_class!(
    /// A control's or menu item's target, running a closure.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "ErisMenusSettingsAction"]
    #[ivars = Box<dyn Fn()>]
    struct Action;

    impl Action {
        #[unsafe(method(run:))]
        fn run(&self, _sender: Option<&AnyObject>) {
            (self.ivars())();
        }
    }
);

impl Action {
    fn new(mtm: MainThreadMarker, action: impl Fn() + 'static) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Box::new(action) as Box<dyn Fn()>);
        unsafe { msg_send![super(this), init] }
    }
}

define_class!(
    /// Quits once the window is closed.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "ErisMenusSettingsDelegate"]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    unsafe impl NSApplicationDelegate for Delegate {
        #[unsafe(method(applicationShouldTerminateAfterLastWindowClosed:))]
        fn last_window_closed(&self, _app: &NSApplication) -> bool {
            true
        }
    }
);

impl Delegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        unsafe { msg_send![Self::alloc(mtm), init] }
    }
}

/// The settings as the window shows them, and the file they go to.
struct State {
    path: PathBuf,
    settings: Cell<Settings>,
    /// A color being dragged around the color panel goes out once it rests.
    pending: RefCell<Option<Retained<NSTimer>>>,
}

impl State {
    fn edit(&self, change: impl FnOnce(&mut Settings)) {
        let mut settings = self.settings.get();
        change(&mut settings);
        self.settings.set(settings);
        if let Err(e) = config::save(&self.path, &settings) {
            eprintln!("erismenus: {e}");
        }
    }

    /// A color reaches every app (each redraws its controls and icons), so a color dragged across
    /// the color panel is sent when it settles rather than at every step.
    fn edit_soon(self: &Rc<Self>, change: impl Fn(&mut Settings) + 'static) {
        if let Some(timer) = self.pending.borrow_mut().take() {
            timer.invalidate();
        }
        let me = Rc::downgrade(self);
        let block = RcBlock::new(move |_: std::ptr::NonNull<NSTimer>| {
            if let Some(state) = me.upgrade() {
                state.pending.borrow_mut().take();
                state.edit(&change);
            }
        });
        let timer = unsafe { NSTimer::scheduledTimerWithTimeInterval_repeats_block(0.15, false, &block) };
        *self.pending.borrow_mut() = Some(timer);
    }
}

/// A color setting: a checkbox that turns it on, a color well and a hex field.
#[derive(Clone)]
struct ColorPicker {
    checkbox: Retained<NSButton>,
    well: Retained<NSColorWell>,
    hex: Retained<NSTextField>,
    get: fn(&Settings) -> Option<u32>,
    set: fn(&mut Settings, Option<u32>),
    /// The last color, kept while it's off so turning it back on returns it.
    last: Rc<Cell<u32>>,
}

impl ColorPicker {
    fn new(mtm: MainThreadMarker, title: &str, get: fn(&Settings) -> Option<u32>, set: fn(&mut Settings, Option<u32>), first: u32) -> Self {
        let well = NSColorWell::colorWellWithStyle(NSColorWellStyle::Default, mtm);
        well.widthAnchor().constraintEqualToConstant(44.0).setActive(true);
        let hex = NSTextField::textFieldWithString(&NSString::from_str(""), mtm);
        hex.setPlaceholderString(Some(ns_string!("#FF6B00")));
        hex.widthAnchor().constraintEqualToConstant(90.0).setActive(true);
        Self { checkbox: checkbox(mtm, title), well, hex, get, set, last: Rc::new(Cell::new(first)) }
    }

    fn row(&self, mtm: MainThreadMarker) -> Retained<NSView> {
        let views: [&NSView; 2] = [&self.well, &self.hex];
        let stack = NSStackView::stackViewWithViews(&NSArray::from_slice(&views), mtm);
        stack.setSpacing(8.0);
        Retained::into_super(stack)
    }

    fn show(&self, settings: &Settings) {
        let value = (self.get)(settings);
        if let Some(rgb) = value {
            self.last.set(rgb);
        }
        let rgb = value.unwrap_or(self.last.get());
        self.checkbox.setState(on(value.is_some()));
        self.well.setColor(&color(rgb));
        self.well.setEnabled(value.is_some());
        self.hex.setStringValue(&NSString::from_str(&config::hex(rgb)));
        self.hex.setEnabled(value.is_some());
    }

    /// The checkbox, the well and the field, each changing the setting.
    fn connect(&self, state: &Rc<State>, show: &Rc<dyn Fn()>, connect: &mut dyn FnMut(&NSControl, Box<dyn Fn()>)) {
        let (me, state_, show_) = (self.clone(), state.clone(), show.clone());
        connect(
            &self.checkbox,
            Box::new(move || {
                let custom = me.checkbox.state() == NSControlStateValueOn;
                let rgb = me.last.get();
                state_.edit(|s| (me.set)(s, custom.then_some(rgb)));
                show_();
            }),
        );
        let (me, state_) = (self.clone(), state.clone());
        connect(
            &self.well,
            Box::new(move || {
                let rgb = rgb_of(&me.well.color());
                me.last.set(rgb);
                me.hex.setStringValue(&NSString::from_str(&config::hex(rgb)));
                let set = me.set;
                state_.edit_soon(move |s| set(s, Some(rgb)));
            }),
        );
        let (me, state_, show_) = (self.clone(), state.clone(), show.clone());
        connect(
            &self.hex,
            Box::new(move || {
                match parse_hex(&me.hex.stringValue().to_string()) {
                    Some(rgb) => state_.edit(|s| (me.set)(s, Some(rgb))),
                    None => NSBeep(),
                }
                show_();
            }),
        );
    }
}

pub fn run(snapshot: Option<PathBuf>) -> Result<(), String> {
    let mtm = MainThreadMarker::new().ok_or("the settings window has to run on the main thread")?;
    let path = config::path()?;
    let settings = config::load(&path)?;
    let state = Rc::new(State { path, settings: Cell::new(settings), pending: RefCell::new(None) });

    let app = NSApplication::sharedApplication(mtm);
    app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
    let delegate = Delegate::new(mtm);
    app.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    app.setMainMenu(Some(&main_menu(mtm)));

    let window = unsafe {
        NSWindow::initWithContentRect_styleMask_backing_defer(
            NSWindow::alloc(mtm),
            CGRect::ZERO,
            NSWindowStyleMask::Titled | NSWindowStyleMask::Closable | NSWindowStyleMask::Miniaturizable,
            NSBackingStoreType::Buffered,
            false,
        )
    };
    window.setTitle(ns_string!("ErisMenus Settings"));
    unsafe { window.setReleasedWhenClosed(false) };

    let accent = ColorPicker::new(mtm, "Use a custom accent color", |s| s.accent, |s, c| s.accent = c, 0xFF6B00);
    let icons = ColorPicker::new(
        mtm,
        "Tint icons a color of their own",
        |s| s.icon_tint,
        |s, c| s.icon_tint = c,
        settings.accent.unwrap_or(0x00A0A0),
    );
    let icons_note = note(
        mtm,
        "Tinted app icons (System Settings ▸ Appearance ▸ Icon & widget style) and folders, everywhere \
         including the Dock. Otherwise they follow the accent color.",
    );
    let accent_note = note(
        mtm,
        "Every app takes it on at once, in place of the one in System Settings. Web page form controls \
         and the Dock keep System Settings' own.",
    );
    let menus = checkbox(mtm, "Draw menus instead of Liquid Glass ones");
    let glass = checkbox(mtm, "Paint over Liquid Glass");
    let glass_note = note(mtm, "Control Center, popovers, alerts, tooltips. Glass already on screen stays as it is until it's drawn again.");
    let animation = NSPopUpButton::initWithFrame_pullsDown(NSPopUpButton::alloc(mtm), CGRect::ZERO, false);
    let titles: Vec<_> = STYLES.iter().map(|style| NSString::from_str(title(*style))).collect();
    animation.addItemsWithTitles(&NSArray::from_retained_slice(&titles));
    let open = unsafe { NSButton::buttonWithTitle_target_action(ns_string!("Open Settings File"), None, None, mtm) };
    let file_note = note(
        mtm,
        &format!(
            "Saved to {}. `erismenus set` changes the same settings from a terminal.",
            state.path.display().to_string().replacen(&std::env::var("HOME").unwrap_or_default(), "~", 1)
        ),
    );

    let label = |text: &str| -> Retained<NSView> {
        Retained::into_super(Retained::into_super(NSTextField::labelWithString(&NSString::from_str(text), mtm)))
    };
    let empty = || NSGridCell::emptyContentView(mtm);
    let rows: Vec<[Retained<NSView>; 2]> = vec![
        [label("Accent color:"), view(&accent.checkbox)],
        [empty(), accent.row(mtm)],
        [empty(), view(&accent_note)],
        [label("Icon tint:"), view(&icons.checkbox)],
        [empty(), icons.row(mtm)],
        [empty(), view(&icons_note)],
        [label("Menus:"), view(&menus)],
        [label("Animation:"), view(&animation)],
        [label("Glass:"), view(&glass)],
        [empty(), view(&glass_note)],
        [empty(), view(&open)],
        [empty(), view(&file_note)],
    ];
    let rows: Vec<_> = rows.iter().map(|row| NSArray::from_retained_slice(row)).collect();
    let grid = NSGridView::gridViewWithViews(&NSArray::from_retained_slice(&rows), mtm);
    grid.setRowSpacing(10.0);
    grid.setColumnSpacing(10.0);
    grid.setRowAlignment(NSGridRowAlignment::FirstBaseline);
    grid.columnAtIndex(0).setXPlacement(NSGridCellPlacement::Trailing);
    let size = grid.fittingSize();
    grid.setFrame(CGRect::new(CGPoint::new(20.0, 20.0), size));
    window.setContentSize(CGSize::new(size.width + 40.0, size.height + 40.0));
    if let Some(content) = window.contentView() {
        content.addSubview(&grid);
    }

    let show = {
        let (state, accent, icons, menus, glass, animation) =
            (state.clone(), accent.clone(), icons.clone(), menus.clone(), glass.clone(), animation.clone());
        move || {
            let s = state.settings.get();
            accent.show(&s);
            icons.show(&s);
            menus.setState(on(s.menus));
            glass.setState(on(s.glass));
            animation.selectItemAtIndex(STYLES.iter().position(|style| *style == s.style).unwrap_or(0) as isize);
            animation.setEnabled(s.menus);
        }
    };
    show();
    let show: Rc<dyn Fn()> = Rc::new(show);

    let mut actions = Vec::new();
    let mut connect = |control: &NSControl, action: Box<dyn Fn()>| {
        let target = Action::new(mtm, action);
        let object: &AnyObject = &target;
        unsafe {
            control.setTarget(Some(object));
            control.setAction(Some(sel!(run:)));
        }
        actions.push(target);
    };
    accent.connect(&state, &show, &mut connect);
    icons.connect(&state, &show, &mut connect);
    {
        let (state, show, checkbox) = (state.clone(), show.clone(), menus.clone());
        connect(
            &menus,
            Box::new(move || {
                state.edit(|s| s.menus = checkbox.state() == NSControlStateValueOn);
                show();
            }),
        );
    }
    {
        let (state, checkbox) = (state.clone(), glass.clone());
        connect(&glass, Box::new(move || state.edit(|s| s.glass = checkbox.state() == NSControlStateValueOn)));
    }
    {
        let (state, popup) = (state.clone(), animation.clone());
        connect(
            &animation,
            Box::new(move || {
                let style = STYLES.get(popup.indexOfSelectedItem().max(0) as usize).copied().unwrap_or_default();
                state.edit(|s| s.style = style);
            }),
        );
    }
    {
        let state = state.clone();
        connect(&open, Box::new(move || open_file(&state)));
    }

    if let Some(path) = snapshot {
        return draw_into(&window, &path);
    }
    window.center();
    window.makeKeyAndOrderFront(None);
    #[allow(deprecated)]
    app.activateIgnoringOtherApps(true);
    app.run();
    drop(actions);
    Ok(())
}

/// The window's content, drawn offscreen into a PNG.
fn draw_into(window: &NSWindow, path: &std::path::Path) -> Result<(), String> {
    let content = window.contentView().ok_or("no content view")?;
    // Offscreen there's no window to draw the background.
    content.setWantsLayer(true);
    if let Some(layer) = content.layer() {
        let background = RefCell::new(None);
        content.effectiveAppearance().performAsCurrentDrawingAppearance(&RcBlock::new(|| {
            *background.borrow_mut() = Some(NSColor::windowBackgroundColor().CGColor());
        }));
        if let Some(color) = background.into_inner() {
            let _: () = unsafe { msg_send![&*layer, setBackgroundColor: &*color] };
        }
    }
    content.layoutSubtreeIfNeeded();
    let bounds = content.bounds();
    let rep = content.bitmapImageRepForCachingDisplayInRect(bounds).ok_or("can't draw the window")?;
    content.cacheDisplayInRect_toBitmapImageRep(bounds, &rep);
    let png = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new()) }
        .ok_or("can't make a PNG")?;
    png.writeToFile_atomically(&NSString::from_str(&path.to_string_lossy()), true)
        .then_some(())
        .ok_or_else(|| format!("can't write {}", path.display()))
}

fn title(style: Style) -> &'static str {
    match style {
        Style::Squishy => "Squishy",
        Style::Expressive => "Expressive",
        Style::Standard => "Standard",
    }
}

fn color(rgb: u32) -> Retained<NSColor> {
    let (r, g, b) = components(rgb);
    NSColor::colorWithSRGBRed_green_blue_alpha(r, g, b, 1.0)
}

fn rgb_of(color: &NSColor) -> u32 {
    let Some(srgb) = color.colorUsingColorSpace(&NSColorSpace::sRGBColorSpace()) else { return 0 };
    let c = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    c(srgb.redComponent()) << 16 | c(srgb.greenComponent()) << 8 | c(srgb.blueComponent())
}

fn open_file(state: &State) {
    // Written first, so there's something to open.
    if !state.path.exists() {
        state.edit(|_| {});
    }
    let url = NSURL::fileURLWithPath(&NSString::from_str(&state.path.to_string_lossy()));
    let workspace = NSWorkspace::sharedWorkspace();
    // With no app for .toml files, show it in Finder instead.
    if !workspace.openURL(&url) {
        workspace.activateFileViewerSelectingURLs(&NSArray::from_retained_slice(&[url]));
    }
}

/// Quit, the edit commands (for the hex field) and Close.
fn main_menu(mtm: MainThreadMarker) -> Retained<NSMenu> {
    let item = |title: &str, action: Sel, key: &str| unsafe {
        NSMenuItem::initWithTitle_action_keyEquivalent(
            NSMenuItem::alloc(mtm),
            &NSString::from_str(title),
            Some(action),
            &NSString::from_str(key),
        )
    };
    let submenu = |title: &str, items: Vec<Retained<NSMenuItem>>| {
        let menu = NSMenu::initWithTitle(NSMenu::alloc(mtm), &NSString::from_str(title));
        for item in &items {
            menu.addItem(item);
        }
        let holder = NSMenuItem::new(mtm);
        holder.setSubmenu(Some(&menu));
        holder
    };
    let bar = NSMenu::new(mtm);
    bar.addItem(&submenu("ErisMenus", vec![item("Quit ErisMenus", sel!(terminate:), "q")]));
    bar.addItem(&submenu(
        "Edit",
        vec![
            item("Undo", sel!(undo:), "z"),
            item("Cut", sel!(cut:), "x"),
            item("Copy", sel!(copy:), "c"),
            item("Paste", sel!(paste:), "v"),
            item("Select All", sel!(selectAll:), "a"),
        ],
    ));
    bar.addItem(&submenu("Window", vec![item("Close", sel!(performClose:), "w"), item("Minimize", sel!(performMiniaturize:), "m")]));
    bar
}

fn view<T: AsRef<NSView>>(control: &Retained<T>) -> Retained<NSView> {
    let view: &NSView = (**control).as_ref();
    objc2::Message::retain(view)
}

fn on(on: bool) -> isize {
    if on { NSControlStateValueOn } else { NSControlStateValueOff }
}

fn checkbox(mtm: MainThreadMarker, title: &str) -> Retained<NSButton> {
    unsafe { NSButton::checkboxWithTitle_target_action(&NSString::from_str(title), None, None, mtm) }
}

fn note(mtm: MainThreadMarker, text: &str) -> Retained<NSTextField> {
    let label = NSTextField::wrappingLabelWithString(&NSString::from_str(text), mtm);
    label.setFont(Some(&NSFont::systemFontOfSize(NSFont::smallSystemFontSize())));
    label.setTextColor(Some(&NSColor::secondaryLabelColor()));
    label.setPreferredMaxLayoutWidth(300.0);
    label
}
