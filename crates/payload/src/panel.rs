//! One open menu on screen: a borderless panel holding a layer tree, the body unrolling from the
//! corner it opens at, and the highlight sliding from row to row.

use alloc::vec::Vec;

use menus_core::keys;
use menus_core::layout::{self, Anchor, MenuLayout, Metrics, Opening, RowKind, RowSpec};
use menus_core::math;
use menus_core::shape;
use menus_core::spring::{Animation, Spring};
use menus_core::{Point, Rect};

use crate::model::Row;
use crate::msg;
use crate::objc::{Id, NIL, class, ns, nsstring, release, retain, sel, utf16};
use crate::style::{self, palette};
use crate::sys::{self, CGColorRef, CGPathRelease, CGPoint, CGRect, CGSize, rect};

/// Room around the body for its shadow.
const MARGIN: f64 = 28.0;
/// The tallest an item's own image is drawn.
const MAX_IMAGE: f64 = 16.0;
/// NSPopUpMenuWindowLevel.
const MENU_LEVEL: isize = 101;

/// The layers that draw one row; NIL where the row has no such thing.
#[derive(Clone, Copy)]
struct RowLayers {
    title: Id,
    subtitle: Id,
    key: Id,
    image: Id,
    state: Id,
    chevron: Id,
    line: Id,
}

impl RowLayers {
    const NONE: RowLayers = RowLayers { title: NIL, subtitle: NIL, key: NIL, image: NIL, state: NIL, chevron: NIL, line: NIL };

    fn all(&self) -> [Id; 7] {
        [self.title, self.subtitle, self.key, self.image, self.state, self.chevron, self.line]
    }
}

pub struct Panel {
    pub menu: Id,
    pub rows: Vec<Row>,
    pub layout: MenuLayout,
    /// The body's frame on screen.
    pub frame: Rect,
    pub opening: Opening,
    pub highlighted: Option<usize>,
    pub scroll: f64,
    window: Id,
    /// The window's content view: the layer-hosting view that draws the menu, and above it the
    /// views of items that draw themselves.
    container: Id,
    /// (row, the AppKit view holding its item's view), for items that draw themselves with views:
    /// the item's `NSMenuItemViewer`, which keeps the view at its origin and is how the view finds
    /// its item (`enclosingMenuItem`); retained.
    item_views: Vec<(usize, Id)>,
    root: Id,
    body: Id,
    mask: Id,
    content: Id,
    highlight: Id,
    row_layers: Vec<RowLayers>,
    font: Id,
    scale: f64,
    animation: Animation,
    width: Spring,
    height: Spring,
    reveal: Spring,
    highlight_top: Spring,
    highlight_height: Spring,
    highlight_alpha: Spring,
}

