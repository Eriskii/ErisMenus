# Architecture

ErisMenus replaces how macOS 26 shows menus, not what's in them. AppKit still builds every menu,
validates its items and performs the chosen one; ErisMenus draws the menu and tracks the pointer and
keyboard while it's open. It runs inside every app, loaded by a root daemon.

## Pieces

| Path | What |
|---|---|
| `crates/core` (`menus-core`) | Everything without AppKit, `no_std`, unit-tested: row layout and columns, placement on screen (`Anchor`, `place`), the smoothed outline (`shape`), springs and the three animation styles, key-equivalent glyphs, keyboard navigation and type-select. |
| `crates/payload` (`liberismenus.dylib`) | The code that runs inside apps, `no_std`, on ErisMacInjectLib's payload runtime. |
| `crates/erismenus` | The command and the root daemon. |
| `tools/devhost.m` | A test app that loads the dylib and drives every kind of menu. |
| `tools/e2e.sh` | The same test app, loaded by the installed daemon instead. |
| `../../libs/ErisMacInjectLib` | Loading into apps: the injector, the daemon loop, launchd, the payload runtime (PAC, objc, logging). Shared with apple-sharpenerer. |

## The payload

`crates/payload/src`:

- `lib.rs`: `payload!(name: "erismenus", …)` exports `_erismenus_bootstrap` / `_erismenus_main`.
  `start(injected)` waits for AppKit (`NSContextMenuImpl`), then, when injected, for `NSApp` (up to
  30 s; processes that never become apps get nothing), then installs the hooks on the main thread
  and checks them 5 s later.
- `hooks.rs`: the six replaced AppKit methods (table in `hooks()`), `install()` and `verify()`.
  Each build defines a marker class `ErisMenusBuild<unix time>` (stamped by `build.rs`); a copy
  finding its own build's marker stands down, one finding only `ErisMenusLoaded` (an older build)
  hooks over it, so reinstalling updates running apps.
- `session.rs`: one menu interaction from open to close. `present()` handles popped-up menus,
  `track_menu_bar()` the menu bar. Both run the same modal loop (`Session::track`), with a stack
  of `Level`s (the menu and its open submenus) and, for the menu bar, a `MenuBar` of titles (the
  Apple menu and the main menu's items, left to right).
- `panel.rs`: one open menu on screen: a borderless `NSPanel` at level 101, a layer-hosting view
  drawing the body (`CAShapeLayer`, smoothed corners, shadow), rows (`CATextLayer`s, image layers,
  separators) and the highlight, plus a container view holding items' own views.
- `model.rs`: a menu's items as rows: hidden items out, separators tidied, alternates folded into
  the row they replace, each retained.
- `style.rs`: colors, fonts, attributed strings, SF Symbols.
- `sys.rs`: CoreGraphics geometry, paths, colors, the clock, and `Tracking`: what AppKit's
  `_setupMenuTracking` does with the window server while a menu is open (background mouse events,
  the keyboard, HIToolbox's menu tracking notifications), undone when dropped.

### Hooks

| Method | Why |
|---|---|
| `+[NSContextMenuImpl presentPopup:fromView:withContext:animated:]` | Every popped-up menu goes through it: context menus, `popUpMenuPositioningItem:`, pop-up and pull-down buttons. |
| `-[NSMenuBarTrackingSession _mouseDownEventHandler:]` | A click on the menu bar. AppKit opens its own menu inside this handler, so it has to be taken over here. |
| `-[NSMenuTrackingSession startMonitoringEvents:]` | The menu bar's modal loop, reached without a click (Control-F2). AppKit has begun its session by then; we end it (`dismissAnimated:`) after ours. |
| `-[NSMenuBarTrackingSession _handleMonitorEvent:]` | AppKit's local event monitor for the menu bar; it stands aside while we track, or it acts on events our loop dequeues. |
| `-[NSCocoaMenuImpl _cancelTrackingWithAnimation:]` | `-[NSMenu cancelTracking]`, honored only for menus we have open. |
| `-[NSCocoaMenuImpl highlightedItem]` | `-[NSMenu highlightedItem]` while we track. |

### A menu's lifecycle

The same AppKit calls, in AppKit's order, so delegates and the app can't tell the difference:

1. on the menu's impl: `_sendMenuPopulate` (`menuNeedsUpdate:`), `_sendMenuOpening`
   (`menuWillOpen:`), `_sendMenuEnableItems` (validation);
2. `menu.systemAttributes |= 4` (AppKit's "opened" bit; without it `menuDidClose:` never goes
   out) and `setIsVisible:YES`;
3. our panel and loop;
4. `setIsVisible:NO`, `_sendMenuClosed` (`menuDidClose:`);
5. the chosen item through `+[NSCocoaMenuImpl _performActionForMenuItem:invocationType:]`, which
   is also what makes `popUpMenuPositioningItem:` return YES.

Submenus get steps 1–4 on their own impls. `NSMenuDidBeginTrackingNotification` and
`NSMenuDidEndTrackingNotification` are posted around the whole thing, and `sys::Tracking` is held
for it: like AppKit, the app gets mouse events from anywhere and the keyboard while the menu is
open, even when it isn't frontmost, and HIToolbox's cancel-tracking notification closes the menu.

### The loop

`Session::track` pulls events with `nextEventMatchingMask:` in `NSEventTrackingRunLoopMode` every
1/120 s (1/60 s when nothing moves), polls `+[NSEvent mouseLocation]` for hover, runs the submenu
open/close timers and steps the springs. Mouse events on an item's own view go straight to the view
under the pointer (`hitTest:`), as AppKit's menu windows do. A click outside, Escape, the app
losing focus, choosing an item or a cancel for one of our menus ends it; the log says which.

Opening a menu: `Anchor` comes from the `NSMenuContext`: a pop-up button's bounds
(`popupButtonBounds`, flipped screen coordinates) with its checked item over the button; a
positioned item (`checkedItemIndex` at `frameAlignmentPoint`); hanging from the `exclusionRect`
when `frameAlignmentPoint` is inside it (a menu bar extra's item); otherwise the menu's top-left at
`frameAlignmentPoint`, flipping up or left when there's no room. Pull-downs (`flags & 8`) leave out
item 0. Submenus open beside their row; menu bar menus hang from their title (`Anchor::Below`,
right up against the bar, right edges lined up near the screen's right).

## The daemon

`erismenus daemon` (run by launchd as `io.github.eriskii.erismenus`) is ErisMacInjectLib's daemon
loop with a `Client` whose `wants` takes every process that can show an AppKit menu
(`shows_menus` in `crates/erismenus/src/main.rs`): regular apps, agents (`LSUIElement`), app
extensions, and XPC services whose `RunLoopType` is `NSApplicationMain`. It leaves out background
processes, helpers nested in a `Frameworks` folder (browser and Electron renderers), the login
window, `SecurityAgent`, and the Dock (yabai's). An app that exits within 10 s of a load, twice,
is left alone until the daemon restarts.

Installed files: `/Library/Application Support/ErisMenus/{erismenus,eris-inject-thread,liberismenus.dylib}`,
`/Library/LaunchDaemons/io.github.eriskii.erismenus.plist`, `/usr/local/bin/erismenus`, log
`/Library/Logs/erismenus.log`.
