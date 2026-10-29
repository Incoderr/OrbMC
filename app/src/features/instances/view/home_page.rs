use super::{InstancesView, InstancesViewEvent};
use crate::features::settings::*;
use crate::shared::theme::Palette;
use gpui_kit::base::{Button as UiButton, StyledExt};
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

impl InstancesView {
    pub(crate) fn home_page(
        &self,
        palette: Palette,
        locale: Locale,
        cx: &Context<Self>,
    ) -> AnyElement {
        div()
            .v_flex()
            .w_full()
            .flex_1()
            .min_h_0()
            .pr_4()
            .overflow_y_scrollbar()
            .gap_4()
            .when(self.instances.read(cx).profiles().is_empty(), |this| {
                this.child(
                    div().v_flex().items_start().gap_3().p_6()
                        .rounded(px(12.)).bg(rgb(palette.surface))
                        .border_1().border_color(rgb(palette.border))
                        .child(div().text_size(px(20.)).font_weight(FontWeight::BOLD)
                            .child(locale.text("Создай первую сборку", "Create your first build")))
                        .child(div().text_size(px(13.)).text_color(rgb(palette.muted))
                            .child(locale.text("Выбери версию Minecraft. Здесь появятся твои сборки и кнопки запуска.", "Choose a Minecraft version. Your builds and launch controls will appear here.")))
                        .child(UiButton::new("home-create-build").cursor_pointer()
                            .px_4().py_3().rounded(px(8.))
                            .bg(rgb(palette.accent)).text_color(rgb(palette.accent_foreground))
                            .hover(|style| style.opacity(0.9))
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(InstancesViewEvent::CreateBuild)))
                            .child(locale.text("Создать сборку", "Create build"))),
                )
            })
            .when(!self.instances.read(cx).profiles().is_empty(), |this| {
                this.child(self.instances_section(palette, locale, cx))
            })
            .into_any_element()
    }
}
