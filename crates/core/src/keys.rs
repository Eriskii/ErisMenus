//! Writing a menu item's key equivalent the way menus show it: ⌃⌥⇧⌘ then the key.

use alloc::string::String;

/// `NSEventModifierFlags`, the parts menus show.
pub const SHIFT: u64 = 1 << 17;
pub const CONTROL: u64 = 1 << 18;
pub const OPTION: u64 = 1 << 19;
pub const COMMAND: u64 = 1 << 20;
pub const FUNCTION: u64 = 1 << 23;
pub const DEVICE_INDEPENDENT: u64 = 0xffff_0000;

/// The key equivalent `key` (an `NSMenuItem.keyEquivalent`, as UTF-16) with `mask`; empty when
/// there's no key. An uppercase letter implies Shift, as AppKit has it.
pub fn glyphs(key: &[u16], mask: u64) -> String {
    let Some(&first) = key.first() else { return String::new() };
    let mut mask = mask;
    let mut name = String::new();
    match special(first) {
        Some(special) => name.push_str(special),
        None => {
            let text = String::from_utf16_lossy(key);
            let mut chars = text.chars();
            let c = chars.next().unwrap_or(' ');
            if c.is_uppercase() && chars.next().is_none() {
                mask |= SHIFT;
            }
            for c in text.chars() {
                name.extend(c.to_uppercase());
            }
        }
    }
    let mut out = String::new();
    if mask & FUNCTION != 0 && !matches!(first, 0xF704..=0xF726) {
        out.push_str("fn ");
    }
    for (bit, glyph) in [(CONTROL, '⌃'), (OPTION, '⌥'), (SHIFT, '⇧'), (COMMAND, '⌘')] {
        if mask & bit != 0 {
            out.push(glyph);
        }
    }
    out.push_str(&name);
    out
}

fn special(c: u16) -> Option<&'static str> {
    const F_KEYS: [&str; 35] = [
        "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12", "F13", "F14", "F15", "F16", "F17",
        "F18", "F19", "F20", "F21", "F22", "F23", "F24", "F25", "F26", "F27", "F28", "F29", "F30", "F31", "F32", "F33",
        "F34", "F35",
    ];
    Some(match c {
        0xF700 => "↑",
        0xF701 => "↓",
        0xF702 => "←",
        0xF703 => "→",
        0xF704..=0xF726 => F_KEYS[(c - 0xF704) as usize],
        0xF728 => "⌦",
        0x08 | 0x7F => "⌫",
        0xF729 => "↖",
        0xF72B => "↘",
        0xF72C => "⇞",
        0xF72D => "⇟",
        0x0D => "↩",
        0x03 => "⌤",
        0x1B => "⎋",
        0x09 => "⇥",
        0x19 => "⇤",
        0x20 => "Space",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(key: &str, mask: u64) -> String {
        glyphs(&key.encode_utf16().collect::<Vec<_>>(), mask)
    }

    #[test]
    fn modifiers_come_in_menu_order() {
        assert_eq!(g("c", COMMAND), "⌘C");
        assert_eq!(g("v", COMMAND | OPTION | SHIFT), "⌥⇧⌘V");
        assert_eq!(g("q", COMMAND | CONTROL), "⌃⌘Q");
    }

    #[test]
    fn an_uppercase_key_implies_shift() {
        assert_eq!(g("Z", COMMAND), "⇧⌘Z");
    }

    #[test]
    fn special_keys_have_glyphs() {
        assert_eq!(g("\u{8}", COMMAND), "⌘⌫");
        assert_eq!(g("\u{7f}", COMMAND), "⌘⌫");
        assert_eq!(g("\u{f728}", 0), "⌦");
        assert_eq!(g("\u{f700}", CONTROL), "⌃↑");
        assert_eq!(g("\u{f705}", 0), "F2");
        assert_eq!(g("\r", COMMAND), "⌘↩");
        assert_eq!(g(" ", CONTROL), "⌃Space");
    }

    #[test]
    fn no_key_shows_nothing() {
        assert_eq!(g("", COMMAND), "");
    }
}
