//! Discord Rich Presence.
//!
//! Pushes "playing Minecraft" status updates to Discord over its local IPC
//! pipe. All pipe IO is blocking, so the connection lifetime is owned by the
//! background task started from [`run`]; presentation code only sends small
//! commands through [`RichPresenceHandle`].
//!
//! Icons: profiles without their own cover use the loader icons that must be
//! uploaded to that application's Rich Presence assets under the keys
//! [`VANILLA_ICON_KEY`], [`FORGE_ICON_KEY`], [`FABRIC_ICON_KEY`], and
//! [`NEOFORGE_ICON_KEY`]. Profiles with a build cover pass its public image
//! URL instead; Discord renders public https URLs directly.

use std::time::{SystemTime, UNIX_EPOCH};

use super::instances::{InstanceEvent, InstanceId, InstanceRuntime, InstancesState};
use super::settings::SettingsState;
use super::versions::LoaderChoice;
use discord_rich_presence::{DiscordIpc, DiscordIpcClient, activity};
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use gpui_kit::{Context, Entity, Subscription};

/// Discord application whose name and icons the status shows, taken from
/// `ORBMC_DISCORD_APP_ID` at build time. Official builds get it from CI;
/// builds without it (forks, local builds) simply show no status, so they
/// never appear under someone else's application.
const DISCORD_APPLICATION_ID: Option<&str> = option_env!("ORBMC_DISCORD_APP_ID");

/// Key of the grass block asset in the Discord application's Rich Presence
/// assets; shown for vanilla profiles.
pub const VANILLA_ICON_KEY: &str = "vanilla";

/// Key of the Forge logo asset (upload `forge.png` to the Discord app).
const FORGE_ICON_KEY: &str = "forge";

/// Key of the Fabric logo asset (upload `fabric.png` to the Discord app).
const FABRIC_ICON_KEY: &str = "fabric";

/// Key of the NeoForge logo asset (upload `neoforge.png` to the Discord app).
const NEOFORGE_ICON_KEY: &str = "neoforge";

/// Icon shown next to the status.
#[derive(Clone, Debug)]
pub enum PresenceIcon {
    /// Public image URL (for example a Modrinth or CurseForge build cover);
    /// Discord renders such URLs directly.
    Url(String),
    /// Key of an asset uploaded to the Discord application's Rich Presence
    /// assets.
    Asset(&'static str),
}

impl PresenceIcon {
    /// Maps a loader profile to its status icon; loader icons come from the
    /// application's Rich Presence assets (see the keys above).
    pub fn from_loader(loader: LoaderChoice) -> Self {
        Self::Asset(match loader {
            LoaderChoice::Vanilla => VANILLA_ICON_KEY,
            LoaderChoice::Fabric => FABRIC_ICON_KEY,
            LoaderChoice::Forge => FORGE_ICON_KEY,
            LoaderChoice::NeoForge => NEOFORGE_ICON_KEY,
        })
    }
}

/// Status shown under the game in Discord.
#[derive(Clone, Debug)]
pub struct PresenceActivity {
    /// First (emphasis) line, e.g. the profile name.
    pub details: String,
    /// Icon next to the status, if any.
    pub icon: Option<PresenceIcon>,
    /// Unix seconds the game started at; `0` hides the elapsed timer.
    pub start_unix: u64,
}

#[derive(Debug)]
pub enum PresenceCommand {
    Activity(PresenceActivity),
    Clear,
}

/// Cloneable sender used by the launcher state to update Discord presence.
#[derive(Clone, Debug)]
pub struct RichPresenceHandle {
    commands: UnboundedSender<PresenceCommand>,
}

impl RichPresenceHandle {
    /// Creates the command channel; the returned receiver must be fed to
    /// [`run`] on a background task.
    pub fn session() -> (Self, UnboundedReceiver<PresenceCommand>) {
        let (commands, receiver) = unbounded();
        (Self { commands }, receiver)
    }

    /// Shows (or updates) the "playing Minecraft" status. Sending only
    /// queues a command and never blocks the caller.
    pub fn set_activity(&self, activity: PresenceActivity) {
        let _ = self
            .commands
            .unbounded_send(PresenceCommand::Activity(activity));
    }

    /// Removes the status from Discord profile.
    pub fn clear(&self) {
        let _ = self.commands.unbounded_send(PresenceCommand::Clear);
    }
}

/// Keeps the Discord status in sync with running builds and the settings
/// toggle: a started game shows up, an exited one disappears.
pub struct DiscordPresence {
    handle: RichPresenceHandle,
    instances: Entity<InstancesState>,
    enabled: bool,
    /// Build whose session is currently shown.
    shown: Option<InstanceId>,
    _subscriptions: [Subscription; 2],
}

impl DiscordPresence {
    pub fn new(
        instances: Entity<InstancesState>,
        settings: &Entity<SettingsState>,
        cx: &mut Context<Self>,
    ) -> Self {
        let (handle, commands) = RichPresenceHandle::session();
        cx.background_executor().spawn(run(commands)).detach();
        let subscriptions = [
            cx.subscribe(
                &instances,
                |this, _, event: &InstanceEvent, cx| match *event {
                    InstanceEvent::GameStarted(id) => this.show(id, cx),
                    InstanceEvent::GameExited(id) if this.shown == Some(id) => this.clear(),
                    _ => {}
                },
            ),
            cx.observe(settings, |this, settings, cx| {
                let enabled = settings.read(cx).discord_presence();
                if enabled == this.enabled {
                    return;
                }
                this.enabled = enabled;
                if !enabled {
                    this.clear();
                } else if let Some(id) = this.running_build(cx) {
                    // Turning the toggle on shows an already running game.
                    this.show(id, cx);
                }
            }),
        ];
        Self {
            handle,
            enabled: settings.read(cx).discord_presence(),
            instances,
            shown: None,
            _subscriptions: subscriptions,
        }
    }