impl Panel {
    /// Lays out `rows`, puts them on screen at `anchor` (a row index in an anchor means one of
    /// `rows`) and starts unrolling. None if there's nothing to show.
    pub fn open(
        menu: Id,
        rows: Vec<Row>,
        anchor: Anchor,
        min_width: f64,
        screen: Rect,
        animation: Animation,
    ) -> Option<Panel> {
        if rows.is_empty() {
            return None;
        }
        let metrics = Metrics::default();
        let font = retain(style::menu_font(menu));
        let specs: Vec<RowSpec> = rows.iter().map(|r| measure(r, font)).collect();
        let layout = layout::lay_out(
            &specs,
            &metrics,
            min_width,
            screen.width / 2.0,
            screen.height - 2.0 * metrics.screen_margin,
        );
        let placement = layout::place(&layout, anchor, screen);
        let frame = placement.frame;

        let window_frame = rect(frame.x - MARGIN, frame.y - MARGIN, frame.width + 2.0 * MARGIN, frame.height + 2.0 * MARGIN);
        let window = new_window(window_frame);
        let scale = msg!(f64; window, c"backingScaleFactor");
        let root = new_layer(c"CALayer");
        let bounds = rect(0.0, 0.0, window_frame.size.width, window_frame.size.height);
        msg!((); root, c"setBounds:", bounds => CGRect);
        let container = msg!(Id; class(c"NSView"), c"alloc");
        let container = msg!(Id; container, c"initWithFrame:", bounds => CGRect);
        msg!((); container, c"setWantsLayer:", true => bool);
        let view = msg!(Id; class(c"NSView"), c"alloc");
        let view = msg!(Id; view, c"initWithFrame:", bounds => CGRect);
        msg!((); view, c"setLayer:", root => Id);
        msg!((); view, c"setWantsLayer:", true => bool);
        msg!((); container, c"addSubview:", view => Id);
        release(view);
        msg!((); window, c"setContentView:", container => Id);

        let p = palette();
        let body = new_layer(c"CAShapeLayer");
        msg!((); body, c"setFillColor:", p.body => CGColorRef);
        msg!((); body, c"setShadowColor:", p.shadow => CGColorRef);
        msg!((); body, c"setShadowOpacity:", 0.32f32 => f32);
        msg!((); body, c"setShadowRadius:", 18.0 => f64);
        msg!((); body, c"setShadowOffset:", CGSize { width: 0.0, height: -8.0 } => CGSize);
        msg!((); root, c"addSublayer:", body => Id);

        let clip = new_layer(c"CALayer");
        msg!((); clip, c"setFrame:", rect(0.0, 0.0, window_frame.size.width, window_frame.size.height) => CGRect);
        let mask = new_layer(c"CAShapeLayer");
        msg!((); mask, c"setFillColor:", p.body => CGColorRef);
        msg!((); clip, c"setMask:", mask => Id);
        msg!((); root, c"addSublayer:", clip => Id);
        release(clip);

        let content = new_layer(c"CALayer");
        msg!((); clip, c"addSublayer:", content => Id);
        let highlight = new_layer(c"CALayer");
        msg!((); highlight, c"setBackgroundColor:", p.highlight => CGColorRef);
        msg!((); highlight, c"setCornerRadius:", metrics.highlight_radius => f64);
        msg!((); highlight, c"setCornerCurve:", nsstring(c"continuous") => Id);
        msg!((); highlight, c"setOpacity:", 0.0f32 => f32);
        msg!((); content, c"addSublayer:", highlight => Id);

        let collapsed = |full: f64, fraction: f64| full * fraction;
        let mut panel = Panel {
            menu: retain(menu),
            rows,
            layout,
            frame,
            opening: placement.opening,
            highlighted: None,
            scroll: 0.0,
            window,
            container,
            item_views: Vec::new(),
            root,
            body,
            mask,
            content,
            highlight,
            row_layers: Vec::new(),
            font,
            scale,
            animation,
            width: Spring::new(collapsed(frame.width, animation.collapsed_width), animation.widen).with_epsilon(0.05),
            height: Spring::new(collapsed(frame.height, animation.collapsed_height), animation.unroll).with_epsilon(0.05),
            reveal: Spring::new(0.0, animation.reveal),
            highlight_top: Spring::new(0.0, animation.slide).with_epsilon(0.05),
            highlight_height: Spring::new(0.0, animation.slide).with_epsilon(0.05),
            highlight_alpha: Spring::new(0.0, animation.reveal),
        };
        panel.width.animate(frame.width);
        panel.height.animate(frame.height);
        panel.reveal.animate(1.0);
        panel.build_rows();
        panel.adopt_item_views();
        panel.render();
        msg!((); window, c"orderFrontRegardless");
        Some(panel)
    }

    pub fn window(&self) -> Id {
        self.window
    }

    /// The view of the item under `p` (on screen), for items that draw themselves.
    pub fn item_view_at(&self, p: Point) -> Option<Id> {
        let row = self.row_at(p)?;
        self.item_views.iter().find(|&&(r, _)| r == row).map(|&(_, v)| v)
    }

