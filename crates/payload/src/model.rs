//! A menu's items as rows: hidden items left out, separators tidied, and each alternate folded into
//! the row of the item it stands in for.

use alloc::vec::Vec;

use menus_core::keys::{COMMAND, CONTROL, OPTION, SHIFT};
use menus_core::layout::RowKind;

use crate::msg;
use crate::objc::{Id, release, responds, retain, utf16};

const MODIFIERS: u64 = SHIFT | CONTROL | OPTION | COMMAND;

pub struct Row {
    pub kind: RowKind,
    /// The item and its alternates, each with the modifiers that show it; retained.
    variants: Vec<(Id, u64)>,
    pub current: usize,
}

impl Row {
    pub fn item(&self) -> Id {
        self.variants[self.current].0
    }

    pub fn items(&self) -> impl Iterator<Item = Id> + '_ {
        self.variants.iter().map(|v| v.0)
    }

    pub fn enabled(&self) -> bool {
        self.kind == RowKind::Item && msg!(bool; self.item(), c"isEnabled")
    }

    pub fn submenu(&self) -> Id {
        if self.kind == RowKind::Item { msg!(Id; self.item(), c"submenu") } else { core::ptr::null_mut() }
    }

    /// Shows the variant `modifiers` call for; true if that changed which one.
    pub fn show_variant_for(&mut self, modifiers: u64) -> bool {
        let held = modifiers & MODIFIERS;
        let primary = self.variants[0].1;
        let pick = self
            .variants
            .iter()
            .skip(1)
            .position(|&(_, mask)| held != 0 && (held == mask || held == mask & !primary))
            .map_or(0, |i| i + 1);
        let changed = pick != self.current;
        self.current = pick;
        changed
    }
}

impl Drop for Row {
    fn drop(&mut self) {
        for &(item, _) in &self.variants {
            release(item);
        }
    }
}

/// Whether any item that would show draws itself with a view, which we leave to AppKit.
pub fn has_custom_views(menu: Id) -> bool {
    let count = msg!(isize; menu, c"numberOfItems");
    (0..count).any(|i| {
        let item = msg!(Id; menu, c"itemAtIndex:", i => isize);
        !msg!(bool; item, c"isHidden") && !msg!(Id; item, c"view").is_null()
    })
}

/// The rows of `menu`, leaving out its first item if `skip_first` (a pull-down's title).
pub fn rows(menu: Id, skip_first: bool, modifiers: u64) -> Vec<Row> {
    let count = msg!(isize; menu, c"numberOfItems");
    let mut rows: Vec<Row> = Vec::new();
    for i in (skip_first as isize)..count {
        let item = msg!(Id; menu, c"itemAtIndex:", i => isize);
        if msg!(bool; item, c"isHidden") {
            continue;
        }
        let kind = if msg!(bool; item, c"isSeparatorItem") {
            RowKind::Separator
        } else if responds(item, c"isSectionHeader") && msg!(bool; item, c"isSectionHeader") {
            RowKind::Header
        } else {
            RowKind::Item
        };
        let mask = msg!(u64; item, c"keyEquivalentModifierMask") & MODIFIERS;
        if kind == RowKind::Item && msg!(bool; item, c"isAlternate") {
            if let Some(last) = rows.last_mut().filter(|r| r.kind == RowKind::Item && same_key(r.variants[0].0, item)) {
                last.variants.push((retain(item), mask));
                continue;
            }
        }
        rows.push(Row { kind, variants: alloc::vec![(retain(item), mask)], current: 0 });
    }
    tidy_separators(&mut rows);
    for row in &mut rows {
        row.show_variant_for(modifiers);
    }
    rows
}

fn same_key(a: Id, b: Id) -> bool {
    let key = |item: Id| utf16(msg!(Id; item, c"keyEquivalent")).iter().map(|&c| lower(c)).collect::<Vec<_>>();
    key(a) == key(b)
}

fn lower(c: u16) -> u16 {
    if (b'A' as u16..=b'Z' as u16).contains(&c) { c + 32 } else { c }
}

/// No separator first, last, or right after another.
fn tidy_separators(rows: &mut Vec<Row>) {
    let mut previous_was_separator = true;
    rows.retain(|r| {
        let separator = r.kind == RowKind::Separator;
        let keep = !(separator && previous_was_separator);
        if keep {
            previous_was_separator = separator;
        }
        keep
    });
    while rows.last().is_some_and(|r| r.kind == RowKind::Separator) {
        rows.pop();
    }
}
