//! Moving the highlight with the keyboard and by typing.

use alloc::string::String;

/// The next row from `from` in `direction` (+1 down, -1 up) that `selectable` allows, or `from`
/// if there's none. With no row highlighted, down starts at the top and up at the bottom.
pub fn step(count: usize, from: Option<usize>, direction: isize, selectable: impl Fn(usize) -> bool) -> Option<usize> {
    let mut i: isize = match from {
        Some(i) => i as isize + direction,
        None if direction > 0 => 0,
        None => count as isize - 1,
    };
    while i >= 0 && (i as usize) < count {
        if selectable(i as usize) {
            return Some(i as usize);
        }
        i += direction;
    }
    from
}

/// The first selectable row at or past `direction`'s end: the first (+1) or last (-1).
pub fn end(count: usize, direction: isize, selectable: impl Fn(usize) -> bool) -> Option<usize> {
    if direction > 0 { step(count, None, 1, selectable) } else { step(count, None, -1, selectable) }
}

/// The first selectable row whose title starts with `typed`, ignoring case.
pub fn type_select<'a>(
    titles: impl Iterator<Item = &'a str>,
    typed: &str,
    selectable: impl Fn(usize) -> bool,
) -> Option<usize> {
    if typed.is_empty() {
        return None;
    }
    let typed: String = typed.chars().flat_map(char::to_lowercase).collect();
    titles.enumerate().find_map(|(i, title)| {
        let lower: String = title.chars().flat_map(char::to_lowercase).take(typed.chars().count()).collect();
        (selectable(i) && lower == typed).then_some(i)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stepping_skips_rows_that_cant_be_chosen() {
        let ok = |i: usize| i != 1 && i != 2;
        assert_eq!(step(5, Some(0), 1, ok), Some(3));
        assert_eq!(step(5, Some(3), -1, ok), Some(0));
        assert_eq!(step(5, Some(4), 1, ok), Some(4));
        assert_eq!(step(5, None, 1, ok), Some(0));
        assert_eq!(step(5, None, -1, ok), Some(4));
        assert_eq!(end(5, -1, |i| i < 3), Some(2));
    }

    #[test]
    fn typing_finds_a_title() {
        let titles = ["Copy", "Cut", "Paste", "Paste and Match Style"];
        assert_eq!(type_select(titles.iter().copied(), "pa", |_| true), Some(2));
        assert_eq!(type_select(titles.iter().copied(), "CU", |_| true), Some(1));
        assert_eq!(type_select(titles.iter().copied(), "paste", |i| i != 2), Some(3));
        assert_eq!(type_select(titles.iter().copied(), "x", |_| true), None);
    }
}