    /// The view under `p` (in window coordinates) inside an item's own view, for sending it mouse
    /// events; NIL when `p` isn't on one.
    pub fn item_view_hit(&self, p: CGPoint) -> Id {
        let origin = msg!(CGRect; self.window, c"frame").origin;
        if self.item_view_at(Point::new(origin.x + p.x, origin.y + p.y)).is_none() {
            return NIL;
        }
        msg!(Id; self.container, c"hitTest:", p => CGPoint)
    }

    /// Moves the views of items that draw themselves into our window, as AppKit's menus do.
    fn adopt_item_views(&mut self) {
        for row in 0..self.rows.len() {
            let view = self.rows[row].view();
            if view.is_null() {
                continue;
            }
            let item = self.rows[row].item();
            let viewer = if msg!(bool; item, c"respondsToSelector:", sel(c"_menuItemViewer") => *const core::ffi::c_void) {
                msg!(Id; item, c"_menuItemViewer")
            } else {
                NIL
            };
            // Without AppKit's holder the view itself goes in, and can't find its item.
            let holder = retain(if viewer.is_null() { view } else { viewer });
            msg!((); holder, c"removeFromSuperview");
            msg!((); self.container, c"addSubview:", holder => Id);
            self.item_views.push((row, holder));
        }
    }

    pub fn contains(&self, p: Point) -> bool {
        self.frame.contains(p)
    }

    /// The row under `p`, on screen.
    pub fn row_at(&self, p: Point) -> Option<usize> {
        self.layout.row_at(Point::new(p.x - self.frame.x, p.y - self.frame.y), self.scroll)
    }

    /// Where `row` is on screen.
    pub fn row_rect(&self, row: usize) -> Rect {
        let r = self.layout.row_rect(row, self.scroll);
        Rect::new(self.frame.x + r.x, self.frame.y + r.y, r.width, r.height)
    }

    pub fn selectable(&self, row: usize) -> bool {
        self.rows[row].enabled()
    }

    pub fn set_highlight(&mut self, row: Option<usize>) {
        if row == self.highlighted {
            return;
        }
        match row {
            Some(r) => {
                let f = self.layout.rows[r];
                if self.highlighted.is_none() && self.highlight_alpha.value < 0.05 {
                    self.highlight_top.set(f.top);
                    self.highlight_height.set(f.height);
                } else {
                    self.highlight_top.animate(f.top);
                    self.highlight_height.animate(f.height);
                }
                self.highlight_alpha.animate(1.0);
                self.show_row(r);
            }
            None => self.highlight_alpha.animate(0.0),
        }
        self.highlighted = row;
    }

    /// Scrolls `row` into view.
    pub fn show_row(&mut self, row: usize) {
        self.scroll = self.layout.scroll_to_show(row, self.scroll);
    }

    pub fn scroll_by(&mut self, dy: f64) {
        self.scroll = (self.scroll + dy).clamp(0.0, self.layout.max_scroll());
    }

    /// Shows each row's alternate for `modifiers`; widths were measured for every variant.
    pub fn show_variants_for(&mut self, modifiers: u64) {
        for i in 0..self.rows.len() {
            if self.rows[i].show_variant_for(modifiers) {
                self.fill_row(i);
            }
        }
    }

    /// Advances the springs by `dt` and redraws; true while anything is still moving.
    pub fn tick(&mut self, dt: f64) -> bool {
        let springs = [
            &mut self.width,
            &mut self.height,
            &mut self.reveal,
            &mut self.highlight_top,
            &mut self.highlight_height,
            &mut self.highlight_alpha,
        ];
        let mut moving = false;
        for s in springs {
            s.step(dt);
            moving |= !s.is_resting();
        }
        self.render();
        moving
    }

