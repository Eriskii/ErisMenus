# Handoff: ErisMacShell and CustomMacColors

Written 2026-11-04 for whoever does this after a context compaction. Nothing below is started. Read
this, then `SCRATCHPAD.md` ("Working with this user", "Environment", the past mistakes), then
`architecture.md` and `accent.md`.

## What the user asked for

1. **`Eriskii/ErisMacShell`**: a new public repo merging ErisDock and ErisMenus, with one unified
   settings window and each component optional: "no glass but og dock, custom dock but full glass, etc."
2. **`Eriskii/CustomMacColors`**: a new public repo, **not in the monorepo**, holding the custom color
   feature (accent and icon tint). The one thing that doesn't go into ErisMacShell.

## Decisions the user made

- **Old repos:** import their code and history into ErisMacShell, then archive `Eriskii/ErisDock` and
  `Eriskii/ErisMenus` on GitHub (read-only, README pointing to the new repo) and remove their
  submodules from the monorepo. Archive only; don't delete.
- **apple-sharpenerer stays separate**, as it is. Don't touch it (it may later be merged; not now).
- **Menu highlight:** neutral (`Palette::highlight`, white at 16%) by default. If CustomMacColors is
  running with a custom accent, the highlight uses it. Neither project depends on the other.

## My design defaults (not asked; the user can overrule them)

Tell the user these when reporting; they're conventional, not their decisions.

- ErisMacShell is at `apps/ErisMacShell` in the monorepo, as a submodule, GPL-3.0-or-later, like the
  others. CustomMacColors is at `Projects/CustomMacColors`, next to the monorepo, like apple-sharpenerer.
- **Components:** dock, menus, glass. Three independent switches. (Tooltips belong to glass.)
- **One app, one daemon.** `ErisMacShell.app` runs the dock (when on), the settings window and the
  command line. A root LaunchDaemon injects the payload for menus and glass.
