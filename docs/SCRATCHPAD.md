# Scratchpad (for picking the work back up)

Working notes for whoever continues ErisMenus, an agent after a context compaction included. Read
this, then `architecture.md`, `appkit-internals.md` and `testing.md`. Keep it current: rewrite the
"State" and "Next" sections rather than appending history.

## State (2026-11-03, evening)

- **Installed and confirmed by the user**: menus everywhere (context, pop-up/pull-down, menu bar,
  Apple menu, menu bar extras), glass painted over including Control Center's modules, which look
  and open like our menus. Installed build: lib 0.1.2 (`d9f00c5` era).
- **Built, tested, not installed yet: settings and a custom accent.**
  - `~/.config/erismenus/config.toml` (accent hex or "system", menus on/off, glass on/off,
    animation), set with `erismenus set` or the settings window (`/Applications/ErisMenus.app`,
    same binary). Published as the state of the `io.github.eriskii.erismenus.settings` Darwin
    notification; the daemon holds a registration (state dies with the last one) and publishes the
    user's file at login.
  - Accent: `accent.rs`, recipe in `docs/accent.md` (Multicolor in the argument domain,
    `_controlTintChanged:` before `_setAccentColor:`, swizzles on `+[NSColor controlAccentColor]`
    and the folder tint). Live changes verified with `tools/accenthost.m` (system → color → color
    → system), our menus highlight in it. Not covered: WebKit form controls (WebContent process),
    the Dock, out-of-process UI.
  - Icon tint, separate from the accent (`icon_tint`): macOS's own tint set to "other" (10) with
    our color via `SLSIconAppearanceConfiguration -save`, from `erismenus` as the user; the payload's
    folder swizzle is the fallback. The save path was never run in tests (it writes the user's
    global prefs, and the sandbox refused); the first real use is the user's.
  - Regression: 26 devhost scenarios and 40 glasshost checks pass; `menus off` / `glass off`
    verified with a held registration (see `testing.md`).
- **Repos**, all public GPL-3.0-or-later, pushed, under `Eris <13102203+Eriskii@users.noreply.github.com>`:
  - `Eriskii/ErisMenus`, the monorepo's `apps/ErisMenus` submodule;
  - `Eriskii/ErisMacInjectLib` (`libs/ErisMacInjectLib`), tags v0.1.0–v0.1.2: v0.1.1 rechecks
    processes caught mid-launch (xpcproxy) and only counts crashes with a crash report; v0.1.2
    waits on suspended processes (extensions) instead of stalling the daemon;
  - `Eriskii/apple-sharpenerer` (`../apple-sharpenerer`), loads into regular and agent apps;
  - both depend on the lib by git tag, patched to `libs/` inside the monorepo (`.cargo/config.toml`);
  - ErisDock got one fix from us (`dd5501d`, rejoin every space when pinned to one); the rest of
    its working tree is the other agent's, uncommitted.
- Lessons from today: installs can leave two builds in running apps until they relaunch (the
  user saw "random" breakage after installs; a reboot after installing gives a clean state).
  Don't drive the user's pointer or spaces (see memory).

## Working with this user

- Wants **every** menu on the system replaced, with no app lists or special cases; says
  "tooltips" for menus. Real hover tooltips are restyled by `glass.rs` (asked for with the rest of
  the glass: "anywhere where we still got liquid ASS").
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

- **Glass the payload can't reach**: the Dock (Dock background, folders, Launchpad, Mission
  Control) builds its own glass from Core Animation and is skipped for yabai; the menu bar itself is
  drawn out of process; login window and SecurityAgent are skipped. Glass in light mode: SwiftUI
  glass that's only part of a window keeps dark text on the black body (AppKit glass and windows
  made of glass are fixed); the user is in dark mode.
- **Glass panels, unverified**: whether Control Center's real module windows match `hang` (one
  transparent "Control Center" window, level 23, 458×1125 pt from the top of the screen, seen with
  `CGWindowListCopyWindowInfo`; the glass inside it hanging under the menu bar); whether its own
  open animation (a `setAlphaValue:` fade, `_BentoTransitionCoordinator` in SwiftUI) is fully
  overridden; closing like ours (a 0.12 s fade) would need the order-out held back, not done.
  Popovers with an arrow keep their own shape (not a single rounded rect).
- **Help menu search**: AppKit's Help menu has a search field row (not an item; see
  `appkit-internals.md`); ours has none. Would need a field in our panel that gets the keys, and
  results from the main menu's items (and help books).
- **Palette submenus** (`NSMenuPresentationStylePalette`) open as ordinary submenus; AppKit draws
  them inline as a row of images. An inline row with ← → between its cells would match.
- **Not menus, still AppKit's look**: NSComboBox's list (`NSComboBoxWindow`, a visual effect
  view), the share picker (a popover; its glass is painted over by `glass.rs`).
- Status item keyboard navigation between menu bar extras (`+[NSStatusBarButtonCell
  _trackNavigationLoopOnMenu:view:]`) isn't replicated.
- Long menus scroll with the wheel and keyboard, but there are no scroll arrows and no hover
  auto-scroll.
- Accessibility (VoiceOver) for our panels: none.
- Settings: animation style and colors are compiled in (`Style::default()`, `style.rs`); a config
  like ErisDock's (`~/.config/…`, Darwin-notify hot reload as sharpenerer does) would fit.
- Possibly someday: the out-of-process route (TrustedUIService), see `appkit-internals.md`.
