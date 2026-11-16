//! "Add a mod from GitHub": a repository link whose release jar goes into
//! the build.

use super::InstancesView;
use crate::features::instances::InstanceId;
use crate::shared::motion::DialogMotion as _;
use gpui_kit::base::StyledExt;
use gpui_kit::component::WindowExt;
use gpui_kit::component::button::{Button as ComponentButton, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::*;

impl InstancesView {
    pub(crate) fn open_github_mod_dialog(
        &mut self,
        id: InstanceId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let locale = self.locale;
        let palette = self.palette;
        let input =
            cx.new(|cx| InputState::new(window, cx).placeholder("https://github.com/owner/repo"));
        let owner = cx.entity().downgrade();
        window.open_dialog(cx, move |dialog, _, _| {
            let input = input.clone();
            let owner = owner.clone();
            dialog
                .w(px(520.))
                .title(locale.text("Мод с GitHub", "Mod from GitHub"))
                .animated_content(move |content, _, _| {
                    let submit_input = input.clone();
                    let owner = owner.clone();
                    content.child(
                        div()
                            .v_flex()
                            .gap_3()
                            .p_4()
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .text_color(rgb(palette.muted))
                                    .child(locale.text(
                                        "Ссылка на репозиторий. Будет установлен jar из последнего \
                                         релиза, подходящий под версию игры и лоадер сборки; \
                                         обновления проверяются так же, как у модов из каталога.",
                                        "Repository link. The jar of the newest release that fits \
                                         the build's game version and loader is installed; updates \
                                         are checked like for catalog mods.",
                                    )),
                            )
                            .child(Input::new(&input))
                            .child(
                                div().h_flex().justify_end().child(
                                    ComponentButton::new("github-mod-add")
                                        .cursor_pointer()
                                        .primary()
                                        .label(locale.text("Добавить", "Add"))
                                        .on_click(move |_, window, cx| {
                                            let link = submit_input.read(cx).value().to_string();
                                            let Some(owner) = owner.upgrade() else {
                                                return;
                                            };
                                            let accepted = owner.update(cx, |this, cx| {
                                                this.catalog.update(cx, |catalog, cx| {
                                                    catalog
                                                        .install_github_mod(id, &link, window, cx)
                                                })
                                            });
                                            if accepted {
                                                window.close_dialog(cx);
                                            } else {
                                                window.push_notification(
                                                    locale.text(
                                                        "Это не ссылка на репозиторий GitHub",
                                                        "This is not a GitHub repository link",
                                                    ),
                                                    cx,
                                                );
                                            }
                                        }),
                                ),
                            ),
                    )
                })
        });
    }
}
