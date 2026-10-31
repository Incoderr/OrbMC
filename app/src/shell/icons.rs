//! Remix Icon (https://remixicon.com, Apache-2.0) in place of the bundled
//! Lucide icons. Each Remix file is served under the path of the Lucide icon
//! it replaces, so `IconName::…` in OrbMC and inside gpui-component draw the
//! Remix version; icons missing here stay Lucide. Files: `assets/icons/remix`.

use std::borrow::Cow;

use gpui_kit::{AssetSource, SharedString};

macro_rules! remix {
    ($name:literal) => {
        include_bytes!(concat!("../../../assets/icons/remix/", $name, ".svg")).as_slice()
    };
}

/// Lucide asset path → Remix icon drawn for it.
const ICONS: &[(&str, &[u8])] = &[
    ("icons/arrow-down.svg", remix!("arrow-down-line")),
    ("icons/arrow-left.svg", remix!("arrow-left-line")),
    ("icons/asterisk.svg", remix!("asterisk")),
    ("icons/ban.svg", remix!("forbid-line")),
    ("icons/blocks.svg", remix!("apps-2-line")),
    ("icons/book-open.svg", remix!("book-open-line")),
    ("icons/bot.svg", remix!("robot-2-line")),
    ("icons/box.svg", remix!("box-3-line")),
    ("icons/calendar.svg", remix!("calendar-line")),
    ("icons/check.svg", remix!("check-line")),
    ("icons/chevron-down.svg", remix!("arrow-down-s-line")),
    ("icons/chevron-left.svg", remix!("arrow-left-s-line")),
    ("icons/chevron-right.svg", remix!("arrow-right-s-line")),
    ("icons/chevron-up.svg", remix!("arrow-up-s-line")),
    ("icons/chevrons-up-down.svg", remix!("expand-up-down-line")),
    ("icons/circle-check.svg", remix!("checkbox-circle-line")),
    ("icons/circle-user.svg", remix!("account-circle-line")),
    ("icons/circle-x.svg", remix!("close-circle-line")),
    ("icons/clock.svg", remix!("time-line")),
    ("icons/close.svg", remix!("close-line")),
    ("icons/copy.svg", remix!("file-copy-line")),
    ("icons/cpu.svg", remix!("cpu-line")),
    ("icons/delete.svg", remix!("delete-back-2-line")),
    ("icons/download.svg", remix!("download-2-line")),
    ("icons/ellipsis.svg", remix!("more-line")),
    ("icons/expand.svg", remix!("fullscreen-line")),
    ("icons/external-link.svg", remix!("external-link-line")),
    ("icons/eye.svg", remix!("eye-line")),
    ("icons/eye-off.svg", remix!("eye-off-line")),
    ("icons/file-text.svg", remix!("file-text-line")),
    ("icons/folder-open.svg", remix!("folder-open-line")),
    ("icons/gallery-vertical-end.svg", remix!("stack-line")),
    ("icons/github.svg", remix!("github-line")),
    ("icons/hard-drive.svg", remix!("hard-drive-2-line")),
    ("icons/heart.svg", remix!("heart-line")),
    ("icons/house.svg", remix!("home-5-line")),
    ("icons/inbox.svg", remix!("inbox-line")),
    ("icons/info.svg", remix!("information-line")),
    ("icons/languages.svg", remix!("translate-2")),
    ("icons/loader.svg", remix!("loader-4-line")),
    ("icons/loader-circle.svg", remix!("loader-2-line")),
    ("icons/map.svg", remix!("map-2-line")),
    ("icons/maximize.svg", remix!("fullscreen-line")),
    ("icons/minimize.svg", remix!("fullscreen-exit-line")),
    ("icons/minus.svg", remix!("subtract-line")),
    ("icons/moon.svg", remix!("moon-line")),
    ("icons/package.svg", remix!("archive-2-line")),
    ("icons/paintbrush.svg", remix!("brush-line")),
    ("icons/palette.svg", remix!("palette-line")),
    ("icons/panel-left.svg", remix!("side-bar-line")),
    ("icons/panel-left-close.svg", remix!("sidebar-fold-line")),
    ("icons/panel-left-open.svg", remix!("sidebar-unfold-line")),
    ("icons/panel-top-close.svg", remix!("arrow-up-double-line")),
    ("icons/pause.svg", remix!("pause-line")),
    ("icons/play.svg", remix!("play-line")),
    ("icons/plus.svg", remix!("add-line")),
    ("icons/puzzle.svg", remix!("puzzle-line")),
    ("icons/refresh-cw.svg", remix!("refresh-line")),
    ("icons/replace.svg", remix!("swap-line")),
    ("icons/search.svg", remix!("search-line")),
    ("icons/settings.svg", remix!("settings-3-line")),
    ("icons/settings-2.svg", remix!("equalizer-line")),
    ("icons/sort-ascending.svg", remix!("sort-asc")),
    ("icons/sort-descending.svg", remix!("sort-desc")),
    ("icons/star.svg", remix!("star-line")),
    ("icons/star-fill.svg", remix!("star-fill")),
    ("icons/sun.svg", remix!("sun-line")),
    ("icons/trash.svg", remix!("delete-bin-line")),
    ("icons/triangle-alert.svg", remix!("alert-line")),
    ("icons/undo-2.svg", remix!("arrow-go-back-line")),
    ("icons/user.svg", remix!("user-line")),
];

pub struct RemixIcons;

impl AssetSource for RemixIcons {
    fn load(&self, path: &str) -> gpui_kit::Result<Option<Cow<'static, [u8]>>> {
        Ok(ICONS
            .iter()
            .find(|(icon, _)| *icon == path)
            .map(|(_, svg)| Cow::Borrowed(*svg)))
    }

    fn list(&self, path: &str) -> gpui_kit::Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .map(|(icon, _)| *icon)
            .filter(|icon| icon.starts_with(path))
            .map(Into::into)
            .collect())
    }
}
