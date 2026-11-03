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
- Control-F2 comes through `handleMenuBarFocusSymbolicHotKey:` → `startMonitoringEvents:`.
- Highlighting a title without opening its menu: `-[NSMenuBarImpl setHighlightedView:]`;
  `unhighlightItemIfNeeded` clears it. The Apple menu isn't one of the main menu's items.
- If AppKit opens its own menu for a menu we've marked visible it throws
  `NSInternalInconsistencyException: Populating a menu window that is already visible`.

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

## Other gotchas

- The main dispatch queue isn't serviced in `NSEventTrackingRunLoopMode`; timers in common modes
  are.
- A non-key, non-activating panel doesn't pass a first click to its views; send mouse events
  straight to the view under the pointer.
- Two dylibs with the same install name load once; `dlopen` of the second returns the first.
