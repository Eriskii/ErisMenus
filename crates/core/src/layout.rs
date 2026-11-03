//! A menu's rows and columns, and where on screen the whole menu goes.

use alloc::vec::Vec;

use crate::geometry::{Point, Rect, Size};
use crate::math;

/// Sizes in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Metrics {
    pub row_height: f64,
    /// A row with a subtitle under its title.
    pub subtitle_row_height: f64,
    pub separator_height: f64,
    pub header_height: f64,
    /// Above the first row and below the last.
    pub padding: f64,
    /// Between the body's sides and the highlight.
    pub inset: f64,
    /// Between the highlight's sides and what's in the row.
    pub text_inset: f64,
    pub state_width: f64,
    pub image_gap: f64,
    pub indentation: f64,
    /// Between a title and its key equivalent.
    pub key_gap: f64,
    pub chevron_width: f64,
    pub min_width: f64,
    pub corner_radius: f64,
    pub highlight_radius: f64,
    pub smoothing: f64,
    /// How close to the screen's edges a menu may come.
    pub screen_margin: f64,
    /// How far a submenu tucks over its parent.
    pub submenu_overlap: f64,
}

impl Default for Metrics {
    fn default() -> Self {
        Self {
            row_height: 24.0,
            subtitle_row_height: 38.0,
            separator_height: 11.0,
            header_height: 24.0,
            padding: 6.0,
            inset: 6.0,
            text_inset: 9.0,
            state_width: 16.0,
            image_gap: 7.0,
            indentation: 12.0,
            key_gap: 28.0,
            chevron_width: 10.0,
            min_width: 160.0,
            corner_radius: 16.0,
            highlight_radius: 10.0,
            smoothing: 0.6,
            screen_margin: 6.0,
            submenu_overlap: 4.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    Item,
    Separator,
    /// A section header: a title that can't be chosen.
    Header,
}

/// What a row needs room for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowSpec {
    pub kind: RowKind,
    pub title_width: f64,
    pub subtitle_width: f64,
    pub has_subtitle: bool,
    pub key_width: f64,
    pub image_width: f64,
    pub has_state: bool,
    pub indent: usize,
    pub submenu: bool,
    /// An item that draws itself with a view of this size, across the menu's full width.
    pub custom: Option<Size>,
}

impl RowSpec {
    pub fn separator() -> Self {
        Self { kind: RowKind::Separator, ..Self::item(0.0) }
    }

    pub fn item(title_width: f64) -> Self {
        Self {
            kind: RowKind::Item,
            title_width,
            subtitle_width: 0.0,
            has_subtitle: false,
            key_width: 0.0,
            image_width: 0.0,
            has_state: false,
            indent: 0,
            submenu: false,
            custom: None,
        }
    }

    pub fn custom(size: Size) -> Self {
        Self { custom: Some(size), ..Self::item(0.0) }
    }
}

/// A row's place, measured down from the top of the content.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RowFrame {
    pub top: f64,
    pub height: f64,
}

/// Where each kind of thing in a row starts, from the body's left edge.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Columns {
    pub state_x: f64,
    pub image_x: f64,
    pub title_x: f64,
    /// Key equivalents end here.
    pub key_right: f64,
    pub chevron_x: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MenuLayout {
    pub size: Size,
    pub rows: Vec<RowFrame>,
    pub columns: Columns,
    /// All the rows and padding; more than the body's height when the menu has to scroll.
    pub content_height: f64,
    pub metrics: Metrics,
}

impl MenuLayout {
    pub fn scrolls(&self) -> bool {
        self.content_height > self.size.height + 0.5
    }

    pub fn max_scroll(&self) -> f64 {
        (self.content_height - self.size.height).max(0.0)
    }

