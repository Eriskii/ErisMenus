# A custom accent color (research, not built yet)

How to make the system accent an arbitrary color (e.g. `#FF6B00`) in every app on macOS 26.4
(25E238), found by probing and disassembly in test processes. Nothing here is in the payload yet.
The prototype and probes are in `tools/accent/`, and `accent/overview.png` shows them working.

## How AppKit picks the accent

`AppleAccentColor` (global default) is an index into Apple's presets, read in each process
(`-AppleAccentColor 3` on the command line changes one app). For a preset,
`_NSAppearanceTintColorNameFromUserAccentColor` turns the index into a catalog name
(`controlAccentPurpleColor`, …) and `_UpdateSystemAppearanceWithNamedTintColor` hands CoreUI that
name. That's why swizzling `+[NSColor controlAccentColor]`, the 13 `+controlAccent<Preset>Color`
methods, `-[CUINamedColor cgColor]`, or tinting the appearance (`appearanceByApplyingTintColor:`)
leaves the controls in the preset color.

With Multicolor (`AppleAccentColor` = -2) AppKit takes an arbitrary color instead:
`_UpdateSystemAppearancesWithAccentColor` and `-[NSCompositeAppearance _updateSystemTintColor]` use
`[NSApp _effectiveAccentColor]`: the `_accentColor` ivar, else the app's Info.plist accent
(`NSColorMainBundleCustomAccentColor`). `-[NSApplication _setAccentColor:]` sets the ivar (AppKit's
own debug menu, `-[NSDebugCustomAccentColorViewController updateCustomAccentColor:]`, does just that).

## The recipe (verified in test processes)

On the main thread, in this order:

1. **Multicolor for this process only.** `NSColorGetUserAccentColor` is a C function AppKit calls
   directly, but it reads `[[NSUserDefaults standardUserDefaults] objectForKey:@"AppleAccentColor"]`.
   Put `AppleAccentColor = -2` in the argument domain at runtime:
   `[ud setVolatileDomain:<existing NSArgumentDomain merged with AppleAccentColor=-2> forName:@"NSArgumentDomain"]`.
   Nothing is written to disk, `CFPreferencesCopyAppValue` sees it too, and it outranks the global
   setting, so a preset picked in System Settings later doesn't undo it.
2. **Folder icons:** swizzle `-[SLSIconAppearanceConfiguration resolvedIconTintColor]` (an AppKit
   category). With `iconTintColorName == 0` ("follow the accent") it returns nil in Multicolor mode,
   which makes folders blue; return our color. Never call that class's `-save` (the global setter).
3. `[NSApp _setAccentColor:color]` with a color we keep a strong reference to (a static sRGB color
   is enough; a `colorWithName:dynamicProvider:` color gives separate light and dark shades). It
   asserts off the main thread, and it overrides an app's own `NSAccentColorName`.
4. `[[NSSystemAppearanceProxy systemProxy] _controlTintChanged:nil]`: one of only two writers of
   AppKit's cached accent index (the other is `NSColorSetUserAccentColor`), and the path a change in
   System Settings takes. It resets the cache, recaches dynamic colors, refreshes appearances and
   icons, and posts `NSControlTintDidChangeNotification`. Required when the app is already running
   (until the cached index is -2, `_setAccentColor:` only stores the ivar).

A new color later: `_setAccentColor:` again, then `_controlTintChanged:` again. Look up every class
and selector by name and leave the accent alone if any is missing.

## What follows the color

Verified, at launch and applied to windows already showing, in light and dark:

- AppKit controls: checkbox, radio, `NSSwitch`, `NSSlider`, `NSProgressIndicator`, the selected
  segment, the default button, `controlAccentColor`, folder icons. Pressed states lighten the way the
  presets do.
- Derived colors, computed by AppKit itself: for `FF6B00`, `selectedContentBackgroundColor` is
  e15600 (light) / d14c00 (dark), `keyboardFocusIndicatorColor` f45700 / ff981a at 50%.
- SwiftUI: toggles (checkbox and switch), `.borderedProminent`, default buttons, segmented and radio
  pickers, slider, progress view, `Color.accentColor`, `.tint`, list and sidebar selection.
- Layers that copied a `CGColor` when they were built keep the old one, as with stock macOS when the
  user changes the accent.

Not covered:

- **WebKit form controls** (verified): drawn in `com.apple.WebKit.WebContent`, which reads its own
  `AppleAccentColor`. The UI process forwards the app's accent (`PageClientImpl::accentColor` →
  `WebPage::SetAccentColor`), but WebContent only uses it when `PageClientImpl::appUsesCustomAccentColor()`
  is true, which checks Info.plist `NSAccentColorName` once and caches it. Covering it means running
  the recipe in WebContent (untested).
- Anything not injected: WindowServer's selection tint, widgets, QuickLook thumbnails, the Dock,
  Catalyst/iOS apps (untested).
- Inferred, not verified: stock AppKit menus highlight blue in Multicolor mode
  (`ignoresMulticolorAccent` on `NSPopupMenuWindow`), which doesn't matter where ErisMenus draws
  them; System Settings would show Multicolor selected; sandboxed apps' remote open panels need
  their service process covered; an app that resets its argument domain loses the override.

## Patching AppKit's code (works, not needed)

`vm_protect(page, RW | VM_PROT_COPY)` on AppKit's text, writing `movn x0, #1; ret` into
`NSColorGetUserAccentColor`, then back to RX and an icache flush, makes it return -2
(`tools/accent/patch.m`). Worked in arm64 ad hoc, arm64 hardened runtime and arm64e
(`runtime,kill,hard`) test binaries on this machine (debugging restrictions off); untested in Apple's
own binaries. The argument domain does the same without touching code.

## Tools

`tools/accent/`: `accent.m` (the prototype payload, `ERIS_ACCENT=RRGGBB`), `board.m` and
`sui.swift` (AppKit and SwiftUI boards, `LIGHT`/`DARK`/`LATE`/`PRESSED`), `web.m` (WebKit),
`colors.m` (system color dump), `icon.m` (icon appearance config), `xref.m` (code that reads or
writes an address), `meths.m` (grep every ObjC method), `strs.m`/`strs2.m` (strings in shared-cache
images), `dis.sh` and `bt.py`/`trace1.lldb` (lldb disassembly and backtraces at a breakpoint).
