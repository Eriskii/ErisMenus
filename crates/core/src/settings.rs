//! What can be set: the accent color every app uses, whether menus and glass are ours, and how
//! menus move. Small enough to travel as one `u64`, the state of the Darwin notification the
//! payload in every app watches.

use crate::spring::Style;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// The system accent color, as 0xRRGGBB, in place of the one picked in System Settings.
    pub accent: Option<u32>,
    /// Menus drawn by ErisMenus (otherwise AppKit's own).
    pub menus: bool,
    /// Liquid Glass painted over.
    pub glass: bool,
    pub style: Style,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { accent: None, menus: true, glass: true, style: Style::Squishy }
    }
}

/// Set in every packed value, so a notification nobody has published yet (state 0) isn't read
/// as settings.
const VALID: u64 = 1 << 63;
const ACCENT_ON: u64 = 1 << 24;
const MENUS_OFF: u64 = 1 << 25;
const GLASS_OFF: u64 = 1 << 26;
const STYLE_SHIFT: u32 = 27;

impl Settings {
    pub fn pack(&self) -> u64 {
        let mut packed = VALID;
        if let Some(rgb) = self.accent {
            packed |= ACCENT_ON | u64::from(rgb & 0xFF_FFFF);
        }
        if !self.menus {
            packed |= MENUS_OFF;
        }
        if !self.glass {
            packed |= GLASS_OFF;
        }
        packed | (style_index(self.style) << STYLE_SHIFT)
    }

    pub fn unpack(packed: u64) -> Option<Settings> {
        if packed & VALID == 0 {
            return None;
        }
        Some(Settings {
            accent: (packed & ACCENT_ON != 0).then_some((packed & 0xFF_FFFF) as u32),
            menus: packed & MENUS_OFF == 0,
            glass: packed & GLASS_OFF == 0,
            style: STYLES.get(((packed >> STYLE_SHIFT) & 3) as usize).copied().unwrap_or_default(),
        })
    }
}

pub const STYLES: [Style; 3] = [Style::Squishy, Style::Expressive, Style::Standard];

fn style_index(style: Style) -> u64 {
    STYLES.iter().position(|s| *s == style).unwrap_or(0) as u64
}

impl Style {
    pub fn name(self) -> &'static str {
        match self {
            Style::Squishy => "squishy",
            Style::Expressive => "expressive",
            Style::Standard => "standard",
        }
    }

    pub fn from_name(name: &str) -> Option<Style> {
        STYLES.iter().copied().find(|style| style.name().eq_ignore_ascii_case(name))
    }
}

/// `#FF6B00`, `FF6B00`, `#f60` or `f60` as 0xRRGGBB.
pub fn parse_hex(text: &str) -> Option<u32> {
    let hex = text.trim();
    let hex = hex.strip_prefix('#').unwrap_or(hex);
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    match hex.len() {
        6 => u32::from_str_radix(hex, 16).ok(),
        3 => {
            let short = u32::from_str_radix(hex, 16).ok()?;
            let (r, g, b) = ((short >> 8) & 0xF, (short >> 4) & 0xF, short & 0xF);
            Some((r * 17) << 16 | (g * 17) << 8 | b * 17)
        }
        _ => None,
    }
}

/// The components of 0xRRGGBB, each 0 to 1.
pub fn components(rgb: u32) -> (f64, f64, f64) {
    let c = |shift: u32| f64::from((rgb >> shift) & 0xFF) / 255.0;
    (c(16), c(8), c(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let all = [
            Settings::default(),
            Settings { accent: Some(0xFF6B00), ..Settings::default() },
            Settings { accent: Some(0), menus: false, glass: false, style: Style::Standard },
            Settings { accent: Some(0xFFFFFF), menus: true, glass: false, style: Style::Expressive },
        ];
        for settings in all {
            assert_eq!(Settings::unpack(settings.pack()), Some(settings));
        }
    }

    #[test]
    fn nothing_published_is_no_settings() {
        assert_eq!(Settings::unpack(0), None);
        assert_eq!(Settings::unpack(0xFF6B00), None);
    }

    #[test]
    fn reads_hex() {
        assert_eq!(parse_hex("#FF6B00"), Some(0xFF6B00));
        assert_eq!(parse_hex(" ff6b00 "), Some(0xFF6B00));
        assert_eq!(parse_hex("#f60"), Some(0xFF6600));
        assert_eq!(parse_hex("#FF6B0"), None);
        assert_eq!(parse_hex("orange"), None);
        assert_eq!(parse_hex("+F6B00"), None);
    }

    #[test]
    fn names_styles() {
        for style in STYLES {
            assert_eq!(Style::from_name(style.name()), Some(style));
        }
        assert_eq!(Style::from_name("Squishy"), Some(Style::Squishy));
        assert_eq!(Style::from_name("bouncy"), None);
    }
}