- **Dock-only use needs no root and no daemon.** The daemon is only for menus and glass.
- **Dock off:** the app doesn't draw a dock, the native Dock is restored (`scripts/native-dock.sh
  restore` logic), and it's hidden again when the dock goes on. Opening the app shows settings.
- **Config:** `~/.config/erismacshell/config.toml` with `[dock]`, `[menus]`, `[glass]` tables, imported
  once from `~/.config/erisdock/config.toml` and `~/.config/erismenus/config.toml`. `pinned.toml` moves too.

## Where things stand

All pushed: ErisMenus `d1424c4` + the commit adding this file, ErisDock `97ef87c`, ErisMacInjectLib
`ed2e40b` (tags to `v0.1.2`), apple-sharpenerer `4aee083`. The monorepo's `apps/ErisReader` shows as
modified; it isn't ours, leave it.

Installed on the user's Mac: ErisMenus (`/Applications/ErisMenus.app`, daemon
`io.github.eriskii.erismenus`), ErisDock (`~/Applications/ErisDock.app`, `com.eriskii.ErisDock`, login
item via `SMAppService`), sharpenerer. **The user's live settings, which must survive the move:**
`accent = "#B473F7"`, `icon_tint = "#DDB3FF"`, menus on, glass on, animation squishy; icon theme
TintedDark. macOS's own tint is saved as `Other` (confirmed in `defaults read -g`, so the `-save` path
in `icons.rs` works, despite `SCRATCHPAD.md` saying it was never run).

## Plan

### 1. CustomMacColors (do first: it takes the color code out)

Local `Projects/CustomMacColors`, own git repo. Rust workspace like ErisMenus' shape:

- **core**: settings (accent, icon tint) with `pack`/`unpack`, `parse_hex`, `components`, from
  `crates/core/src/settings.rs`.
- **payload** (`no_std`, own dylib, own `payload!` name): `crates/payload/src/accent.rs` and a
  `settings.rs` like ErisMenus'. Remove `cg_color()` if unused.
- **the command**: from `crates/erismenus/src`: `config.rs`, `icons.rs`, a window with the two
  `ColorPicker` rows from `window.rs`, the daemon `Client`, `shows_menus`-style target rule, an app
  bundle in `/Applications/CustomMacColors.app`.
- Also move `docs/accent.md`, `docs/accent/overview.png`, `tools/accent/`, `tools/accenthost.m`.
- Depends on the lib by **git tag** (`eris-inject = { git = "https://github.com/Eriskii/ErisMacInjectLib", tag = "v0.1.2" }`,
  see the monorepo README, "How apps reach libraries"), with the Makefile finding the lib via `cargo
  metadata`, as apple-sharpenerer's does. It's outside the monorepo, so no `[patch]` applies.
- Its own Darwin notification: `io.github.eriskii.custommaccolors.settings`. Its daemon holds a
  registration for good (a notification's state dies with the last registration) and publishes the
  user's file at login. Config: `~/.config/custommaccolors/config.toml`, imported from the user's
  ErisMenus file.
- **The contract with the Shell's highlight:** document the packed format in both READMEs. Bit 63
  valid, bit 24 accent on, bits 0-23 RGB. Keep those bits stable.
- Fresh history is fine (git-filter-repo isn't installed; `brew install git-filter-repo` needs the
  user's OK). State in the README where it came from.

It's a third daemon (with the Shell's and sharpenerer's), each injecting a payload into every app.
Check they coexist: run all three in a test host.

### 2. ErisMacShell

Create the repo, import both with history. `git-filter-repo` isn't installed; use `git subtree add
--prefix=...` or `git merge --allow-unrelated-histories` after moving files in a branch. Layout:

```
crates/core      menus-core, without accent and icon tint
crates/payload   the dylib (menus + glass), without accent.rs
crates/dock      ErisDock's src/, as a library
crates/shell     the app: main (CLI, daemon, no args in a bundle = app), config, unified settings window
tools/ docs/ Resources/ scripts/
```

- Everything from ErisDock is a root-level single crate today, so it needs turning into a library
  (`src/main.rs` into a `run`). Its `objc2` deps (`0.6.5`, app-kit `0.3.2`) match ErisMenus'.
- **Dependencies:** lib by tag as above. Add nothing to `.cargo/config.toml` but check its `[patch]`
  still applies. **Remove `accent` and `icon_tint` from `Settings`**, reassigning their bits.
- **Settings window:** merge `apps/ErisDock/src/settings_window.rs` and `crates/erismenus/src/window.rs`
  (grid style is the same). A section per component, each with its on/off switch, its controls
  disabled when off. The dock's window needs `Dock` for "Replay Animation" and such; with the dock
  off, those are off too.
- **Payload:** when menus and glass are both off, the daemon needn't load into new processes;
  already-loaded payloads go passive (they already read `settings::current()` on each use).
- **Highlight:** in `panel.rs`, `crate::accent::cg_color().unwrap_or(p.highlight)` becomes: read the
  `io.github.eriskii.custommaccolors.settings` state with `notify_register_check`/`notify_get_state`
  at each menu open, and if it's valid with bit 24 on, that color; else `p.highlight`.
- **Names:** daemon `io.github.eriskii.erismacshell`, dir `/Library/Application
  Support/ErisMacShell`, log `/Library/Logs/erismacshell.log`, link `/usr/local/bin/erismacshell`,
  notification `io.github.eriskii.erismacshell.settings`. Marker classes and `ErisMenusDisabled`
  defaults: rename, and accept the old default names too.
- **Install has to replace what's there:** unload and remove `io.github.eriskii.erismenus` and its
  files, `/Applications/ErisMenus.app`, `/usr/local/bin/erismenus`; quit ErisDock, remove
  `~/Applications/ErisDock.app`, unregister its login item. Don't leave two daemons or two docks.
- **A new bundle id and signature means Accessibility must be granted again**, once per ad hoc
  build (`ERISDOCK_SIGN` signs with a lasting identity). Tell the user up front.

### 3. Retire the old repos

Push a final README to ErisDock and ErisMenus pointing to the new repo, then `gh repo archive`.
In the monorepo: `git rm` both submodules, add `apps/ErisMacShell`, update its README and `.gitmodules`.
Before archiving, confirm the user has ErisMacShell installed and working.

### 4. Verify, then one install

All of it before asking the user to install, once:

- `make test`, `make self-test`, all 26 devhost scenarios, all 40 glasshost checks, accenthost in
  CustomMacColors, with the three payloads coexisting.
- **Each combination of switches** (dock on/off × menus on/off × glass on/off). Hold a registration
  during tests started after a `set` (see `testing.md`).
- A fresh clone of each new repo builds on its own.
- Settings imported from the old files; the user's colors unchanged.
- After install: a reboot (a running app otherwise holds two builds' payloads).

## Don't get bitten (all of these happened)

- **Don't drive the user's pointer or switch spaces** (`cliclick`, `yabai -m space --focus`) unless asked
  for that task. Use test windows of our own. Running test hosts that post events to themselves is OK.
- **Don't write the user's real settings or global defaults in tests.** Use a scratch `HOME`; `set`
  with a color also saves macOS's icon tint, whatever `HOME` is, so use `reload` or hand-written files
  in tests. The sandbox refuses global preference writes anyway; don't work around it.
- **Follow the monorepo README** for dependencies and layout. Read it first.
- **Don't invent scope.** No extra features. Ask what's broken instead of guessing, but don't send the
  user through test steps that can be automated.
- **Test against the real failing state,** not a similar one: the dock's first "fix" passed its test
  and did nothing.
- **Say what's unverified.** No Claude attribution in commits. Rust only for shipped code.
- The dock can get pinned to one space by something outside it (cause unknown); `rejoin_every_space`
  in `view.rs` repairs it. Keep that. `/private/tmp` is wiped on reboot, so anything worth keeping
  goes in a repo.
