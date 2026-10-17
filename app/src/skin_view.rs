use super::theme::Palette;
use super::*;
use crate::features::skins::SkinModel;
use gpui_kit::component::switch::Switch;
use gpui_kit::{MouseDownEvent, MouseMoveEvent, RenderImage};
use std::sync::Arc;

impl LauncherApp {
    pub(super) fn import_skin(&mut self, cx: &mut Context<Self>) {
        if !self.skins.can_add() {
            self.skin_message = Some(
                self.settings
                    .locale()
                    .text(
                        "В галерее уже 10 скинов. Удали один, чтобы добавить новый.",
                        "The gallery already has 10 skins. Remove one before adding another.",
                    )
                    .to_owned(),
            );
            cx.notify();
            return;
        }
        let locale = self.settings.locale();
        let response = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(
                locale
                    .text(
                        "Выбери PNG-файл скина 64 × 64 или 64 × 32",
                        "Choose a 64 × 64 or 64 × 32 skin PNG",
                    )
                    .into(),
            ),
        });
        cx.spawn(async move |this, cx| {
            let path = match response.await {
                Ok(Ok(Some(paths))) => paths.into_iter().find(|path| path.is_file()),
                Ok(Ok(None)) => None,
                Ok(Err(error)) => {
                    tracing::warn!(%error, "skin picker failed");
                    let _ = this.update(cx, |this, cx| {
                        this.skin_message = Some(error.to_string());
                        cx.notify();
                    });
                    return;
                }
                Err(error) => {
                    tracing::warn!(%error, "skin picker response channel closed");
                    return;
                }
            };
            let Some(path) = path else {
                return;
            };
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("skin.png")
                .to_owned();
            let read = cx.background_executor().spawn(async move {
                std::fs::read(path).map_err(|error| format!("Не удалось прочитать PNG: {error}"))
            });
            let bytes = match read.await {
                Ok(bytes) => bytes,
                Err(error) => {
                    let _ = this.update(cx, |this, cx| {
                        this.skin_message = Some(error);
                        cx.notify();
                    });
                    return;
                }
            };
            let _ = this.update(cx, |this, cx| {
                this.skin_message = this
                    .skins
                    .add(&name, &bytes, SkinModel::Classic, this.settings.locale())
                    .err();
                this.skin_yaw = -0.24;
                this.skin_preview_request = None;
                this.request_skin_preview(cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn rotate_skin(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if !self.skin_dragging {
            return;
        }
        let x = event.position.x.as_f32();
        self.skin_yaw =
            (self.skin_yaw + (x - self.skin_last_x) * 0.012).rem_euclid(std::f32::consts::TAU);
        self.skin_last_x = x;
        cx.notify();
        self.request_skin_preview(cx);
    }

    pub(super) fn begin_skin_drag(&mut self, event: &MouseDownEvent) {
        self.skin_dragging = true;
        self.skin_last_x = event.position.x.as_f32();
    }

    pub(super) fn request_skin_preview(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.skins.selected() else {
            return;
        };
        let Some(model) = self.skins.selected_model() else {
            return;
        };
        let degree = (self.skin_yaw.to_degrees().round() as i16).rem_euclid(360);
        // Keep one renderer active and always present a completed frame while dragging.
        if self.skins.selected_preview_ready(id, degree, model)
            || self.skin_preview_request.is_some()
        {
            return;
        }
        let Some(texture) = self.skins.texture(id) else {
            return;
        };
        self.skin_preview_request = Some((id, degree, model));
        let task = cx.background_executor().spawn(async move {
            crate::features::skins::SkinState::render_frame(
                &texture,
                model,
                f32::from(degree).to_radians(),
                224,
                384,
            )
        });
        cx.spawn(async move |this, cx| {
            let frame = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.skin_preview_request == Some((id, degree, model)) {
                    this.skin_preview_request = None;
                    if this.skins.selected() == Some(id)
                        && this.skins.selected_model() == Some(model)
                        && let Some(previous) =
                            this.skins.cache_selected_preview(id, degree, model, frame)
                    {
                        cx.drop_image(previous, None);
                    }
                    cx.notify();
                    // Catch up to the most recent pointer position without queueing old angles.
                    this.request_skin_preview(cx);
                }
            });
        })
        .detach();
    }

    fn request_skin_thumbnail(&mut self, id: uuid::Uuid, model: SkinModel, cx: &mut Context<Self>) {
        if self.skins.thumbnail(id).is_some() || !self.skin_thumbnails_pending.insert((id, model)) {
            return;
        }
        let task = cx.background_executor().spawn(async move {
            crate::features::skins::SkinState::render_skin_thumbnail(id, model)
        });
        cx.spawn(async move |this, cx| {
            let rendered = task.await;
            let _ = this.update(cx, |this, cx| {
                this.skin_thumbnails_pending.remove(&(id, model));
                match rendered {
                    Ok((texture, frame)) => {
                        this.skins.cache_thumbnail(id, model, texture, frame);
                        this.request_skin_preview(cx);
                    }
                    Err(error) => {
                        this.skin_thumbnails_pending.insert((id, model));
                        this.skin_message = Some(error);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

pub(super) fn skin_page(
    this: &mut LauncherApp,
    palette: Palette,
    locale: Locale,
    cx: &mut Context<LauncherApp>,
) -> AnyElement {
    let selected = this.skins.selected();
    let selected_skin = this.skins.selected_skin().cloned();
    this.request_skin_preview(cx);
    let thumbnail_ids = this
        .skins
        .skins()
        .iter()
        .map(|skin| (skin.id, skin.model))
        .collect::<Vec<_>>();
    for (id, model) in thumbnail_ids {
        this.request_skin_thumbnail(id, model, cx);
    }
    let preview = this.skins.selected_preview();
    let preview_label = selected_skin.as_ref().map_or_else(
        || {
            locale
                .text(
                    "Загрузи скин, чтобы посмотреть его здесь",
                    "Upload a skin to preview it here",
                )
                .to_owned()
        },
        |skin| skin.name.clone(),
    );
    let preview_window = div()
        .relative()
        .w_full()
        .flex_1()
        .min_h(px(180.))
        .max_h(px(420.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(12.))
        .bg(rgb(palette.surface))
        .border_1()
        .border_color(rgb(palette.border))
        .cursor(if this.skin_dragging {
            CursorStyle::ClosedHand
        } else {
            CursorStyle::OpenHand
        })
        .when_some(preview, |view, image: Arc<RenderImage>| {
            view.child(img(image).size_full().object_fit(ObjectFit::Contain))
        })
        .when(selected.is_none(), |view| {
            view.child(
                div()
                    .w(px(210.))
                    .text_size(px(13.))
                    .text_color(rgb(palette.muted))
                    .text_center()
                    .child(preview_label.clone()),
            )
        })
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, event: &MouseDownEvent, _, cx| {
                this.begin_skin_drag(event);
                cx.notify();
            }),
        )
        .on_mouse_move(
            cx.listener(|this, event: &MouseMoveEvent, _, cx| this.rotate_skin(event, cx)),
        )
        .on_mouse_exit(cx.listener(|this, _, _, cx| {
            this.skin_dragging = false;
            this.request_skin_preview(cx);
            cx.notify();
        }))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                this.skin_dragging = false;
                this.request_skin_preview(cx);
                cx.notify();
            }),
        )
        .on_mouse_up_out(
            MouseButton::Left,
            cx.listener(|this, _, _, cx| {
                this.skin_dragging = false;
                this.request_skin_preview(cx);
                cx.notify();
            }),
        );

    let model_controls = selected_skin.as_ref().map(|skin| {
        let id = skin.id;
        let classic = skin.model == SkinModel::Classic;
        div()
            .h_flex()
            .w_full()
            .gap_2()
            .child(
                UiButton::new("skin-model-classic")
                    .flex_1()
                    .px_3()
                    .py_2()
                    .rounded(px(8.))
                    .bg(rgb(if classic {
                        palette.selected
                    } else {
                        palette.surface
                    }))
                    .text_color(rgb(if classic {
                        palette.accent
                    } else {
                        palette.muted
                    }))
                    .selected(classic)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.skin_message = this.skins.set_model(id, SkinModel::Classic).err();
                        this.skin_preview_request = None;
                        this.request_skin_preview(cx);
                        cx.notify();
                    }))
                    .child(locale.text("Широкие руки", "Classic arms")),
            )
            .child(
                UiButton::new("skin-model-slim")
                    .flex_1()
                    .px_3()
                    .py_2()
                    .rounded(px(8.))
                    .bg(rgb(if !classic {
                        palette.selected
                    } else {
                        palette.surface
                    }))
                    .text_color(rgb(if !classic {
                        palette.accent
                    } else {
                        palette.muted
                    }))
                    .selected(!classic)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.skin_message = this.skins.set_model(id, SkinModel::Slim).err();
                        this.skin_preview_request = None;
                        this.request_skin_preview(cx);
                        cx.notify();
                    }))
                    .child(locale.text("Узкие руки", "Slim arms")),
            )
    });
    let delete_button = selected_skin.as_ref().map(|skin| {
        let id = skin.id;
        UiButton::new("remove-saved-skin")
            .w_full()
            .px_3()
            .py_2()
            .rounded(px(8.))
            .bg(rgb(palette.control))
            .text_color(rgb(0xef6a65))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.skin_message = this.skins.remove(id).err();
                cx.notify();
            }))
            .child(locale.text("Удалить из галереи", "Remove from gallery"))
    });

    let mut gallery: Vec<AnyElement> = Vec::new();
    if this.skins.can_add() {
        let add = cx.listener(|this, _, _, cx| this.import_skin(cx));
        gallery.push(
            UiButton::new("add-user-skin")
                .w(px(126.))
                .h(px(168.))
                .flex_shrink_0()
                .v_flex()
                .items_center()
                .justify_center()
                .gap_2()
                .rounded(px(10.))
                .bg(rgb(palette.surface))
                .border_1()
                .border_color(rgb(palette.border))
                .hover(|style| style.bg(rgb(palette.hover)))
                .on_click(add)
                .child(
                    div()
                        .text_size(px(25.))
                        .text_color(rgb(palette.accent))
                        .child(IconName::Plus),
                )
                .child(
                    div()
                        .text_size(px(12.))
                        .font_weight(FontWeight::BOLD)
                        .child(locale.text("Добавить скин", "Add a skin")),
                )
                .child(
                    div()
                        .text_size(px(10.))
                        .text_color(rgb(palette.muted))
                        .child(locale.text("PNG 64 × 64", "PNG 64 × 64")),
                )
                .into_any_element(),
        );
    }
    for skin in this.skins.skins().to_vec() {
        let image = this.skins.thumbnail(skin.id);
        let active = selected == Some(skin.id);
        let id = skin.id;
        gallery.push(
            UiButton::new(format!("saved-skin-{id}"))
                .w(px(126.))
                .h(px(168.))
                .flex_shrink_0()
                .v_flex()
                .items_center()
                .justify_between()
                .gap_1()
                .p_1()
                .rounded(px(10.))
                .bg(rgb(if active {
                    palette.selected
                } else {
                    palette.surface
                }))
                .border_1()
                .border_color(rgb(if active {
                    palette.accent
                } else {
                    palette.border
                }))
                .hover(|style| style.bg(rgb(palette.hover)))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.skin_message = this.skins.select(id).err();
                    this.skin_yaw = -0.24;
                    this.skin_preview_request = None;
                    this.request_skin_preview(cx);
                    cx.notify();
                }))
                .when_some(image, |card, image| {
                    card.child(
                        img(image)
                            .w(px(116.))
                            .h(px(136.))
                            .object_fit(ObjectFit::Contain),
                    )
                })
                .when(this.skins.thumbnail(skin.id).is_none(), |card| {
                    card.child(
                        div()
                            .w(px(116.))
                            .h(px(136.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(rgb(palette.muted))
                            .child(IconName::LoaderCircle),
                    )
                })
                .child(
                    div()
                        .w_full()
                        .px_1()
                        .truncate()
                        .text_size(px(11.))
                        .text_color(rgb(if active {
                            palette.accent
                        } else {
                            palette.foreground
                        }))
                        .child(skin.name),
                )
                .into_any_element(),
        );
    }

    let count = format!("{} / {}", this.skins.skins().len(), this.skins.max_skins());
    let local_enabled = this.skins.apply_locally();
    let local_switch = Switch::new("skin-apply-locally")
        .checked(local_enabled)
        .color(rgb(palette.accent))
        .on_change(cx.listener(|this, checked, _, cx| {
            this.skin_message = this.skins.set_apply_locally(*checked).err();
            cx.notify();
        }));
    div().h_flex().items_stretch().w_full().flex_1().min_h_0().gap_4()
        .child(div().id("skin-preview-column").v_flex().w(px(300.)).flex_shrink_0().h_full().min_h_0().gap_3()
            .child(div().text_size(px(15.)).font_weight(FontWeight::BOLD).child(locale.text("Выбранный скин", "Selected skin")))
            .child(preview_window)
            .child(div().text_size(px(11.)).text_color(rgb(palette.muted)).text_center().child(locale.text("Зажми и веди мышью, чтобы вращать", "Drag to rotate")))
            .when_some(model_controls, |column, controls| column.child(controls))
            .when_some(delete_button, |column, button| column.child(button)))
        .child(div().v_flex().flex_1().h_full().min_w_0().min_h_0().gap_4()
            .child(saved_skin_header(count, palette, locale))
            .child(div().id("saved-skins-scroll").v_flex().w_full().flex_1().min_h_0().overflow_y_scroll().gap_4()
                .child(saved_skin_grid(gallery))
                .child(div().h_flex().w_full().gap_3().justify_between().p_3().rounded(px(8.)).bg(rgb(palette.surface))
                    .child(div().v_flex().flex_1().min_w_0().gap_1()
                        .child(div().text_size(px(12.)).child(locale.text("Использовать в игре локально", "Use locally in game")))
                        .child(div().text_size(px(10.)).text_color(rgb(palette.muted)).child(locale.text("Виден только тебе · применяется при следующем запуске", "Visible only to you · applied on the next launch"))))
                    .child(local_switch))
                .child(div().text_size(px(11.)).text_color(rgb(palette.muted)).child(locale.text("Для локального скина нужна сборка с Forge, Fabric или NeoForge. Мод скинов установится при запуске.", "Local skins require Forge, Fabric or NeoForge. The skin mod is installed when launching.")))
                .when_some(this.skin_message.as_ref(), |column, message| column.child(div().p_3().rounded(px(8.)).bg(rgb(palette.control)).text_size(px(12.)).text_color(rgb(0xef8580)).child(message.clone())))
                .when_some(this.skins.error(), |column, message| column.child(div().p_3().rounded(px(8.)).text_size(px(12.)).text_color(rgb(0xef8580)).child(message.to_owned())))))
        .into_any_element()
}

