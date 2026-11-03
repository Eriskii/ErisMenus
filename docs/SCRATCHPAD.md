# Scratchpad (for picking the work back up)

Working notes for whoever continues ErisMenus, an agent after a context compaction included. Read
this, then `architecture.md`, `appkit-internals.md` and `testing.md`. Keep it current: rewrite the
"State" and "Next" sections rather than appending history.

## State (2026-11-03)

- **Works, confirmed by the user after a reboot**: context menus, pop-up and pull-down buttons,
  menu bar menus, in every app with a UI: Obsidian, kitty, Finder (including its tag row), Helium,
  ErisDock, and system UI.
- **New since, devhost-verified only (not installed yet; the user runs `sudo target/dist/erismenus
  install`)**:
  - the Apple menu is ours (click, ← →, Control-F2);
  - menu bar extras' menus hang from the item flush with the bar; the item highlights (AppKit's
    `popUpStatusBarMenu:` does that around `presentPopup:`);
  - `sys::Tracking`: menus take the keyboard and background mouse events like AppKit's
    `_setupMenuTracking`. Before, a menu bar extra's menu in an agent app (or a context menu in a
    window behind) couldn't be used from the keyboard and didn't close on a click elsewhere
    (checked: the old build in the `inactive` scenario never got the keyboard);
  - after Control-F2, AppKit's begun session is ended (it used to stay current, holding the
    background event mask);
  - the menu bar title highlight works: the window server is told the title's rect
    (`menuBarMenuTitlesArray`) while open, and nothing after. Not seen on screen.
  - search field, segmented control, path control, token field, toolbar item, right click and
    Services menus were all already ours (now scenarios).
- **Installed** build: `ErisMenusBuild1793682636` (matches commit `1feffa2`). The daemon is a
  system LaunchDaemon (`io.github.eriskii.erismenus`) and survives reboots.
- **Repos**, all public GPL-3.0-or-later, pushed, under `Eris <13102203+Eriskii@users.noreply.github.com>`:
  - `Eriskii/ErisMenus`, the monorepo's `apps/ErisMenus` submodule;
  - `Eriskii/ErisMacInjectLib`, the monorepo's `libs/ErisMacInjectLib` submodule;
  - `Eriskii/apple-sharpenerer` (outside the monorepo, at `../apple-sharpenerer`), which also loads
    through the lib, by a local path dependency (`../ErisMonorepo/libs/ErisMacInjectLib`);
  - `Eriskii/ErisMonorepo` records both submodules.
- 26 devhost scenarios pass (the default set); `menus-core` has 27 unit tests; the lib has 5.
- Another agent works on removing Liquid Glass elsewhere (Control Center's modules, which aren't
  NSMenus) on branch `glass`, worktree `apps/ErisMenus-glass`; its devhost runs show up in the log
  ("painting over glass") and can steal focus from ours mid-run.

## Working with this user

- Wants **every** menu on the system replaced, with no app lists or special cases; says
  "tooltips" for menus. Real hover tooltips (the yellow-ish help tags) haven't been asked for
  clearly. I offered; there was no answer yet.
- Impatient with process talk: fix it, verify it, report briefly. Doesn't want to be sent through
  test steps that could have been automated (use `tools/e2e.sh`).
- Rust only (objc2 in std apps, the raw `msg!` runtime in payloads). No `Co-Authored-By` or
  "Generated with" lines in commits. Plain commit subjects, a body that says what and why.
- ErisDock (`apps/ErisDock`) is another agent's work; don't touch it or commit its submodule
  pointer (it shows as modified in the monorepo; leave it).

## Environment

- macOS 26.4 (25E238), Apple Silicon MacBook, notched display (1800×1169 pt, menu bar 39 pt).
  SIP partly off (debugging, filesystem, nvram) and `-arm64e_preview_abi`; yabai with its
  scripting addition in the Dock.
- No root here: installing (`sudo target/dist/erismenus install`) is the user's step. Without
  root, test with `devhost`, `tools/e2e.sh` (exercises whatever is installed), `make self-test`.
- The terminal has no Accessibility or Screen Recording access: no real clicks, no screenshots.
  Menus are checked through the log and `renderInContext:` PNGs.
- rustup isn't on PATH: `export PATH="/opt/homebrew/opt/rustup/bin:$PATH"`. Toolchain
  `nightly-2026-11-02` (pinned), payloads need `-Zbuild-std=core,alloc` (the Makefile does it).
- `/private/tmp` (and so the session scratchpad) is wiped on reboot.
- zsh's `log` builtin shadows `/usr/bin/log`; there's no `timeout` (use `perl -e 'alarm N; exec @ARGV' …`).

## How we got here (what went wrong, so it isn't repeated)

1. Only pop-ups were hooked: menu bar menus stayed glass. → menu bar takeover.
2. The menu bar was hooked at `startMonitoringEvents:`, after AppKit had already opened its
   menu. → `_mouseDownEventHandler:`.
3. Helium crashed: AppKit's menu bar event monitor sees events our nested loop dequeues, and
   opened its own menu over ours. → both menu bar hooks decline while we track.
4. Clicks anywhere opened the title in the same column: `viewAtCurrentMouseLocation` matches on x.
   → only real title rects.
5. Clicks on the screen's top row (the usual way to hit the bar) missed every title. → inclusive
   top edge.
6. Menus with items' own views (Finder) fell back to glass. → host the item's `NSMenuItemViewer`.
7. `cancelTracking` on any menu ended ours. → only our open impls.
8. The daemon only loaded into regular apps (sharpenerer's rule): ErisDock and system UI were
   skipped. → `procs::app` kinds and `shows_menus`.
9. A second copy stood down even when newer, so updates needed relaunches. → per-build markers.

## Next (none started)

- **Help menu search**: AppKit's Help menu has a search field row (not an item; see
  `appkit-internals.md`); ours has none. Would need a field in our panel that gets the keys, and
  results from the main menu's items (and help books).
- **Palette submenus** (`NSMenuPresentationStylePalette`) open as ordinary submenus; AppKit draws
  them inline as a row of images. An inline row with ← → between its cells would match.
- **Not menus, still AppKit's look**: NSComboBox's list (`NSComboBoxWindow`, a visual effect
  view), the share picker (a popover), Control Center's modules (the other agent).
- Status item keyboard navigation between menu bar extras (`+[NSStatusBarButtonCell
  _trackNavigationLoopOnMenu:view:]`) isn't replicated.
- **Hover tooltips**, if the user wants them: AppKit's tooltip window (`NSToolTipManager`,
  `_NSToolTipWindow`/panel) would need the same treatment.
- Long menus scroll with the wheel and keyboard, but there are no scroll arrows and no hover
  auto-scroll.
- Accessibility (VoiceOver) for our panels: none.
- Settings: animation style and colors are compiled in (`Style::default()`, `style.rs`); a config
  like ErisDock's (`~/.config/…`, Darwin-notify hot reload as sharpenerer does) would fit.
- apple-sharpenerer's path dependency on the lib: switch to a git dependency so its repo builds
  on its own.
- Possibly someday: the out-of-process route (TrustedUIService), see `appkit-internals.md`.
