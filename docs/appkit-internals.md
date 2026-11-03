# AppKit's menus on macOS 26.4

What ErisMenus relies on, all private and found by disassembly and probing on 26.4 (25E238). Check
these first when a macOS update breaks something.

## Which code shows a menu

- `-[NSMenu _createMenuImpl]` picks a menu's implementation. In process: `NSMenuBarImpl` for the
  main menu, `NSHorizontalMenuImpl` for menus with inline content, `NSContextMenuImpl` (a
  subclass of `NSCocoaMenuImpl`) for everything else. `NSCarbonMenuImpl` exists but isn't
  created.
- Popped-up menus are drawn in the app: `NSPopupMenuWindow` (level 101) holding an `NSGlassView`
  and an `NSTableViewBackedMenuRepresentation` (an `NSTableView`) of `NSContextMenuItemView` rows.
- Every pop-up goes through `+[NSContextMenuImpl presentPopup:fromView:withContext:animated:]`.
  Its callers: `_NSPopUpMenu` (context menus, positioned menus, pop-up and pull-down buttons),
  `-[NSCocoaMenuImpl _popUpMenuRelativeToRect:inView:preferredEdge:]`, and the Swift
  `NSMenu.startContextTracking(impl:delegate:context:)`. The `popup` argument is the menu's impl.
  The tracking loop runs inside it.
- The `NSMenuContext` (`context`) carries `menu`, `event`, `frameAlignmentPoint` (screen),
  `checkedItemIndex`, `popupButtonBounds` (flipped screen, from the top of the main display),
  `minWidth`, `flags` (8: pull-down, 16: pop-up button, 4: context menu), `privateFlags`,
  `positioningOptions`, and setters for `menuCanceled`, `menuOpened`, `displayingMenu`.
- `-[NSContextMenuImpl shouldSendLifecycleNotifications]`: true while the impl has a
  `presentationContext` (set by the caller before `presentPopup:`) whose `privateFlags & 4` is 0.
  `-[NSMenu _sendMenuClosedNotification:]` only calls `menuDidClose:` if `systemAttributes` has
  bit 2 set.

## The menu bar

- The bar itself is drawn out of process; its titles are `NSMenuBarItemView`s in an in-process
  `NSMenuBarReplicantWindow` (not in `NSApp.windows`) across the top of the screen.
  `-[NSMenuBarImpl activeRepresentationView]` → `NSMenuBarRepresentation`, `subviewAtMenuIndex:`,
  `menuIndexForView:`, `frameForViewAtMenuIndex:` (in the bar's own space).
- That window's `convertRectToScreen:` answers as if it sat at the bottom of the screen; add its
  `frame.origin` by hand. `viewAtCurrentMouseLocation` / `_viewAtPoint:` go through the same
  conversion and in effect match on x alone, so they find a title for clicks anywhere on screen.
- With the pointer pushed up into the bar it's on the screen's top row: y = the screen's height
  (1169 on this MacBook), one past the title views' frames.
- A click: `-[NSMenuBarTrackingSession _handleMonitorEvent:]` (a local event monitor, which also
  sees every event a nested loop dequeues) → `_mouseDownEventHandler:` → `beginTrackingSession` →
  `[super _mouseDownEventHandler:]` → `-[NSMenuBarImpl mouseDown:]` → `handleMouseEvent:` →
  `openSubmenuFromView:` → `-[NSCocoaMenuImpl presentSubmenuWindow:fromView:parentFrame:…]`
  (AppKit's menu is up by now) → `startMonitoringEvents:` → `startRunningMenuEventLoop:` (modal;
  `_setupMenuTracking` steals key focus with `CPSStealKeyFocusReturningID`). AppKit also hands this
  handler clicks that aren't on the bar.
- Control-F2 comes through `-[NSMenuBarTrackingSession handleMenuBarFocusSymbolicHotKey:]`:
  `[mainMenu update]`, `[bar highlightNext:nil]` (the Apple menu), `beginTrackingSession`, then
  `startMonitoringEvents:`. AppKit only highlights the title; ↓ or Return opens it. Since the
  session was begun, whoever runs the loop instead has to end it (`dismissAnimated:`), or it stays
  `+[NSMenuTrackingSession currentSession]` with the background event mask still set.
- Highlighting a title without opening its menu: `-[NSMenuBarImpl setHighlightedView:]`;
  `unhighlightItemIfNeeded` clears it. The title views never draw a highlight themselves (not even
  with `_setHighlighted:menuItemView:`, which sets their `isHighlighted`): `setHighlightedView:` →
  `_selectionLayerDidChange` → `-[NSMenuBarImpl viewDidLayout]` → `+[NSMenuBarPresentationInstance
  update]` → `+[NSCGSSpace setMenuBarSettings:globalSettings:]`, whose per-display dictionary
  carries `menuBarMenuTitlesArray`: the highlighted titles' rects (`highlightedTitles`, the
  title's frame padded, e.g. {112, 7, 52, 25} for a title at {117, 0, 42, 39}) as 32-byte
  `NSData` CGRects. The window server draws the selection behind them. `_indexOfHighlightedViewInMenu`
  is -2 with nothing highlighted.

### The Apple menu

- Not one of the main menu's items: `+[NSMenuBarImpl appleMenuItem]`, an `NSMenuItem` with no
  `menu`, whose submenu (title "Apple", an `NSContextMenuImpl`) is empty until `_sendMenuPopulate`
  has its delegate, `_NSAppleMenu`, fill it (`updateMenu:withEvent:withFlags:`). Its items are
  `_NSAppleMenuItem`s targeting `_NSAppleMenu` (`_aboutThisMacRequested:`, `_sleepRequested:`, …;
  alternates such as System Information and Restart without a dialog), performed through
  `-[_NSAppleMenuItem _corePerformAction:]` like any other item.
- In the bar it's the `NSMenuBarItemView` at menu index -1 (`subviewAtMenuIndex:-1`,
  `menuIndexForView:` answers -1; `isMagicAppleMenu`), first in the replicant window's
  `_NSMenuBarBackingView`.