    /// Fades the panel out and lets it go.
    pub fn close(self, fade: bool) {
        // Item views go back to their items right away, so the menu can show them again.
        for &(_, holder) in &self.item_views {
            msg!((); holder, c"removeFromSuperview");
        }
        if fade {
            let context = class(c"NSAnimationContext");
            msg!((); context, c"beginGrouping");
            msg!((); msg!(Id; context, c"currentContext"), c"setDuration:", 0.12 => f64);
            msg!((); msg!(Id; self.window, c"animator"), c"setAlphaValue:", 0.0 => f64);
            msg!((); context, c"endGrouping");
            msg!((); self.window, c"performSelector:withObject:afterDelay:", sel(c"orderOut:") => *const core::ffi::c_void, NIL => Id, 0.14 => f64);
        } else {
            msg!((); self.window, c"orderOut:", NIL => Id);
        }
    }

    fn render(&mut self) {
        let catransaction = class(c"CATransaction");
        msg!((); catransaction, c"begin");
        msg!((); catransaction, c"setDisableActions:", true => bool);

        let (full_w, full_h) = (self.frame.width, self.frame.height);
        let (w, h) = (self.width.value.max(0.0), self.height.value.max(0.0));
        let x = MARGIN + if self.opening.left { full_w - w } else { 0.0 };
        let y = MARGIN + if self.opening.up { 0.0 } else { full_h - h };
        let m = &self.layout.metrics;
        let elements = shape::rounded_rect(Rect::new(0.0, 0.0, w, h), m.corner_radius, m.smoothing);
        let path = sys::path(&elements, x, y);
        msg!((); self.body, c"setPath:", path => sys::CGPathRef);
        msg!((); self.body, c"setShadowPath:", path => sys::CGPathRef);
        msg!((); self.mask, c"setPath:", path => sys::CGPathRef);
        unsafe { CGPathRelease(path) };

        let content_h = self.layout.content_height;
        let content_y = MARGIN + full_h - content_h + self.scroll;
        msg!((); self.content, c"setFrame:", rect(MARGIN, content_y, full_w, content_h) => CGRect);
        msg!((); self.content, c"setOpacity:", self.reveal.value.clamp(0.0, 1.0) as f32 => f32);

        // The moving highlight stretches along its way, jelly-like.
        let stretch = (self.highlight_top.velocity.abs() * self.animation.stretch).min(10.0);
        let top = self.highlight_top.value - stretch / 2.0;
        let height = self.highlight_height.value + stretch;
        let hy = content_h - top - height;
        msg!((); self.highlight, c"setFrame:", rect(m.inset, hy, full_w - 2.0 * m.inset, height) => CGRect);
        msg!((); self.highlight, c"setOpacity:", self.highlight_alpha.value.clamp(0.0, 1.0) as f32 => f32);

        msg!((); catransaction, c"commit");

        // Item views show once the body has mostly unrolled, and scroll with the rows.
        for &(row, view) in &self.item_views {
            let r = self.layout.row_rect(row, self.scroll);
            let visible = self.reveal.value > 0.6 && r.y >= 0.0 && r.max_y() <= full_h;
            msg!((); view, c"setFrame:", rect(MARGIN, MARGIN + r.y, full_w, r.height) => CGRect);
            msg!((); view, c"setHidden:", !visible => bool);
        }
    }

    fn build_rows(&mut self) {
        self.row_layers = alloc::vec![RowLayers::NONE; self.rows.len()];
        for i in 0..self.rows.len() {
            self.fill_row(i);
        }
    }

