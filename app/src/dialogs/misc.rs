use crate::app::LauncherApp;
use gpui_kit::base::StyledExt;
use gpui_kit::component::WindowExt;
use gpui_kit::component::button::Button as ComponentButton;
use gpui_kit::component::scroll::ScrollableElement;
use gpui_kit::*;

impl LauncherApp {
    pub(crate) fn open_world_picker(
        &mut self,
        id: uuid::Uuid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use crate::features::worlds::WorldList;
        let Some(profile) = self.instances.profile(id).cloned() else {
            return;
        };
        let locale = self.settings.locale();
        let title = format!(
            "{} · {}",
            locale.text("Войти в мир", "Play a world"),
            profile.name
        );
        self.instances.set_worlds(id, WorldList::Loading);
        let task = cx
            .background_executor()
            .spawn(async move { crate::features::worlds::load(profile) });
        cx.spawn(async move |this, cx| {
            let worlds = task.await;
            let _ = this.update(cx, |this, cx| {
                this.instances.set_worlds(id, worlds);
                cx.notify();
            });
        })
        .detach();
        let owner = cx.weak_entity();
        window.open_dialog(cx, move |dialog, _, _| {
            let owner = owner.clone();
            dialog.title(title.clone()).content(move |content, _, cx| {
                let worlds = owner
                    .upgrade()
                    .map(|owner| owner.read(cx).instances.worlds(id));
                let body = match worlds {
                    Some(WorldList::Ready(worlds)) if !worlds.is_empty() => div()
                        .v_flex()
                        .gap_2()
                        .children(worlds.into_iter().map(|world| {
                            let owner = owner.clone();
                            let label = world.clone();
                            ComponentButton::new(format!("quick-play-{world}"))
                                .outline()
                                .label(label)
                                .on_click(move |_, window, cx| {
                                    window.close_dialog(cx);
                                    let _ = owner.update(cx, |this, cx| {
                                        this.launch_into(id, Some(world.clone()), cx)
                                    });
                                })
                        }))
                        .into_any_element(),
                    Some(WorldList::Ready(_)) => div()
                        .child(locale.text(
                            "Миров пока нет. Создай мир в игре — он появится здесь.",
                            "No worlds yet. Create one in the game first.",
                        ))
                        .into_any_element(),
                    Some(WorldList::Failed(error)) => div().child(error).into_any_element(),
                    _ => div()
                        .child(locale.text("Читаем сохранения…", "Loading worlds…"))
                        .into_any_element(),
                };
                content.child(
                    div()
                        .p_4()
                        .max_h(px(420.))
                        .overflow_y_scrollbar()
                        .child(body),
                )
            })
        });
    }

    pub(crate) fn start_profile_launch(&mut self, id: uuid::Uuid, cx: &mut Context<Self>) {
        self.launch_into(id, None, cx);
    }
}
