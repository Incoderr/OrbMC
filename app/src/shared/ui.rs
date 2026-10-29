//! Small visual building blocks shared by several features.

use gpui_kit::*;

use super::locale::Locale;
use super::theme::Palette;

pub(crate) fn field_label(label: &'static str) -> AnyElement {
    div()
        .text_size(px(13.))
        .font_weight(FontWeight::BOLD)
        .child(label)
        .into_any_element()
}

pub(crate) fn empty_page(palette: Palette, locale: Locale) -> AnyElement {
    div()
        .text_size(px(13.))
        .text_color(rgb(palette.muted))
        .child(locale.text("Здесь пока пусто", "Nothing here yet"))
        .into_any_element()
}

/// Text the user can select and copy (names, messages, logs). It takes the
/// colour and size of its parent.
pub(crate) fn selectable(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
) -> gpui_kit::base::SelectableText {
    gpui_kit::base::SelectableText::new(id, text)
}