    /// (Re)makes row `i`'s layers for the variant it shows.
    fn fill_row(&mut self, i: usize) {
        for l in self.row_layers[i].all() {
            if !l.is_null() {
                msg!((); l, c"removeFromSuperlayer");
                release(l);
            }
        }
        let mut layers = RowLayers::NONE;
        let row = &self.rows[i];
        if !row.view().is_null() {
            self.row_layers[i] = layers;
            return;
        }
        let m = self.layout.metrics;
        let cols = self.layout.columns;
        let f = self.layout.rows[i];
        let row_y = self.layout.content_height - f.top - f.height;
        let mid = row_y + f.height / 2.0;
        let p = palette();
        let full_w = self.frame.width;

        match row.kind {
            RowKind::Separator => {
                let line = new_layer(c"CALayer");
                msg!((); line, c"setBackgroundColor:", p.separator => CGColorRef);
                let x = m.inset + m.text_inset;
                msg!((); line, c"setFrame:", rect(x, math::round(mid), full_w - 2.0 * x, 1.0) => CGRect);
                layers.line = line;
            }
            RowKind::Header => {
                let font = style::system_font(11.0, style::SEMIBOLD);
                let text = style::attributed(msg!(Id; row.item(), c"title"), font, p.secondary_text);
                let lh = style::line_height(font);
                let x = m.inset + m.text_inset;
                layers.title = self.text_layer(text, rect(x, mid - lh / 2.0, cols.key_right - x, lh), false);
                release(text);
            }
            RowKind::Item => {
                let item = row.item();
                let enabled = row.enabled();
                let color = if enabled { p.text } else { p.disabled_text };
                let lh = style::line_height(self.font);
                let subtitle = if msg!(bool; item, c"respondsToSelector:", sel(c"subtitle") => *const core::ffi::c_void) {
                    msg!(Id; item, c"subtitle")
                } else {
                    NIL
                };
                let has_subtitle = !subtitle.is_null() && msg!(usize; subtitle, c"length") > 0;
                let title_y = if has_subtitle { mid + 1.0 } else { mid - lh / 2.0 };

                let key_text = keys::glyphs(&utf16(msg!(Id; item, c"keyEquivalent")), msg!(u64; item, c"keyEquivalentModifierMask"));
                let submenu = !row.submenu().is_null();
                let mut title_right = cols.key_right;
                if !key_text.is_empty() {
                    let key = style::attributed(ns(&key_text), self.font, if enabled { p.secondary_text } else { p.disabled_text });
                    let kw = style::text_size(key).width;
                    title_right = cols.key_right - kw - 12.0;
                    layers.key = self.text_layer(key, rect(cols.title_x, title_y, cols.key_right - cols.title_x, lh), true);
                    release(key);
                } else if submenu {
                    title_right = cols.chevron_x - 8.0;
                    let chevron = style::symbol(c"chevron.right", 11.0, style::SEMIBOLD);
                    layers.chevron = self.image_layer(chevron, cols.chevron_x, mid, Some(if enabled { p.glyph } else { p.dim_glyph }));
                }

                let indent = msg!(isize; item, c"indentationLevel").max(0) as f64 * m.indentation;
                let attributed_title = msg!(Id; item, c"attributedTitle");
                let title = if attributed_title.is_null() {
                    style::attributed(msg!(Id; item, c"title"), self.font, color)
                } else {
                    style::recolored(attributed_title, self.font, color)
                };
                let x = cols.title_x + indent;
                layers.title = self.text_layer(title, rect(x, title_y, (title_right - x).max(0.0), lh), false);
                release(title);

                if has_subtitle {
                    let font = style::system_font(11.0, 0.0);
                    let text = style::attributed(subtitle, font, if enabled { p.secondary_text } else { p.disabled_text });
                    let slh = style::line_height(font);
                    layers.subtitle = self.text_layer(text, rect(x, mid - slh - 1.0, (title_right - x).max(0.0), slh), false);
                    release(text);
                }

                let state = msg!(isize; item, c"state");
                if state != 0 {
                    let name = if state > 0 { c"checkmark" } else { c"minus" };
                    let glyph = style::symbol(name, 11.0, style::SEMIBOLD);
                    let x = cols.state_x + (m.state_width - 12.0) / 2.0;
                    layers.state = self.image_layer(glyph, x, mid, Some(if enabled { p.glyph } else { p.dim_glyph }));
                }

                let image = msg!(Id; item, c"image");
                if !image.is_null() {
                    let tint = msg!(bool; image, c"isTemplate").then_some(if enabled { p.glyph } else { p.dim_glyph });
                    layers.image = self.image_layer(image, cols.image_x, mid, tint);
                    if !enabled && tint.is_none() {
                        msg!((); layers.image, c"setOpacity:", 0.4f32 => f32);
                    }
                }
            }
        }
        for l in layers.all() {
            if !l.is_null() {
                msg!((); self.content, c"addSublayer:", l => Id);
            }
        }
        self.row_layers[i] = layers;
    }