    fn running_build(&self, cx: &gpui_kit::App) -> Option<InstanceId> {
        let instances = self.instances.read(cx);
        instances
            .recent_profiles()
            .into_iter()
            .find(|profile| {
                matches!(
                    instances.runtime(&profile.id),
                    InstanceRuntime::Running { .. }
                )
            })
            .map(|profile| profile.id)
    }

    fn show(&mut self, id: InstanceId, cx: &gpui_kit::App) {
        if !self.enabled {
            return;
        }
        let instances = self.instances.read(cx);
        let Some(profile) = instances.profile(id) else {
            return;
        };
        if !matches!(instances.runtime(&id), InstanceRuntime::Running { .. }) {
            return;
        }
        self.shown = Some(id);
        self.handle.set_activity(PresenceActivity {
            details: profile.name.clone(),
            // Build covers (Modrinth/CurseForge metadata) win over the
            // per-loader icons uploaded to the Discord application.
            icon: Some(profile.modpack_icon.clone().map_or_else(
                || PresenceIcon::from_loader(profile.loader),
                PresenceIcon::Url,
            )),
            start_unix: unix_now(),
        });
    }

    fn clear(&mut self) {
        self.shown = None;
        self.handle.clear();
    }
}

/// Unix seconds right now, or `0` when the clock is unavailable.
fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0)
}

/// Owns the Discord IPC connection for as long as the receiver lives.
/// Connects lazily on the first status update and reconnects after failures;
/// commands sent while Discord is not running are simply dropped.
pub async fn run(mut commands: UnboundedReceiver<PresenceCommand>) {
    let Some(application_id) = application_id() else {
        tracing::warn!("Discord Rich Presence is disabled: this build has no ORBMC_DISCORD_APP_ID");
        return;
    };

    let mut client: Option<DiscordIpcClient> = None;
    while let Some(command) = commands.next().await {
        match command {
            PresenceCommand::Activity(update) => {
                client = attach_activity(client, application_id, &update);
            }
            PresenceCommand::Clear => {
                client = detach_activity(client);
            }
        }
    }
}

/// Resolve and validate the baked-in application ID.
fn application_id() -> Option<&'static str> {
    DISCORD_APPLICATION_ID
        .map(str::trim)
        .filter(|id| !id.is_empty())
}

/// Sends `activity` through `client`, reconnecting when the pipe has died or
/// no client was connected yet.
fn attach_activity(
    client: Option<DiscordIpcClient>,
    application_id: &str,
    activity: &PresenceActivity,
) -> Option<DiscordIpcClient> {
    if let Some(mut attached) = client {
        match attached.set_activity(activity_payload(activity)) {
            Ok(_) => return Some(attached),
            Err(error) => tracing::debug!(
                %error,
                "updating Discord presence failed; trying to reconnect"
            ),
        }
    }
    connect(application_id, activity)
}

/// Removes the activity, keeping the pipe alive to reuse on the next update.
fn detach_activity(client: Option<DiscordIpcClient>) -> Option<DiscordIpcClient> {
    match client {
        Some(mut attached) => match attached.clear_activity() {
            Ok(_) => Some(attached),
            Err(error) => {
                tracing::debug!(%error, "clearing Discord presence failed");
                None
            }
        },
        None => None,
    }
}

/// Fresh connection that immediately shows `activity`; `None` means Discord
/// is not reachable right now.
fn connect(application_id: &str, activity: &PresenceActivity) -> Option<DiscordIpcClient> {
    let mut client = DiscordIpcClient::new(application_id);
    match client
        .connect()
        .and_then(|()| client.set_activity(activity_payload(activity)))
    {
        Ok(_) => {
            tracing::debug!("Discord presence connected");
            Some(client)
        }
        Err(error) => {
            tracing::debug!(
                %error,
                "Discord is not reachable; status stays hidden until the next \
                 presence update"
            );
            None
        }
    }
}

fn activity_payload(activity: &PresenceActivity) -> activity::Activity<'_> {
    let mut payload = activity::Activity::new()
        .activity_type(activity::ActivityType::Playing)
        .details(&activity.details);
    if let Some(icon) = &activity.icon {
        let assets = match icon {
            PresenceIcon::Url(url) => activity::Assets::new().large_image(url.as_str()),
            PresenceIcon::Asset(key) => activity::Assets::new().large_image(*key),
        };
        payload = payload.assets(assets);
    }
    if activity.start_unix > 0 {
        payload = payload.timestamps(activity::Timestamps::new().start(activity.start_unix as i64));
    }
    payload
}