    /// The row's rect in the body, y up from the body's bottom, with the content scrolled down
    /// by `scroll`.
    pub fn row_rect(&self, row: usize, scroll: f64) -> Rect {
        let f = self.rows[row];
        let y = self.size.height - (f.top - scroll) - f.height;
        Rect::new(0.0, y, self.size.width, f.height)
    }

    /// The row under `p`, a point in the body.
    pub fn row_at(&self, p: Point, scroll: f64) -> Option<usize> {
        if p.x < 0.0 || p.x >= self.size.width || p.y < 0.0 || p.y >= self.size.height {
            return None;
        }
        let from_top = self.size.height - p.y + scroll;
        self.rows.iter().position(|f| from_top >= f.top && from_top < f.top + f.height)
    }

    /// The scroll that brings `row` fully into view, starting from `scroll`.
    pub fn scroll_to_show(&self, row: usize, scroll: f64) -> f64 {
        let m = self.metrics.padding;
        let f = self.rows[row];
        let mut s = scroll;
        if f.top - m < s {
            s = f.top - m;
        }
        if f.top + f.height + m > s + self.size.height {
            s = f.top + f.height + m - self.size.height;
        }
        s.clamp(0.0, self.max_scroll())
    }
}

/// Lays out `rows`, at least `min_width` wide and at most `max_height` tall.
pub fn lay_out(rows: &[RowSpec], m: &Metrics, min_width: f64, max_width: f64, max_height: f64) -> MenuLayout {
    let any = |f: fn(&RowSpec) -> bool| rows.iter().any(|r| r.kind == RowKind::Item && f(r));
    let image_width = rows.iter().map(|r| r.image_width).fold(0.0, f64::max);
    let mut x = m.inset + m.text_inset;
    let state_x = x;
    if any(|r| r.has_state) {
        x += m.state_width;
    }
    let image_x = x;
    if image_width > 0.0 {
        x += image_width + m.image_gap;
    }
    let title_x = x;
    let text = rows
        .iter()
        .map(|r| {
            let indent = r.indent as f64 * m.indentation;
            let title = if r.kind == RowKind::Header { r.title_width } else { r.title_width.max(r.subtitle_width) };
            indent + title
        })
        .fold(0.0, f64::max);
    let key = rows.iter().map(|r| r.key_width).fold(0.0, f64::max);
    let mut trailing = 0.0f64;
    if key > 0.0 {
        trailing = trailing.max(m.key_gap + key);
    }
    if any(|r| r.submenu) {
        trailing = trailing.max(m.key_gap + m.chevron_width);
    }
    let custom = rows.iter().filter_map(|r| r.custom).map(|s| s.width).fold(0.0, f64::max);
    let width = math::ceil(title_x + text + trailing + m.text_inset + m.inset)
        .max(custom)
        .max(min_width)
        .max(m.min_width)
        .min(max_width);

    let mut top = m.padding;
    let frames = rows
        .iter()
        .map(|r| {
            let height = match r.kind {
                _ if r.custom.is_some() => r.custom.map_or(0.0, |s| s.height),
                RowKind::Separator => m.separator_height,
                RowKind::Header => m.header_height,
                RowKind::Item if r.has_subtitle => m.subtitle_row_height,
                RowKind::Item => m.row_height,
            };
            let frame = RowFrame { top, height };
            top += height;
            frame
        })
        .collect();
    let content_height = top + m.padding;
    let key_right = width - m.inset - m.text_inset;
    MenuLayout {
        size: Size::new(width, content_height.min(max_height)),
        rows: frames,
        columns: Columns { state_x, image_x, title_x, key_right, chevron_x: key_right - m.chevron_width },
        content_height,
        metrics: *m,
    }
}

/// Where a menu should open.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Anchor {
    /// Its top-left corner at the point, opening up and to the left instead when there's no
    /// room (context menus).
    Corner(Point),
    /// A row's top-left at the point (`popUpMenuPositioningItem:` with an item).
    Row { point: Point, row: usize },
    /// Over a pop-up button: the row on the button, its title over the button's.
    Button { bounds: Rect, row: usize },
    /// Beside a row of its parent (submenus), its first row level with that row.
    Beside { row: Rect, parent: Rect },
}