    /// A +1 text layer drawing `string` (attributed) in `frame`.
    fn text_layer(&self, string: Id, frame: CGRect, right: bool) -> Id {
        let layer = new_layer(c"CATextLayer");
        msg!((); layer, c"setString:", string => Id);
        msg!((); layer, c"setContentsScale:", self.scale => f64);
        msg!((); layer, c"setTruncationMode:", nsstring(c"end") => Id);
        if right {
            msg!((); layer, c"setAlignmentMode:", nsstring(c"right") => Id);
        }
        msg!((); layer, c"setFrame:", frame => CGRect);
        layer
    }

    /// A +1 layer drawing `image` with its left edge at `x`, centered on `mid`, no taller than
    /// MAX_IMAGE; a template image is drawn as a mask over `tint`.
    fn image_layer(&self, image: Id, x: f64, mid: f64, tint: Option<CGColorRef>) -> Id {
        if image.is_null() {
            return NIL;
        }
        let size = msg!(CGSize; image, c"size");
        let fit = if size.height > MAX_IMAGE { MAX_IMAGE / size.height } else { 1.0 };
        let (w, h) = (size.width * fit, size.height * fit);
        let frame = rect(x, math::round(mid - h / 2.0), w, h);
        let contents = msg!(Id; image, c"layerContentsForContentsScale:", self.scale => f64);
        let layer = new_layer(c"CALayer");
        msg!((); layer, c"setFrame:", frame => CGRect);
        match tint {
            Some(color) => {
                msg!((); layer, c"setBackgroundColor:", color => CGColorRef);
                let mask = new_layer(c"CALayer");
                msg!((); mask, c"setFrame:", rect(0.0, 0.0, w, h) => CGRect);
                msg!((); mask, c"setContents:", contents => Id);
                msg!((); mask, c"setContentsGravity:", nsstring(c"resizeAspect") => Id);
                msg!((); mask, c"setContentsScale:", self.scale => f64);
                msg!((); layer, c"setMask:", mask => Id);
                release(mask);
            }
            None => {
                msg!((); layer, c"setContents:", contents => Id);
                msg!((); layer, c"setContentsGravity:", nsstring(c"resizeAspect") => Id);
                msg!((); layer, c"setContentsScale:", self.scale => f64);
            }
        }
        layer
    }
}

impl Drop for Panel {
    fn drop(&mut self) {
        for layers in &self.row_layers {
            for l in layers.all() {
                release(l);
            }
        }
        for &(_, view) in &self.item_views {
            release(view);
        }
        for l in [self.highlight, self.content, self.mask, self.body, self.root, self.font, self.menu, self.container] {
            release(l);
        }
        release(self.window);
    }
}

