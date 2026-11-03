# Testing and debugging

Real menus can't be clicked from a terminal (no Accessibility or Screen Recording access), so
everything is driven by synthetic key presses and inspected through the log and offscreen renders.

## Unit tests

`make test` runs `menus-core`'s tests: layout, placement, shapes, springs, glyphs, navigation.

## The development host

```sh
make devhost        # builds target/dist/{liberismenus.dylib,devhost}
target/dist/devhost target/dist/liberismenus.dylib /tmp/menus [scenario ...]
```

It `dlopen`s the dylib (or nothing, given `-`, to see AppKit's own menus), registers
`ErisMenusIgnorePointer` so the real pointer doesn't interfere, opens a window with a pop-up button,
a pull-down and a text view, a second window with other controls that show menus and a toolbar, a
menu bar extra (a status item titled "devhost") and a main menu, and runs scenarios.
Each prints what the app hears (delegate calls, actions, return values) and renders the open menus
to PNGs with `renderInContext:`. The newer ones also print whose menu came up ("1 of our menus, 0
of AppKit's", with the windows' frames) and, with `-`, render AppKit's menu window too
(`<name>-appkit-N.png`, its views without the glass), to compare placement and content.

The Apple menu's actions are swallowed and printed (`APPLE MENU ACTION …`): devhost replaces
`_NSAppleMenu`'s `…Requested:` methods, or the `apple` scenario would open About This Mac and
others could sleep or restart the Mac. The menu bar's selection is read from what AppKit sends the
window server (`+[NSCGSSpace setMenuBarSettings:globalSettings:]`, swizzled to record it).

| Scenario | Checks |
|---|---|
| `context` | A context menu: icons, alternates, a disabled item, a section header, states, a submenu, a subtitle; ↓↓ Return picks Copy. |
| `submenu` | → opens Open With, ↓ Return picks TextEdit. |
| `positioning`, `escape` | `popUpMenuPositioningItem:` returns 1 on a pick, 0 on Escape. |
| `popup`, `pulldown` | A pop-up button changes to Three; a pull-down skips its title and picks Rename. |
| `textview` | NSTextView's own context menu. |
| `cancel` | `cancelTracking` from a timer; returns 0. |
| `menubar` | The menu bar from Control-F2: → ← move between titles, Return picks Close. |
| `menubar-click` | A click on the File title on the screen's top row, through AppKit's real handler. |
| `menubar-monitor` | A second menu bar click while ours is open must not reach AppKit (the Helium crash). |
| `menubar-stray` | A click below a title, not on the bar, must open nothing. |
| `customview` | An item with its own view (a button) is shown and clicked through our panel. |
| `apple` | Control-F2 the way AppKit does it: the Apple menu opens under its highlighted title; ← wraps to Edit, → back, Return picks About This Mac; AppKit's session is ended afterwards. |
| `apple-click` | A click on the Apple title; → moves to the app menu, ← back, Escape. |
| `menubar-highlight` | The selection the window server is told to draw while File is open, and that it's gone after. |
| `statusitem` | A menu bar extra's menu: hangs from the item (top at the bar's bottom), the button highlighted while it's open; ↓↓ Return picks Beta. |
| `inactive` | The same with the app hidden (not frontmost, as an agent always is): the menu has the keyboard and background mouse events while open, and gives both back. |
| `searchfield` | NSSearchField's recent searches menu, from a click on the magnifier. |
| `segmented` | A segment's menu, from a held press. |
| `pathcontrol` | A pop-up NSPathControl. |
| `tokenfield` | A token's menu, from a click on its pull-down arrow. |
| `toolbar` | An `NSMenuToolbarItem`. |
| `rightclick` | A posted right click on the text view (`rightMouseDown:` → `menuForEvent:`). |
| `palette` | A palette submenu (AppKit shows it inline; ours, as a submenu): ↓↓ → Return. |
| `services` | The app menu's Services submenu, filled by the system. |
| `combobox`, `share` | Not menus (NSComboBox's list window, the share popover); they print what came up instead. |
| `frames` | Snapshots of the opening animation. |

With no scenarios named it runs all but `combobox`, `share` and `frames`. Scenarios driven by posted
clicks return before their menu opens and wait 1.5 s for it. A real person (or another agent's
devhost) making another app frontmost mid-run ends menu bar menus with "the app lost focus"; rerun. Every one should print an ACTION or a return
value; `make devhost` sometimes leaves a stale binary, so after editing `devhost.m` build it with
`clang -fobjc-arc -framework AppKit -framework QuartzCore tools/devhost.m -o target/dist/devhost`.

## End to end, through the installed daemon

```sh
tools/e2e.sh /tmp/e2e context popup menubar-click customview
```

Wraps the host as `target/e2e/ErisMenusTest.app` and opens it; the installed daemon loads the
installed dylib at launch, the host waits for the `ErisMenusLoaded` class and runs the scenarios.
This is the closest thing to a person using a real app, and needs no root (but tests whatever build
is installed).

## Without root

- `make self-test`: maps the payload into the `erismenus` command itself, without dyld, as the
  daemon does, and starts it; checks the arm64e thread helper can start a signed thread.
- `erismenus targets`: which running processes the daemon loads into.

## Logs

```sh
log show --last 5m --predicate 'eventMessage BEGINSWITH "[ErisMenus]"'   # inside apps
tail /Library/Logs/erismenus.log                                          # the daemon
```

In zsh, `log` is a builtin; use `/usr/bin/log` in scripts. The payload logs when it's hooked into
an app (`drawing menus in pid …`, `6 of 6 menu hooks in place`), every menu it draws, every menu
bar click it takes (`opening title N`) or can't place, and why each menu closed. Crash reports are
in `~/Library/Logs/DiagnosticReports`; frames inside the payload show up unsymbolicated.

## Two builds in one process

To test a newer build taking over an older one: copy the dylib, give the copy its own install
name (`install_name_tool -id @rpath/older.dylib …`, then `codesign -s - -f`), and run the host with
`DYLD_INSERT_LIBRARIES=<copy>` plus the newer dylib as its argument.
