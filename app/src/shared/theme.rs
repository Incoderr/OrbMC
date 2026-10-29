//! Colours of the launcher UI: the user-selectable theme and accent, and
//! the palette derived from them.

use super::locale::Locale;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AccentColor {
    #[default]
    Orange,
    Red,
    Blue,
    Green,
    Purple,
    Pink,
    /// Any colour picked by the user, as `0xRRGGBB`.
    Custom(u32),
}

impl AccentColor {
    pub const ALL: [Self; 6] = [
        Self::Orange,
        Self::Red,
        Self::Blue,
        Self::Green,
        Self::Purple,
        Self::Pink,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::Orange => "accent-orange",
            Self::Red => "accent-red",
            Self::Blue => "accent-blue",
            Self::Green => "accent-green",
            Self::Purple => "accent-purple",
            Self::Pink => "accent-pink",
            Self::Custom(_) => "accent-custom",
        }
    }

    pub fn name(self, locale: Locale) -> &'static str {
        match self {
            Self::Orange => locale.text("Оранжевый", "Orange"),
            Self::Red => locale.text("Красный", "Red"),
            Self::Blue => locale.text("Синий", "Blue"),
            Self::Green => locale.text("Зелёный", "Green"),
            Self::Purple => locale.text("Фиолетовый", "Purple"),
            Self::Pink => locale.text("Розовый", "Pink"),
            Self::Custom(_) => locale.text("Свой цвет", "Custom color"),
        }
    }

    pub fn color(self, theme: ThemeChoice) -> u32 {
        match (self, theme) {
            (Self::Orange, ThemeChoice::Dark) => 0xff7a1a,
            (Self::Orange, ThemeChoice::Light) => 0xe96a12,
            (Self::Red, ThemeChoice::Dark) => 0xff515b,
            (Self::Red, ThemeChoice::Light) => 0xdc3545,
            (Self::Blue, ThemeChoice::Dark) => 0x448aff,
            (Self::Blue, ThemeChoice::Light) => 0x2563eb,
            (Self::Green, ThemeChoice::Dark) => 0x39c975,
            (Self::Green, ThemeChoice::Light) => 0x168a4a,
            (Self::Purple, ThemeChoice::Dark) => 0xa78bfa,
            (Self::Purple, ThemeChoice::Light) => 0x7c3aed,
            (Self::Pink, ThemeChoice::Dark) => 0xf472b6,
            (Self::Pink, ThemeChoice::Light) => 0xdb2777,
            (Self::Custom(color), _) => color,
        }
    }

    pub fn foreground(self, theme: ThemeChoice) -> u32 {
        match (self, theme) {
            (Self::Custom(color), _) => readable_text_on(color),
            (Self::Orange, _) | (_, ThemeChoice::Dark) => 0x17120e,
            (_, ThemeChoice::Light) => 0xffffff,
        }
    }
}

/// Dark or white text, whichever reads better on `background` (`0xRRGGBB`),
/// by the WCAG relative luminance.
pub fn readable_text_on(background: u32) -> u32 {
    let channel = |shift: u32| {
        let value = f64::from((background >> shift) & 0xff) / 255.;
        if value <= 0.03928 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance = 0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0);
    if luminance > 0.4 { 0x17120e } else { 0xffffff }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeChoice {
    #[default]
    Dark,
    Light,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Palette {
    pub(crate) background: u32,
    pub(crate) sidebar: u32,
    pub(crate) surface: u32,
    pub(crate) border: u32,
    pub(crate) selected: u32,
    pub(crate) control: u32,
    pub(crate) hover: u32,
    pub(crate) selected_hover: u32,
    pub(crate) pressed: u32,
    pub(crate) foreground: u32,
    pub(crate) muted: u32,
    pub(crate) accent: u32,
    pub(crate) accent_foreground: u32,
}

impl Palette {
    pub(crate) fn for_settings(theme: ThemeChoice, accent: AccentColor) -> Self {
        let mut palette = match theme {
            ThemeChoice::Dark => Self {
                background: 0x111111,
                sidebar: 0x0b0b0b,
                surface: 0x181818,
                border: 0x2b2b2b,
                selected: 0x252525,
                control: 0x141414,
                hover: 0x262626,
                selected_hover: 0x343434,
                pressed: 0x3b3b3b,
                foreground: 0xf2f2f2,
                muted: 0xa0a0a0,
                accent: 0,
                accent_foreground: 0,
            },
            ThemeChoice::Light => Self {
                background: 0xf3f3f1,
                sidebar: 0xffffff,
                surface: 0xffffff,
                border: 0xe2e2df,
                selected: 0xe9e9e7,
                control: 0xf8f8f6,
                hover: 0xefefec,
                selected_hover: 0xdfdfdc,
                pressed: 0xe4e4e1,
                foreground: 0x202020,
                muted: 0x747474,
                accent: 0,
                accent_foreground: 0,
            },
        };
        palette.accent = accent.color(theme);
        palette.accent_foreground = accent.foreground(theme);
        palette
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_accent_picks_readable_text() {
        assert_eq!(readable_text_on(0xffffff), 0x17120e);
        assert_eq!(readable_text_on(0xffd400), 0x17120e);
        assert_eq!(readable_text_on(0x1e3a8a), 0xffffff);
        let accent = AccentColor::Custom(0x123456);
        assert_eq!(accent.color(ThemeChoice::Light), 0x123456);
        assert_eq!(accent.foreground(ThemeChoice::Dark), 0xffffff);
    }
}