- If AppKit opens its own menu for a menu we've marked visible it throws
  `NSInternalInconsistencyException: Populating a menu window that is already visible`.

## Menu bar extras

- `NSStatusBar.systemStatusBar` items are `NSSceneStatusItem`s; the button
  (`NSStatusBarButton`, cell `NSStatusBarButtonCell`) sits in an in-process `NSStatusBarWindow`
  (level 25) at the item's place in the bar, which Control Center shows a replica of.
- A click or `performClick:` → `-[NSStatusBarButtonCell performClick:]` →
  `+popUpStatusBarMenu:ofItem:ofBar:inRect:ofView:withEvent:` → `-[NSCocoaMenuImpl
  popUpMenu:atLocation:width:forView:withSelectedItem:withFont:withFlags:withOptions:]` →
  `_NSPopUpMenu` → `presentPopup:`. `popUpStatusBarMenu:` brackets the pop-up with
  `_willPresentContent:` / `_didPresentContent:`, which highlight the button and take the
  highlight off again, so the highlight doesn't depend on who draws the menu.
- Its `NSMenuContext`: `exclusionRect` the item's rect in screen space (e.g. {1412, 1130, 69, 39}),
  `frameAlignmentPoint` 4 pt up inside it, `flags` 3, `minWidth` the item's width. Other pop-ups
  give a zero-width or zero-height `exclusionRect`.

## Tracking from an app that isn't frontmost

`-[NSMenuTrackingSession beginTrackingSession]` → `_setupMenuTracking`, for every menu (pop-ups
from `presentPopup:` too):

- `_RequestBackgroundMouseMovedEvents()`, and `CGSSetBackgroundEventMask(cid, saved | 0x024000fe)`
  (mouse down/up/moved/dragged, scroll wheel, other mouse down): the app gets those events even
  from outside its windows, which is how a click elsewhere closes a menu of an inactive app.
- `CPSGetKeyFocusProcess(&psn, &unused)`; unless `SameProcess` says it's us,
  `CPSStealKeyFocusReturningID(cid, 0xc80, &id)`: the keyboard comes to the menu. A menu bar
  extra's app (often an agent) or a right-clicked window behind others never has it otherwise.
- A TSM document for menu tracking; observers for the distributed
  `com.apple.HIToolbox.cancelMenuTrackingNotification` (→ `dismissAnimated:`) and
  `_UIMenuSystemShouldRevalidateNotification`; a Carbon event handler.
- `sendBeginTrackingNotifications` / `sendEndTrackingNotifications` also post the distributed
  `com.apple.HIToolbox.beginMenuTrackingNotification` / `…endMenuTrackingNotification` (no object,
  deliver immediately).

