use super::CatalogView;
use gpui_kit::assets::IconName;
use gpui_kit::base::{Button as UiButton, StyledExt};
use gpui_kit::component::WindowExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

impl CatalogView {
    /// Opens the fullscreen viewer for gallery screenshot `index`.
    ///
    /// Navigation reopens the dialog with the new index; the shared
    /// gallery index stays consistent with the carousel on the page.
    pub(crate) fn open_gallery_dialog(
        &self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(info) = self.mods.project_info() else {
            return;
        };
        let gallery = info.gallery.clone();
        let Some(current) = info.gallery.get(index) else {
            return;
        };
        let gallery_len = gallery.len();
        let image_url = current
            .full_url
            .clone()
            .unwrap_or_else(|| current.url.clone());
        let caption = if current.title.trim().is_empty() {
            current.description.trim().to_owned()
        } else {
            current.title.trim().to_owned()
        };
        let project_name = self
            .mods
            .selected_mod()
            .map(|project| project.name.clone())
            .unwrap_or_default();
        let palette = self.palette;
        let locale = self.locale;
        let owner = cx.entity().downgrade();

        let title = format!("{project_name} · {}/{gallery_len}", index + 1);
        window.open_dialog(cx, move |dialog, _, _| {
            let owner = owner.clone();
            dialog
                .w(px(880.))
                .title(div().text_size(px(12.)).child(title.clone()))
                .content({
                    let image_url = image_url.clone();
                    let caption = caption.clone();
                    move |content, _, _| {
                        content.child(
                            div()
                                .v_flex()
                                .w_full()
                                .gap_2()
                                .child(
                                    div()
                                        .w_full()
                                        .h(px(480.))
                                        .min_w_0()
                                        .rounded(px(8.))
                                        .overflow_hidden()
                                        .bg(rgb(0x101012))
                                        .child(
                                            img(image_url.clone())
                                                .size_full()
                                                .object_fit(ObjectFit::Contain)
                                                .with_loading(gallery_dialog_placeholder)
                                                .with_fallback(gallery_dialog_placeholder),
                                        ),
                                )
                                .when(!caption.is_empty(), |this| {
                                    this.child(
                                        div()
                                            .w_full()
                                            .min_w_0()
                                            .text_size(px(12.))
                                            .text_color(rgb(palette.muted))
                                            .child(caption.clone()),
                                    )
                                }),
                        )
                    }
                })
                .footer(
                    div()
                        .h_flex()
                        .w_full()
                        .items_center()
                        .gap_2()
                        .child(
                            UiButton::new("gallery-dialog-prev")
                                .cursor_pointer()
                                .px_3()
                                .py_2()
                                .rounded(px(7.))
                                .bg(rgb(palette.control))
                                .text_color(rgb(palette.foreground))
                                .hover(|style| style.bg(rgb(palette.hover)))
                                .on_click({
                                    let new_index = (index + gallery_len - 1) % gallery_len;
                                    let owner = owner.clone();
                                    move |_, window, cx| {
                                        window.close_dialog(cx);
                                        if let Some(owner) = owner.upgrade() {
                                            owner.update(cx, |this, cx| {
                                                this.mods.set_gallery_index(new_index, gallery_len);
                                                cx.notify();
                                                this.open_gallery_dialog(new_index, window, cx);
                                            });
                                        }
                                    }
                                })
                                .child(locale.text("◀ Назад", "◀ Previous")),
                        )
                        .child(
                            div()
                                .text_size(px(11.))
                                .text_color(rgb(palette.muted))
                                .child(format!("{}/{gallery_len}", index + 1)),
                        )
                        .child(
                            UiButton::new("gallery-dialog-next")
                                .cursor_pointer()
                                .px_3()
                                .py_2()
                                .rounded(px(7.))
                                .bg(rgb(palette.control))
                                .text_color(rgb(palette.foreground))
                                .hover(|style| style.bg(rgb(palette.hover)))
                                .on_click({
                                    let new_index = (index + 1) % gallery_len;
                                    let owner = owner.clone();
                                    move |_, window, cx| {
                                        window.close_dialog(cx);
                                        if let Some(owner) = owner.upgrade() {
                                            owner.update(cx, |this, cx| {
                                                this.mods.set_gallery_index(new_index, gallery_len);
                                                cx.notify();
                                                this.open_gallery_dialog(new_index, window, cx);
                                            });
                                        }
                                    }
                                })
                                .child(locale.text("Вперёд ▶", "Next ▶")),
                        ),
                )
        });
    }
}

/// Fills the dialog image slot while a screenshot loads.
fn gallery_dialog_placeholder() -> AnyElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(11.))
        .text_color(rgb(0x8a8a8f))
        .child(IconName::GalleryVerticalEnd)
        .into_any_element()
}
