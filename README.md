# ErisMenus

Menus for macOS 26 drawn by us instead of Liquid Glass: [ErisDock](../ErisDock)'s black body with
smoothed corners and white text, unrolling from the corner the menu opens at, with a highlight
that slides from row to row. AppKit still decides what goes in a menu and what choosing an item
does. ErisMenus only shows the menu and tracks it, so the app sees exactly what it would with
AppKit's menus. Rust, `no_std`, loaded into every app by a root daemon.

- **Every popped-up menu**: context menus, `popUpMenuPositioningItem:`, pop-up buttons (the
  chosen item over the button) and pull-downs (without their title item), and the menus of search
  fields, segmented controls, path controls, token fields, toolbar items and the Services menu.
- **The menu bar**, the Apple menu included: a title's menu hangs from the bar under its
  highlighted title, and moving across the titles or pressing ← and → switches menus. Clicking the
  open title closes it. Control-F2 starts at the Apple menu.
- **Menu bar extras** (an app's `NSStatusItem` menus) hang from the item, which stays highlighted
  while its menu is open. Like AppKit's, a menu takes the keyboard and sees clicks elsewhere even in
  an app that isn't frontmost, so an agent's menu works from the keyboard and closes on a click
  outside it.
- **Rows**: titles (attributed ones keep their fonts), images (templates tinted), checkmarks and
  mixed states, key equivalents as ⌃⌥⇧⌘ glyphs, section headers, subtitles, indentation,
  separators, and submenus with a chevron. Alternates swap in while their modifiers are held.
  Disabled items are dimmed and skipped.
- **Pointer**: rows highlight under the pointer, and submenus open after a short pause and close
  once the pointer settles on another row. Press, drag and release chooses. A click outside, or
  the app losing focus, dismisses the menu.
- **Keyboard**: ↑ ↓, Home and End move. → opens a submenu and ← closes it. Return, Enter and
  Space choose, and Escape dismisses. Typing jumps to a matching title.
- **Long menus** scroll with the wheel and follow the keyboard.
- **All other Liquid Glass** in an app is painted over with the same black body, in the glass's
  own shape and without refraction, rims or sheen: Control Center's modules (Wi-Fi, Battery,
  Sound, …), Notification Center, Spotlight, popovers, alerts, glass buttons, SwiftUI's
  `glassEffect`. Hover tooltips get the black body too. Out of reach: the Dock's own glass and the
  menu bar itself (drawn outside apps).
- **Any accent color**: a hex color of your choosing becomes the system accent in every app,
  in place of Apple's presets: checkboxes, switches, sliders, default buttons, selections, focus
  rings, folders, SwiftUI's `.tint`, and our menus' highlight. Web page form controls and the Dock
  keep the one from System Settings. How: [`docs/accent.md`](docs/accent.md).

## Settings

**ErisMenus** in Applications opens the settings window; `erismenus settings` does too. Changes
reach every app at once. The same settings from a terminal:

```sh
erismenus set accent "#FF6B00"     # any hex color; "system" for System Settings' own
erismenus set icon-tint "#00A0A0"  # tinted app icons and folders; "accent" to follow the accent
erismenus set menus off            # AppKit's menus again (on to come back)
erismenus set glass off            # leave Liquid Glass alone (glass on screen stays until redrawn)
erismenus set animation expressive # squishy (the default), expressive or standard
erismenus status
```

They're kept in `~/.config/erismenus/config.toml` (`erismenus reload` after editing it by hand) and
go to apps as the state of a Darwin notification, so sandboxed apps get them too.

Every menu in an app is left to AppKit with `defaults write <bundle id> ErisMenusDisabled -bool YES`.
`ErisMenusGlassDisabled` leaves just the glass alone.

## How it works

On macOS 26.4 every NSMenu that pops up goes through one private class method,
`+[NSContextMenuImpl presentPopup:fromView:withContext:animated:]`. It fills the menu, shows it,
runs the tracking loop and performs the chosen item. Its `NSMenuContext` says where the menu goes
(`frameAlignmentPoint`, `checkedItemIndex`, `popupButtonBounds`, `minWidth`, `flags`).
ErisMenus replaces that method and does the same job, keeping AppKit's lifecycle calls on the
menu's impl in AppKit's order:

1. `_sendMenuPopulate` (`menuNeedsUpdate:`), `_sendMenuOpening` (`menuWillOpen:`),
   `_sendMenuEnableItems` (validation). The menu is marked open in its `systemAttributes`
   (bit 2), without which `menuDidClose:` never goes out.