fn saved_skin_header(count: String, palette: Palette, locale: Locale) -> Div {
    div()
        .h_flex()
        .w_full()
        .gap_3()
        .justify_between()
        .flex_shrink_0()
        .child(
            div()
                .debug_selector(|| "skin-gallery-title".into())
                .text_size(px(16.))
                .font_weight(FontWeight::BOLD)
                .child(locale.text("Сохранённые скины", "Saved skins")),
        )
        .child(
            div()
                .debug_selector(|| "skin-gallery-count".into())
                .flex_shrink_0()
                .px_2()
                .py_1()
                .rounded(px(6.))
                .bg(rgb(palette.control))
                .text_size(px(11.))
                .text_color(rgb(palette.muted))
                .child(count),
        )
}
fn saved_skin_grid(gallery: Vec<AnyElement>) -> impl IntoElement {
    div()
        .id("saved-skins-grid")
        .flex()
        .flex_wrap()
        .content_start()
        .items_start()
        .w_full()
        .gap_3()
        .children(gallery)
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;
    struct GalleryLayout;
    impl Render for GalleryLayout {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let palette = Palette::for_settings(ThemeChoice::Dark, AccentColor::Purple);
            div()
                .v_flex()
                .w(px(270.))
                .h(px(600.))
                .gap_4()
                .child(saved_skin_header("1 / 10".into(), palette, Locale::Ru))
                .child(
                    div()
                        .id("test-skin-scroll")
                        .v_flex()
                        .flex_1()
                        .min_h_0()
                        .overflow_y_scroll()
                        .child(saved_skin_grid(vec![
                            div()
                                .debug_selector(|| "skin-gallery-card".into())
                                .w(px(126.))
                                .h(px(168.))
                                .bg(rgb(0x222222))
                                .into_any_element(),
                        ])),
                )
        }
    }
    #[gpui_kit::test]
    fn gallery_stays_under_the_header_and_counter_is_separate(cx: &mut TestAppContext) {
        let (_, cx) = cx.add_window_view(|_, _| GalleryLayout);
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let title = cx.debug_bounds("skin-gallery-title").unwrap();
        let count = cx.debug_bounds("skin-gallery-count").unwrap();
        let card = cx.debug_bounds("skin-gallery-card").unwrap();
        assert!(count.origin.x >= title.origin.x + title.size.width + px(12.));
        assert!(card.origin.y <= title.origin.y + title.size.height + px(24.));
        assert!(card.origin.y >= title.origin.y + title.size.height);
    }
}
