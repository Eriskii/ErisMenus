# ErisMenus

Menus for macOS 26 drawn by us instead of Liquid Glass: [ErisDock](../ErisDock)'s black body with
smoothed corners and white text, unrolling from the corner the menu opens at, with a highlight
that slides from row to row. AppKit still decides what goes in a menu and what choosing an item
does. ErisMenus only shows the menu and tracks it, so the app sees exactly what it would with
AppKit's menus. Rust, `no_std`, loaded into every app by a root daemon.

- **Every popped-up menu**: context menus, `popUpMenuPositioningItem:`, pop-up buttons (the
  chosen item over the button) and pull-downs (without their title item).
- **The menu bar**: a title's menu hangs from the bar, and moving across the titles or pressing
  ← and → switches menus. Clicking the open title closes it. The Apple menu is still AppKit's.
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

Menus with custom item views are left to AppKit, and so is every menu in an app with
`defaults write <bundle id> ErisMenusDisabled -bool YES`.

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
`-[NSMenuTrackingSession startMonitoringEvents:]` on an `NSMenuBarTrackingSession`. ErisMenus runs
its own loop there, opening each title's menu with the same lifecycle, keeping the title
highlighted with `-[NSMenuBarImpl setHighlightedView:]`, and finding the titles on screen with
`frameForViewAtMenuIndex:`.

`-[NSMenu cancelTracking]` and `-[NSMenu highlightedItem]` are answered from the running menu, and
the delegate hears `menu:willHighlightItem:`. Submenus get the same lifecycle on their own impls.

Getting into apps is [ErisMacInjectLib](../../libs/ErisMacInjectLib)'s job, shared with
apple-sharpenerer: the `erismenus` daemon watches for apps of the logged-in user starting, maps
`liberismenus.dylib` into each without dyld (so library validation never sees it) and has an arm64e
helper start it. The payload runs on the library's `no_std` runtime, which is what makes it safe in
arm64e apps. The Dock, login window, system UI and password managers are left alone.

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

This puts the files in `/Library/Application Support/ErisMenus`, links `erismenus` into
`/usr/local/bin` and starts a LaunchDaemon that loads into every app as it launches. Apps already
running get ErisMenus once relaunched, or right away with `sudo erismenus inject <pid>`.
`erismenus status` says whether the daemon runs, its log is `/Library/Logs/erismenus.log`, and
`sudo erismenus uninstall` removes it all.

## Building and trying it

Needs the Xcode command line tools, rustup (the pinned nightly toolchain installs itself), and
ErisMacInjectLib at `../../libs/ErisMacInjectLib`.

```sh
make            # target/dist/{erismenus,eris-inject-thread,liberismenus.dylib}
make test       # the core crate's tests
make self-test  # maps the payload into erismenus itself, as the daemon would, without root
make devhost
target/dist/devhost target/dist/liberismenus.dylib /tmp/menus
```

The development host loads the dylib, pops up a context menu, a submenu, a positioned menu, a
pop-up button, a pull-down and NSTextView's own menu, and drives them with key presses. It prints
what the app hears back and draws each menu into a PNG in the output folder. Name scenarios to run
some (`context submenu positioning escape popup pulldown textview cancel menubar frames`), or pass `-`
instead of the dylib to see AppKit's own menus. The payload logs to the unified log:
`log stream --predicate 'eventMessage BEGINSWITH "[ErisMenus]"'`.

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).