/// Which way a menu opened, which is the corner it unrolls from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Opening {
    pub up: bool,
    pub left: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub frame: Rect,
    pub opening: Opening,
}

/// Where a menu of `layout` goes, kept inside `screen` (a visible frame).
pub fn place(layout: &MenuLayout, anchor: Anchor, screen: Rect) -> Placement {
    let (w, h) = (layout.size.width, layout.size.height);
    let m = layout.metrics.screen_margin;
    let area = screen.inset(m, m);
    let mut opening = Opening::default();
    let (mut x, mut y) = match anchor {
        Anchor::Corner(p) => {
            let mut x = p.x;
            let mut y = p.y - h;
            if y < area.y && p.y + h <= area.max_y() {
                y = p.y;
                opening.up = true;
            }
            if x + w > area.max_x() && p.x - w >= area.x {
                x = p.x - w;
                opening.left = true;
            }
            (x, y)
        }
        Anchor::Row { point, row } => {
            let top = point.y + layout.rows[row].top;
            (point.x, top - h)
        }
        Anchor::Button { bounds, row } => {
            let f = layout.rows[row];
            // A pop-up button's title sits about 10pt in from its left edge.
            let x = bounds.x + 10.0 - layout.columns.title_x;
            let top = bounds.mid_y() + f.height / 2.0 + f.top;
            (x, top - h)
        }
        Anchor::Beside { row, parent } => {
            let top = row.max_y() + layout.metrics.padding;
            let mut x = parent.max_x() - layout.metrics.submenu_overlap;
            if x + w > area.max_x() && parent.x - w + layout.metrics.submenu_overlap >= area.x {
                x = parent.x - w + layout.metrics.submenu_overlap;
                opening.left = true;
            }
            (x, top - h)
        }
    };
    x = x.min(area.max_x() - w).max(area.x);
    y = y.min(area.max_y() - h).max(area.y);
    Placement { frame: Rect::new(math::round(x), math::round(y), w, h), opening }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect::new(0.0, 0.0, 1800.0, 1130.0);

    fn menu() -> MenuLayout {
        let rows = [
            RowSpec { key_width: 24.0, ..RowSpec::item(40.0) },
            RowSpec { has_state: true, ..RowSpec::item(80.0) },
            RowSpec::separator(),
            RowSpec { submenu: true, ..RowSpec::item(60.0) },
        ];
        lay_out(&rows, &Metrics::default(), 0.0, 900.0, 2000.0)
    }

    #[test]
    fn rows_stack_down_from_the_padding() {
        let l = menu();
        let m = Metrics::default();
        assert_eq!(l.rows[0], RowFrame { top: m.padding, height: m.row_height });
        assert_eq!(l.rows[2].height, m.separator_height);
        assert_eq!(l.size.height, m.padding * 2.0 + m.row_height * 3.0 + m.separator_height);
        assert!(!l.scrolls());
    }

    #[test]
    fn columns_make_room_for_state_and_keys() {
        let l = menu();
        let m = Metrics::default();
        assert_eq!(l.columns.state_x, m.inset + m.text_inset);
        assert_eq!(l.columns.title_x, m.inset + m.text_inset + m.state_width);
        assert!(l.size.width >= l.columns.title_x + 80.0 + m.key_gap + 24.0 + m.text_inset + m.inset);
    }

    #[test]
    fn hit_testing_finds_rows_from_the_top() {
        let l = menu();
        let m = Metrics::default();
        let top_row = Point::new(20.0, l.size.height - m.padding - 1.0);
        assert_eq!(l.row_at(top_row, 0.0), Some(0));
        assert_eq!(l.row_at(Point::new(20.0, l.size.height - 1.0), 0.0), None);
        assert_eq!(l.row_at(Point::new(-1.0, 20.0), 0.0), None);
        let r = l.row_rect(3, 0.0);
        assert_eq!(l.row_at(Point::new(20.0, r.mid_y()), 0.0), Some(3));
    }

    #[test]
    fn a_custom_view_sets_its_rows_height_and_can_widen_the_menu() {
        let rows = [RowSpec::item(40.0), RowSpec::custom(Size::new(300.0, 52.0))];
        let l = lay_out(&rows, &Metrics::default(), 0.0, 900.0, 2000.0);
        assert_eq!(l.rows[1].height, 52.0);
        assert_eq!(l.size.width, 300.0);
    }

    #[test]
    fn a_tall_menu_scrolls() {
        let rows = vec![RowSpec::item(40.0); 100];
        let l = lay_out(&rows, &Metrics::default(), 0.0, 900.0, 600.0);
        assert_eq!(l.size.height, 600.0);
        assert!(l.scrolls());
        let s = l.scroll_to_show(99, 0.0);
        assert_eq!(s, l.max_scroll());
        let r = l.row_rect(99, s);
        assert!(r.y >= 0.0 && r.max_y() <= 600.0);
    }

    #[test]
    fn a_context_menu_hangs_from_the_pointer() {
        let l = menu();
        let p = place(&l, Anchor::Corner(Point::new(300.0, 800.0)), SCREEN);
        assert_eq!((p.frame.x, p.frame.max_y()), (300.0, 800.0));
        assert_eq!(p.opening, Opening::default());
    }

    #[test]
    fn near_the_bottom_right_it_opens_up_and_left() {
        let l = menu();
        let p = place(&l, Anchor::Corner(Point::new(1790.0, 20.0)), SCREEN);
        assert_eq!((p.frame.max_x(), p.frame.y), (1790.0, 20.0));
        assert_eq!(p.opening, Opening { up: true, left: true });
    }

    #[test]
    fn a_positioned_item_lands_on_the_point() {
        let l = menu();
        let p = place(&l, Anchor::Row { point: Point::new(400.0, 600.0), row: 1 }, SCREEN);
        assert_eq!(p.frame.max_y() - l.rows[1].top, 600.0);
    }

    #[test]
    fn a_pop_up_button_menu_covers_the_button_with_its_row() {
        let l = menu();
        let bounds = Rect::new(200.0, 500.0, 150.0, 26.0);
        let p = place(&l, Anchor::Button { bounds, row: 1 }, SCREEN);
        let row_mid = p.frame.max_y() - l.rows[1].top - l.rows[1].height / 2.0;
        assert_eq!(row_mid, bounds.mid_y());
        assert_eq!(p.frame.x + l.columns.title_x, 210.0);
    }

    #[test]
    fn a_submenu_opens_beside_its_row_or_flips() {
        let l = menu();
        let parent = Rect::new(100.0, 400.0, 200.0, 300.0);
        let row = Rect::new(100.0, 600.0, 200.0, 24.0);
        let p = place(&l, Anchor::Beside { row, parent }, SCREEN);
        assert_eq!(p.frame.x, 300.0 - l.metrics.submenu_overlap);
        assert_eq!(p.frame.max_y() - l.metrics.padding, row.max_y());
        let parent = Rect::new(1650.0, 400.0, 140.0, 300.0);
        let p = place(&l, Anchor::Beside { row, parent }, SCREEN);
        assert!(p.opening.left && p.frame.max_x() <= 1650.0 + l.metrics.submenu_overlap);
    }

    #[test]
    fn menus_stay_on_screen() {
        let l = menu();
        let p = place(&l, Anchor::Row { point: Point::new(-50.0, 5000.0), row: 0 }, SCREEN);
        let m = l.metrics.screen_margin;
        assert!(p.frame.x >= m && p.frame.max_y() <= SCREEN.max_y() - m);
    }
}
