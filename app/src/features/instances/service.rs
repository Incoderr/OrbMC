//! Long-running build operations: install, launch and removal run on worker
//! threads and report back through [`InstanceWorkerEvent`]s, which update the
//! build state here. Other features follow builds through [`InstanceEvent`].

use crate::features::auth::PlayerAccount;
use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use gpui_kit::{Context, EventEmitter};
use mc_launcher_core::prelude::DownloadControl;

use super::{
    InstanceId, InstanceWorkerEvent, InstancesState, install_profile, launch_profile,
    remove_profile_data,
};
use crate::platform::profile_store;

/// Moments of a build's life other features react to.
#[derive(Clone, Copy, Debug)]
pub enum InstanceEvent {
    GameStarted(InstanceId),
    GameExited(InstanceId),
    Removed(InstanceId),
}

impl EventEmitter<InstanceEvent> for InstancesState {}

impl InstancesState {
    /// Writes the builds to disk; failures are logged, the state stays.
    pub fn save(&self) {
        if let Err(error) = profile_store::save(&self.profiles) {
            tracing::error!(%error, "failed to persist Minecraft profiles");
        }
    }

    pub fn start_install(&mut self, id: InstanceId, cx: &mut Context<Self>) {
        let Some(profile) = self.profile(id).cloned() else {
            return;
        };
        let minecraft_directory = profile_store::shared_game_directory(&profile.installation_root);

        self.mark_installing(id);
        let control = DownloadControl::new();
        self.set_install_control(id, control.clone());
        let (sender, receiver) = futures::channel::mpsc::unbounded();
        let worker = std::thread::Builder::new()
            .name(format!("install-minecraft-{id}"))
            .spawn(move || install_profile(profile, minecraft_directory, control, sender));
        match worker {
            Ok(_) => self.listen(receiver, cx),
            Err(error) => self.mark_failed(id, format!("Не удалось запустить установщик: {error}")),
        }
        cx.notify();
    }

    /// Starts the game, optionally straight into a singleplayer world. A build
    /// that is not installed yet gets installed instead.
    pub fn launch(
        &mut self,
        id: InstanceId,
        world: Option<String>,
        player: PlayerAccount,
        cx: &mut Context<Self>,
    ) {
        if self.runtime(&id).is_busy() {
            return;
        }
        let Some(profile) = self.profile(id).cloned() else {
            return;
        };
        if profile.installed_version_id.is_none() {
            self.start_install(id, cx);
            return;
        }
        let minecraft_directory = profile_store::shared_game_directory(&profile.installation_root);

        self.mark_launching(id);
        let (sender, receiver) = futures::channel::mpsc::unbounded();
        let worker = std::thread::Builder::new()
            .name(format!("launch-minecraft-{id}"))
            .spawn(move || launch_profile(profile, minecraft_directory, world, player, sender));
        match worker {
            Ok(_) => self.listen(receiver, cx),
            Err(error) => self.mark_failed(id, format!("Не удалось запустить игру: {error}")),
        }
        cx.notify();
    }

    /// Deletes the build folders; the build disappears once that succeeds.
    pub fn start_removal(&mut self, id: InstanceId, cx: &mut Context<Self>) {
        let Some(profile) = self.profile(id).cloned() else {
            return;
        };
        if self.runtime(&id).is_busy() {
            return;
        }
        self.mark_removing(id);
        let (sender, receiver) = futures::channel::mpsc::unbounded();
        let worker = std::thread::Builder::new()
            .name(format!("remove-profile-{id}"))
            .spawn(move || remove_profile_data(profile, sender));
        match worker {
            Ok(_) => self.listen(receiver, cx),
            Err(error) => {
                self.mark_removal_failed(id, format!("Не удалось запустить удаление: {error}"))
            }
        }
        cx.notify();
    }

    fn listen(&self, mut events: UnboundedReceiver<InstanceWorkerEvent>, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            while let Some(event) = events.next().await {
                let _ = this.update(cx, |this, cx| {
                    this.handle_worker_event(event, cx);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn handle_worker_event(&mut self, event: InstanceWorkerEvent, cx: &mut Context<Self>) {
        match event {
            InstanceWorkerEvent::Progress { id, update } => self.update_progress(id, update),
            InstanceWorkerEvent::Installed { id, version_id } => {
                self.finish_install(id);
                self.mark_installed(id, version_id);
                if let Err(error) = profile_store::save(&self.profiles) {
                    tracing::error!(%error, "failed to save installed Minecraft profile");
                    self.mark_failed(
                        id,
                        format!("Установка завершилась, но профиль не сохранился: {error}"),
                    );
                }
            }
            InstanceWorkerEvent::InstallFailed { id, message } => {
                self.finish_install(id);
                self.mark_failed(id, message);
            }
            InstanceWorkerEvent::LaunchFailed { id, message } => self.mark_failed(id, message),
            InstanceWorkerEvent::ProfileRemoved { id } => {
                self.remove(id);
                self.save();
                cx.emit(InstanceEvent::Removed(id));
            }
            InstanceWorkerEvent::ProfileRemovalFailed { id, message } => {
                self.mark_removal_failed(id, message)
            }
            InstanceWorkerEvent::Launched { id, process_id } => {
                self.mark_running(id, process_id);
                self.save();
                cx.emit(InstanceEvent::GameStarted(id));
            }
            InstanceWorkerEvent::PreparingJava { id, detail } => {
                self.mark_preparing_java(id, detail)
            }
            InstanceWorkerEvent::CustomJavaMissing { id } => {
                self.set_java_path(id, None);
                self.save();
            }
            InstanceWorkerEvent::Output { id, line } => self.push_log_line(id, line),
            InstanceWorkerEvent::GameExited { id, code, stderr } => {
                self.mark_game_stopped(id, code, stderr);
                // Keeps the play time of the session that just ended.
                self.save();
                cx.emit(InstanceEvent::GameExited(id));
            }
        }
    }
}