/// What row `row` needs room for, across all its variants.
fn measure(row: &Row, font: Id) -> RowSpec {
    let view = row.view();
    if !view.is_null() {
        let frame = msg!(CGRect; view, c"frame");
        return RowSpec::custom(menus_core::Size::new(frame.size.width, frame.size.height));
    }
    let mut spec = RowSpec { kind: row.kind, ..RowSpec::item(0.0) };
    match row.kind {
        RowKind::Separator => {}
        RowKind::Header => {
            let font = style::system_font(11.0, style::SEMIBOLD);
            let text = style::attributed(msg!(Id; row.item(), c"title"), font, palette().text);
            spec.title_width = style::text_size(text).width;
            release(text);
        }
        RowKind::Item => {
            for item in row.items() {
                let attributed_title = msg!(Id; item, c"attributedTitle");
                let title = if attributed_title.is_null() {
                    style::attributed(msg!(Id; item, c"title"), font, palette().text)
                } else {
                    style::recolored(attributed_title, font, palette().text)
                };
                spec.title_width = spec.title_width.max(style::text_size(title).width);
                release(title);
                let key = keys::glyphs(&utf16(msg!(Id; item, c"keyEquivalent")), msg!(u64; item, c"keyEquivalentModifierMask"));
                if !key.is_empty() {
                    let text = style::attributed(ns(&key), font, palette().text);
                    spec.key_width = spec.key_width.max(style::text_size(text).width);
                    release(text);
                }
                if msg!(bool; item, c"respondsToSelector:", sel(c"subtitle") => *const core::ffi::c_void) {
                    let subtitle = msg!(Id; item, c"subtitle");
                    if !subtitle.is_null() && msg!(usize; subtitle, c"length") > 0 {
                        spec.has_subtitle = true;
                        let text = style::attributed(subtitle, style::system_font(11.0, 0.0), palette().text);
                        spec.subtitle_width = spec.subtitle_width.max(style::text_size(text).width);
                        release(text);
                    }
                }
                let image = msg!(Id; item, c"image");
                if !image.is_null() {
                    let size = msg!(CGSize; image, c"size");
                    let fit = if size.height > MAX_IMAGE { MAX_IMAGE / size.height } else { 1.0 };
                    spec.image_width = spec.image_width.max(size.width * fit);
                }
                spec.has_state |= msg!(isize; item, c"state") != 0;
                spec.indent = spec.indent.max(msg!(isize; item, c"indentationLevel").max(0) as usize);
                spec.submenu |= !msg!(Id; item, c"submenu").is_null();
            }
        }
    }
    spec
}

/// A +1 layer of `class`.
fn new_layer(class_name: &core::ffi::CStr) -> Id {
    retain(msg!(Id; class(class_name), c"layer"))
}

/// A +1 borderless panel at the menu level that never takes focus from the app.
fn new_window(frame: CGRect) -> Id {
    const BORDERLESS_NONACTIVATING: usize = 1 << 7;
    const BUFFERED: usize = 2;
    const ALL_SPACES_TRANSIENT_FULL_SCREEN: usize = 1 | (1 << 3) | (1 << 8);
    const NO_ANIMATION: isize = 2;
    let window = msg!(Id; class(c"NSPanel"), c"alloc");
    let window = msg!(Id; window, c"initWithContentRect:styleMask:backing:defer:",
        frame => CGRect, BORDERLESS_NONACTIVATING => usize, BUFFERED => usize, false => bool);
    msg!((); window, c"setReleasedWhenClosed:", false => bool);
    msg!((); window, c"setOpaque:", false => bool);
    msg!((); window, c"setBackgroundColor:", msg!(Id; class(c"NSColor"), c"clearColor") => Id);
    msg!((); window, c"setHasShadow:", false => bool);
    msg!((); window, c"setLevel:", MENU_LEVEL => isize);
    msg!((); window, c"setHidesOnDeactivate:", false => bool);
    msg!((); window, c"setAnimationBehavior:", NO_ANIMATION => isize);
    msg!((); window, c"setCollectionBehavior:", ALL_SPACES_TRANSIENT_FULL_SCREEN => usize);
    msg!((); window, c"setFrame:display:", frame => CGRect, false => bool);
    // Items that draw themselves do so on our black, so in dark mode.
    let dark = msg!(Id; class(c"NSAppearance"), c"appearanceNamed:", nsstring(c"NSAppearanceNameDarkAqua") => Id);
    msg!((); window, c"setAppearance:", dark => Id);
    window
}
