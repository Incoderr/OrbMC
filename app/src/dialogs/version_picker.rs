use crate::app::LauncherApp;
use crate::features::instances::InstanceProfile;
use crate::features::versions::CreateProfile;
use gpui_kit::component::WindowExt;
use gpui_kit::*;

impl LauncherApp {
    pub(crate) fn open_version_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let locale = self.settings.read(cx).locale();
        let picker = self.version_picker.clone();
        picker.update(cx, |picker, cx| picker.reset(window, cx));
        let title = locale.text("Добавить Minecraft", "Add Minecraft");
        window.open_dialog(cx, move |dialog, _, _| {
            let picker = picker.clone();
            dialog
                .w(px(560.))
                .title(title)
                .content(move |content, _, _| content.child(picker.clone()))
        });
    }

    pub(crate) fn create_profile(&mut self, request: &CreateProfile, cx: &mut Context<Self>) {
        let profile = InstanceProfile::new(
            request.name.clone(),
            request.game_version.clone(),
            request.loader,
            request.loader_version.clone(),
            self.settings.read(cx).download_directory().clone(),
        );
        let profile_id = profile.id;
        self.instances.update(cx, |instances, cx| {
            instances.add(profile);
            instances.save();
            instances.start_install(profile_id, cx);
        });
        cx.notify();
    }
}