`_tearDownMenuTracking` restores the mask, `_DeclineBackgroundMouseMovedEvents()`,
`CPSReleaseKeyFocusWithID(id)`, and removes the observers.

## Other controls

All of these pop up through `presentPopup:`, checked in the development host: NSSearchField's
recents menu (`searchMenuTemplate`), a segment's menu (`setMenu:forSegment:`, on a held press),
NSPathControl in pop-up style, NSTokenField token menus (`tokenField:menuForRepresentedObject:`, a
click on the token's pull-down arrow), `NSMenuToolbarItem`, a right click on a view
(`rightMouseDown:` → `menuForEvent:`), and the Services submenu. Not menus at all:
NSComboBox's list (`NSComboBoxWindow`, level 0, an `NSVisualEffectView` around an
`NSComboTableView`) and NSSharingServicePicker (`_NSPopoverWindow`, a popover).

- Palette submenus (`NSMenuPresentationStylePalette`, an `NSHorizontalMenuImpl`): AppKit draws
  their items inline in the parent menu, as a row of images. They still open like any submenu.
- The Help menu: AppKit's menu window shows a search field as its first row, an
  `NSCustomMenuItemView` that isn't one of the menu's items. Not traced further; likely candidates
  are `-[NSCustomMenuItemView isFilterView]`, `_NSMenuFilterField` (an NSSearchField subclass) and
  `-[NSCocoaMenuImpl makeViewForMenuItemAtIndex:withFilterQuery:]`.

## Items that draw themselves

`NSMenuItem.view` puts the view in an `NSMenuItemViewer` (`-[NSMenuItem _menuItemViewer]`).
`-[NSView enclosingMenuItem]` walks up superviews until an `NSMenuItemViewer` (or
`NSCustomMenuItemView`) answers, so the view has to stay in its viewer; host the viewer. AppKit keeps
the item view at (0, 0) of whatever holds it.

## Out-of-process menus (unused)

`_createMenuImpl` checks `NSMenuIsOutOfProcess()`: the `CoreFoundation/CFPasteboardVerification`
feature flag, the privacy developer preview, or the `NSMenuEnableOutOfProcessMenus` app
config/default (`NSMenuDisableOutOfProcessMenusDueToIncompatibility` opts out). Then menus become
`NSRemoteContextMenuImpl` / `NSRemoteMenuBarImpl` and are drawn by `TrustedUIService`
(`AppKit.framework/XPCServices`, one per app, ViewBridge `NSViewServiceApplication`) from
Codable `TrustedUITransaction`s (`ShowMenuAction`, `IncrementalUpdateAction`,
`PerformMenuItemAction`, `PrepareToOpenSubmenuAwaitableAction`, …) carrying `AppKit.Menu` /
`AppKit.MenuItem`. Off by default on 26.4; probably how menus will work once Apple ships paste
verification.

## Liquid Glass

Everything glass in an app on 26.4 is one layer class, `SwiftUI.SDFLayer` (SwiftUICore, which AppKit
links). SwiftUI's `glassEffect` / `_glassEffect`, glass materials (`Material._glass(.controlCenter)`,
`.notificationCenter`, …), `GlassEffectContainer`, `.buttonStyle(.glass)`; AppKit's `NSGlassEffectView`
(and `NSGlassView`, `NSContainerConcentricGlassEffectView`), which hosts SwiftUI
(`_NSCoreHostingView<NSGlassEffectView.RootView>`) beside a `ContentHolderView`; `NSPopover`
(`_NSPopoverWindow` → `NSPopoverFrame` → `NSGlassView`); `NSAlert` (sheet and modal); glass-bezel
`NSButton`s. Control Center's binary imports `_glassEffect`, `GlassEffectContainer`,
`_Glass.explicit/tintColor` and `Material._glass(.controlCenter)` from SwiftUI, and nothing glass
from QuartzCore directly. Only QuartzCore, SwiftUICore, DesignLibrary and the Dock reference the
glass filter or the SDF classes (the Dock builds its glass from Core Animation itself).

An `SDFLayer`, built and updated in its `layoutSublayers`:

```
SwiftUI.SDFLayer
  CALayer (0×0)                       the content, hidden at its source and…
  CALayer
    CABackdropLayer  filters=[glassBackground]  delegate=the SDFLayer (luma tracking)
      CASDFLayer  effect=CASDFOutputEffect      the shape as a distance field, for the filter
        CALayer > CASDFElementLayer             one per shape (corner radius, merged shapes)
    [CALayer > CALayer > CASDFLayer CASDFGradientEffect + vibrantColorMatrix(backdrop-aware)
                         CASDFLayer CASDFFillEffect, compositingFilter destIn]   tint only
    SDFPortalLayer                             …shown here, above the glass
    CASDFLayer  effect=CASDFKeyFillHighlightEffect, vibrantColorMatrix   the rim
```

(`NSPopover`, alerts and buttons skip the outer `CALayer`.) The refraction is the
`glassBackground` `CAFilter` (`kCAFilterGlassBackground`) reading the `CASDFOutputEffect` layer under
it; without the filter that layer draws its distance field as a rainbow normal map. A
`CASDFLayer` whose effect is a `CASDFFillEffect` (`color`, a `CGColor`) draws its shape solid. The
tint's color matrix maps what's behind it (`inputBackdropAware`) and carries the tint in its
offsets, so over black it comes out as the tint itself (in an active window; inactive windows
show tints gray, glass or not). `-[CABackdropLayer setEnabled:NO]` stops the capture; with the
filter still on, a disabled backdrop draws nothing at all, sublayers included. The capture is
taken at a fraction of the resolution, so a backdrop without its filter shows a blurry copy of
what's behind in the corners of its rect. SwiftUI reads `effect` back on later layouts, so an
effect has to keep its class (an `as!` would trap): ErisMenus changes it to a subclass of
`CASDFOutputEffect` whose `configureLayer:transaction:` (what puts an effect into the render
tree) is a `CASDFFillEffect`'s. Copies (`copyWithZone:`, `initWithLayer:`) keep the subclass.
KVC on a filter key path that's gone (`filters.glassBackground.inputAmount`) is ignored.

What `glass.rs` does after each `layoutSublayers`: takes the glass filter off and disables the
backdrop, reclasses the output effect (black, or 0.11 gray for glass inside glass), hides the
highlight layers (`CASDFKeyFillHighlightEffect`, `CASDFGlassHighlightEffect`,
`CASDFGlassDisplacementEffect`), marks the `SDFLayer` (an associated object, since its filter is
gone after the first pass) so `backdropLayer:didChangeLuma:` reports 0 (and calls it once with 0
then, since the disabled backdrop won't report again), and on the main thread
gives the glass's `NSGlassEffectView`, and the content view of a window the glass fills (90% of
it), the dark appearance. At install it does the same to the `SDFLayer`s already in
`NSApp.windows`, which may not be laid out again for a while.

Glass inside glass: SwiftUI's has the outer `SDFLayer` among its ancestors; AppKit's has another
`NSGlassEffectView` above the one it belongs to (whose layer is two up from its `SDFLayer`).

Content colors come from the glass's appearance: an `NSGlassEffectView` in light mode gets dark
text, and adaptive glass (`backdropLayer:didChangeLuma:`, luma 0–1) flips its content dark over
bright backgrounds. A popover's content isn't in its glass view but beside it, under an
`NSPopoverFrame` with a `VibrantLight`/`VibrantDark` appearance AppKit sets after showing.

Tooltips aren't glass: `NSToolTipPanel` → `NSVisualEffectView` (blur `CABackdropLayer`, tint
layers) → `NSCustomToolTipDrawView`. `-[NSToolTipManager installContentView:forToolTip:toolTipWindow:isNew:]`
runs for every tooltip shown (`view` is nil for plain ones); the effect view's own layer and
sublayers are reset by AppKit after it, but `-[NSVisualEffectView _setClear:YES]` removes the
material for good. `_displayTemporaryToolTipForView:withString:` shows one without hovering.

Not glass, left alone: `NSVisualEffectView` materials (sidebars, HUDs), the titlebar's scroll
pocket blur (`NSScrollPocket`, `variableBlur`), `NSLessExpensiveSubduedGlassEffectView` (a
vibrancy view, no backdrop), Control Center's `BentoBackdrop` (a `CABackdropLayer` subclass with a
Gaussian blur).

## Other gotchas

- The main dispatch queue isn't serviced in `NSEventTrackingRunLoopMode`; timers in common modes
  are.
- A non-key, non-activating panel doesn't pass a first click to its views; send mouse events
  straight to the view under the pointer.
- Two dylibs with the same install name load once; `dlopen` of the second returns the first.