2. Our panel and our tracking loop (`nextEventMatchingMask:` in the event tracking run loop mode).
3. `_sendMenuClosed` (`menuDidClose:`), then the chosen item through
   `+[NSCocoaMenuImpl _performActionForMenuItem:invocationType:]`, which also makes
   `popUpMenuPositioningItem:` return YES.

The menu bar is tracked by its own modal loop instead, which starts in
`-[NSMenuBarTrackingSession _mouseDownEventHandler:]` (a click) or
`-[NSMenuTrackingSession startMonitoringEvents:]` (Control-F2). ErisMenus runs its own loop there,
opening each title's menu with the same lifecycle, keeping the title highlighted with
`-[NSMenuBarImpl setHighlightedView:]` (AppKit passes the title's rect on to the window server,
which draws the bar and its selection), and finding the titles on screen among the menu bar's
`NSMenuBarItemView`s. The Apple menu isn't one of the main menu's items: it's
`+[NSMenuBarImpl appleMenuItem]`, at index -1 in the bar.

While a menu is open ErisMenus also does what AppKit's `-[NSMenuTrackingSession
_setupMenuTracking]` does: it asks the window server for mouse events from outside the app's
windows, takes the keyboard if another app has it, and posts and listens for HIToolbox's
begin/end/cancel menu tracking notifications.

`-[NSMenu cancelTracking]` and `-[NSMenu highlightedItem]` are answered from the running menu, and
the delegate hears `menu:willHighlightItem:`. Submenus get the same lifecycle on their own impls.

Getting into apps is [ErisMacInjectLib](../../libs/ErisMacInjectLib)'s job, shared with
apple-sharpenerer: the `erismenus` daemon watches for the logged-in user's processes starting, maps
`liberismenus.dylib` into each that can show a menu (apps, agents and menu bar extras, system UI,
app extensions, AppKit's UI services) without dyld, so library validation never sees it, and has an
arm64e helper start it. The payload runs on the library's `no_std` runtime, which is what makes it
safe in arm64e apps. Only background processes, browser and Electron helpers, the login window, the
password prompt and the Dock are left alone; `erismenus targets` lists the rest.

`crates/core` holds everything that doesn't need AppKit: layout, placement on screen, the outline,
springs and key-equivalent glyphs. `crates/payload` is the dylib, `crates/erismenus` the command
and daemon.

## Requirements

- Apple Silicon, macOS 26
- SIP partially disabled (debugging restrictions off) and the `-arm64e_preview_abi` boot-arg, as
  for yabai's scripting addition and apple-sharpenerer
- Root, to load into apps

## Install

```sh
make
sudo target/dist/erismenus install
```

This puts the files in `/Library/Application Support/ErisMenus`, the settings app in
`/Applications/ErisMenus.app`, links `erismenus` into `/usr/local/bin` and starts a LaunchDaemon that loads into every app as it launches. Apps already
running get ErisMenus once relaunched, or right away with `sudo erismenus inject <pid>`.
`erismenus status` says whether the daemon runs, its log is `/Library/Logs/erismenus.log`, and
`sudo erismenus uninstall` removes it all.

## Building and trying it

Needs the Xcode command line tools and rustup (the pinned nightly toolchain installs itself). Cargo
gets ErisMacInjectLib from GitHub, or from `libs/ErisMacInjectLib` inside ErisMonorepo.

```sh
make            # target/dist/{erismenus,eris-inject-thread,liberismenus.dylib}
make test       # the core crate's tests
make self-test  # maps the payload into erismenus itself, as the daemon would, without root
make devhost
target/dist/devhost target/dist/liberismenus.dylib /tmp/menus
```

The development host loads the dylib, opens every kind of menu it knows (context menus,
submenus, positioned menus, pop-up buttons and pull-downs, NSTextView's menu, the menu bar and the
Apple menu, a menu bar extra, and the menus of other controls), and drives them with key presses
and posted clicks. It prints what the app hears back, whose menu came up (ours or AppKit's), and
draws each menu into a PNG in the output folder. Name scenarios to run some (the list is in
[`docs/testing.md`](docs/testing.md)), or pass `-` instead of the dylib to see AppKit's own menus. The payload logs to the unified log:
`log stream --predicate 'eventMessage BEGINSWITH "[ErisMenus]"'`.

## More

[`docs/`](docs): how it's built (`architecture.md`), the AppKit internals it relies on
(`appkit-internals.md`), testing and debugging (`testing.md`), and working notes for picking the
work back up (`SCRATCHPAD.md`).

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
