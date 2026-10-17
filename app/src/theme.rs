use crate::features::settings::{AccentColor, ThemeChoice};

#[derive(Clone, Copy)]
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

#[derive(Debug, Default)]
pub(crate) struct SidebarState {
    collapsed: bool,
}

impl SidebarState {
    pub(crate) fn is_collapsed(&self) -> bool {
        self.collapsed
    }

    pub(crate) fn toggle(&mut self) {
        self.collapsed = !self.collapsed;
    }
}
