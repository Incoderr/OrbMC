# AGENTS.md

Guidance for AI agents and contributors working on **OrbMC**, a native
Minecraft launcher written in Rust with a GPUI interface.

Read this file before changing code. It describes the project as it **is**,
the rules it follows, and what is still planned. When this file and the code
disagree, trust the code and update this file in the same change.

## 1. Project in one paragraph

OrbMC is a desktop Minecraft launcher. The UI is GPUI through `gpui-kit`
(`gpui-component` primitives). Launcher logic is Rust. Java is only the
runtime that Minecraft itself needs; the launcher never depends on Java. No
web runtime (Electron, Tauri, WebView, React, Node.js) unless the user
explicitly asks for one. The UI language is Russian first, English second.

## 2. Repository layout (current)

```text
app/                         binary crate `orbmc` (GPUI application)
  src/main.rs                entry: single-instance check, window, assets
  src/app.rs                 LauncherApp: creates features, navigation, window frame
  src/router.rs              Page enum (navigation targets)
  src/sidebar.rs             sidebar frame: navigation buttons, collapse toggle
  src/pages/home.rs          download indicator shown under every page
  src/dialogs/               thin glue between feature dialogs and the app
  src/features/              feature modules (state, services, views) — see §4
  src/shared/                locale, theme/palette, tiny shared UI pieces
  src/platform/              OS integration (no GPUI): storage, paths, processes,
                             tray, single instance, caches
  src/shell/                 app shell glue: HTTP client, devtools, close-to-tray
  tests/inspector.rs         regression tests for the patched GPUI inspector
core/                        library crate `orbmc-core` (no GPUI): version
                             manifests, loader metadata and build picking,
                             Java requirement and selection, JVM launch
                             arguments, Quick Play, skins
vendor/
  mc-launcher-core/          patched copy of mc-launcher-core 0.1.2 — the real
                             install/launch engine (see §3)
  gpui-pre/                  patched copy of gpui-pre 0.3.7 (inspector fixes)
assets/logo/                 application icon (embedded by app/build.rs)
```

`platform` is a module inside `app`, not a separate crate. Do not create
crates for symmetry; extract a crate only for a concrete reason (reuse,
dependency isolation, compile times, testability without GPUI).

## 3. Vendored crates

Both vendored crates are used through `[patch.crates-io]` in the root
`Cargo.toml` and are **excluded** from the workspace.

- `vendor/mc-launcher-core` does the heavy Minecraft work: version metadata
  and inheritance, client/library/asset/native downloads, Fabric/Quilt/Forge/
  NeoForge installation, Java runtime install, launch command construction,
  offline and Microsoft account helpers. OrbMC calls it from
  `features/instances/worker.rs`.
- `vendor/gpui-pre` only carries inspector fixes.

Rules:

- Prefer solving problems in OrbMC code. Patch a vendored crate only when the
  fix cannot live outside it.
- Every vendored change must be listed in `vendor/README.md` with the reason.
- Keep the vendored code close to upstream so it can be updated or dropped.
- Minecraft logic that OrbMC writes itself belongs in `core`, not in `vendor`.

## 4. Architecture

Dependency direction, never upward:

```text
app (LauncherApp, router, sidebar, pages, dialogs)
  ↓
features
  ↓
shared (locale, theme, small UI pieces)
  ↓
core / platform / vendor crates
```

- `core` and `platform` never depend on GPUI or on `features`.
- `shared` never depends on `features`.
- Features may use other features' public entities (a build page reads the
  catalog's installed-mods list), but never `app`, `pages` or `router`.
- A page composes features; it does not own their logic.

### Features

| Feature | Owns | Public entities / events |
|---|---|---|
| `instances` | builds: model, disk layout and migration, runtime state, worker threads, build screens and dialogs | `InstancesState` (+ `InstanceEvent`), `InstancesView` (+ `InstancesViewEvent`) |
| `mods` | content catalog, project pages, installs into builds, installed-mods index and scans, translation | `CatalogView` (+ `CatalogEvent`) |
| `versions` | version/loader catalogs, "Add Minecraft" dialog | `VersionPicker` (+ `CreateProfile`) |
| `modpacks` | installing Modrinth `.mrpack` packs as new builds (index parsing is in `core::minecraft::mrpack`) | `install_pack`, `ModpackError` |
| `settings` | launcher settings, `settings.json`, settings page | `SettingsState`, `SettingsView` |
| `skins` | skin library and 3D preview; shows the Ely.by skin of the playing account and sends library skins to the Ely.by upload page (Ely.by has no upload API). Offline mode sends no skin to the game | `SkinsView` |
| `auth` | account menu: offline name or Ely.by accounts (sign-in, token refresh, authlib-injector) | `AuthState`, `AccountView`, `PlayerAccount` |
| `rich_presence` | Discord status | `DiscordPresence` |
| `java` | probing, discovering and downloading Java (rules live in `core`) | functions, `JavaError` |
| `worlds` | world lists for Quick Play | functions |
| `downloads` | file transfers for the catalog, global download summary | `TransferHandle`, `download_file` |
| `updates` | launcher self-update from GitHub Releases (check, signed download, install) | `Updater`, `UpdateStatus` |

A feature module typically looks like this (create only what is needed):

```text
features/<name>.rs        module root: re-exports, small types
features/<name>/model.rs  domain types and persistence models
features/<name>/state.rs  runtime state
features/<name>/service.rs async/long work that updates the state
features/<name>/view.rs   GPUI entity, Render impl, events
features/<name>/view/*.rs pages, dialogs, widgets of this feature
```

Feature-specific widgets stay in the feature (`mods/view/widgets.rs`), not
in `shared`. No `utils.rs`, `helpers.rs`, `common.rs`, global `models/` or
`services/` directories.

## 5. GPUI patterns used in this project

These are the conventions the code already follows. Use them for new work.

- **State lives in entities.** `LauncherApp` holds `Entity<...>` handles,
  never one giant mutable state. Shared models: `Entity<InstancesState>`,
  `Entity<SettingsState>`.
- **Views are entities with `Render`.** A feature view (`CatalogView`,
  `InstancesView`, …) is created once in `LauncherApp::new` and embedded with
  `entity.clone().into_any_element()`.
- **Features follow shared models themselves.** A view gets the model handle
  in its constructor and calls `cx.observe(&model, …)`; the app does not push
  theme, language or player name into views on every render. Use
  `cx.observe_in(&model, window, …)` when the reaction needs `window`
  (input placeholders).
- **Features ask the app to navigate through events.** A view implements
  `EventEmitter<SomeEvent>`; the app subscribes with
  `cx.subscribe_in(&view, window, …)` and calls `navigate_to`.
- **Models announce lifecycle moments through events.** `InstancesState`
  emits `InstanceEvent::{GameStarted, GameExited, Removed}`; Discord presence
  and the catalog subscribe to it.
- **Mutate through the owner.** `model.update(cx, |state, cx| { …; cx.notify(); })`.
  Forgetting `cx.notify()` means observers never fire.
- **Read without holding the borrow.** `model.read(cx)` borrows `cx`; copy what
  you need into locals before calling `cx.listener`, `cx.new`, etc.
- **No re-entrant updates.** Never update an entity from inside its own
  `update`. Events and observers run after the update ends, so emitting from
  inside `update` is fine.
- **Async work.** `cx.background_executor().spawn(…)` for CPU/IO, then
  `cx.spawn(async move |this, cx| …)` to apply the result with
  `this.update(cx, …)`. Build workers (install, launch, removal) are OS
  threads sending `InstanceWorkerEvent`s over a channel.
- **Blocking is fine off the UI thread.** `reqwest::blocking`, file IO and
  hashing are allowed inside `background_executor` tasks and worker threads,
  never in `render`, listeners or `update` closures.
- **Clickable things show a hand.** Every `UiButton`/`ComponentButton`
  gets `.cursor_pointer()`; neither sets it by itself.
- **Text people may want to copy** (names, messages, logs) goes through
  `shared::ui::selectable` (`SelectableText`); Markdown views use
  `.selectable(true)`. The selection layer is rendered by the window root.
- **Single-line inputs ignore `.h()`.** Their height comes from the size
  (`Sizable`: medium 32px, `.large()` 44px); size neighbouring buttons to match.
- **Input changes.** `InputState::set_value` does **not** emit
  `InputEvent::Change`; apply the effect yourself after setting a value in
  code.

## 6. Project rules

### Data on disk

- Launcher data: `%LOCALAPPDATA%\OrbMC` (`platform::profile_store::data_directory`).
  `settings.json`, `instances.json`, `accounts.json` (Ely.by accounts without
  tokens; tokens are in the OS credential store via `platform::credentials`),
  `authlib-injector.jar`, `skins/`, `translations/`,
  `instance-data/<instance id>/`.
- A build's game directory is `<download dir>/instances/<build name>` with
  `(1)`, `(2)` suffixes for duplicates (`platform::instance_layout`). It holds
  **only files the game uses**.
- Launcher bookkeeping for a build lives in `instance-data/<id>/`: mod index
  `mods.json`, `mod-icons/`, `game-cache/` (usercache,
  usernamecache — moved in before launch, out after exit), `jvm-logs/`.
- Identity is the typed id (`InstanceId`, `SkinId`), never a path or a name.
- Writes of important files are atomic (`profile_store::write_json_atomically`).
- **Migrations are backward compatible.** New fields use `#[serde(default)]`;
  old layouts are upgraded on startup (`instances::migrate_layout`,
  `profile_store::relocate_legacy_path` for the old `RS MC Launcher` folder).
  Never delete user data during a migration.

### Code

- Rust edition 2024. Prefer explicit, readable code over clever abstractions.
- Typed IDs, enums for finite states, `Option` only for genuinely optional data.
- No `unwrap()`/`expect()` for recoverable runtime failures (tests are fine).
- Errors: some older helpers return `Result<_, String>` with a user-facing
  message. New code uses typed errors with `thiserror` (see `java::JavaError`,
  `instances::LaunchError`):
  variants carry data, `#[error(...)]` holds the user-facing text, callers log
  the error with `tracing` and show `to_string()`.
- Logging: `tracing` with structured fields. Never log tokens, credentials
  or secrets.
- Localized UI text: `locale.text("русский", "English")` with `Locale` from
  `shared::locale`. Every user-visible string has both languages.
- OS-specific code goes behind `platform` APIs; no `#[cfg(target_os)]` in
  features or views.
- Match the surrounding comment density and naming. Comments explain *why*.
- Files use CRLF line endings (Windows checkout); `cargo fmt` keeps them.

### CurseForge API key

`features::mods::curseforge_api_key()` is the only place that reads it: the
`CURSEFORGE_API_KEY` environment variable at run time, otherwise the value of
`CURSEFORGE_API_KEY` at **build** time (baked in with `option_env!`). Release
builds get it from a CI secret; never commit the key. CurseForge is currently switched
off entirely (`features::mods::CURSEFORGE_ENABLED = false`): the catalog and
mod identification use Modrinth only and the source filter is hidden. Without a key the
catalog searches Modrinth only and says so under the source filter.

### Build-time values and logs

- `ORBMC_DISCORD_APP_ID` (Discord status), `ORBMC_UPDATE_PUBLIC_KEY`
  (minisign key for self-updates) and `CURSEFORGE_API_KEY` are read with
  `option_env!` at build time; CI passes them (`vars` / `secrets`). Builds
  without them just lack the feature (without the update key the launcher
  only links to new releases).
- Self-update: `features::updates` reads the latest GitHub release, downloads
  `orbmc.exe` and verifies `orbmc.exe.minisig` before
  `platform::self_update` swaps the executable (the running one is renamed to
  `orbmc.old.exe`, removed on the next start). Debug builds never install.
  Releases come from `.github/workflows/release.yml` on a `vX.Y.Z` tag that
  must match the workspace version; it signs with `rsign` using the
  `ORBMC_UPDATE_SECRET_KEY` secret.
- The launcher logs to `<data>/logs/launcher.log` (`platform::logging`); the
  previous run is `launcher.old.log`. Game output of the latest launch is kept
  per build (`instances::game_log`) and shown in the build page's "Log" tab.
- CI (`.github/workflows/checks.yml`) runs `fmt --check`, `clippy -D warnings`
  and the tests on every push: keep the tree warning-free.

### Dependencies

Before adding one: check std and existing dependencies, the crate's
maintenance, and its dependency tree. No overlapping libraries for the same
job. No architecture frameworks (DI containers, command/event buses,
Redux-style reducers).

### Avoid over-engineering

Simple locally, strict boundaries globally. No traits with one
implementation, no repository traits per data source, no DTO copies of
identical types, no factories for trivial construction. Introduce an
abstraction when it solves a real boundary, testing or ownership problem.

## 7. Checks

```bash
cargo fmt --all
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets
cargo test --workspace
```

- Fix warnings your change introduces. Existing clippy warnings in
  `mods/catalog.rs` and `platform/tray.rs` are known.
- On Windows `cargo test --workspace` fails with "access denied" while the
  launcher is running (it relinks `orbmc.exe`). Close the launcher, or run
  `cargo test -p orbmc --bin orbmc` and `cargo test -p orbmc-core`.
- Test core logic without GPUI: version metadata, loader metadata, rules,
  argument and classpath construction, checksums, serialization, migrations,
  Java selection, state transitions.

## 8. Workflow

1. Read the code you are about to change and the feature that owns it.
2. Decide ownership with the questions below; keep the dependency direction.
3. Implement domain/state, then the service, then the view.
4. Handle loading, empty, success, failure and cancellation states.
5. Add tests for non-trivial logic.
6. Run the checks. Do not refactor unrelated code in the same change.

Where does new code go?

```text
Minecraft domain logic (no UI)?      → core (or a vendor patch if unavoidable)
OS integration?                      → app/src/platform
Owned by one feature?                → app/src/features/<feature>
Navigation / window frame?           → app.rs, router.rs, sidebar.rs
Reusable visual piece, no feature?   → app/src/shared
Unsure?                              → keep it local to the feature until clear
```

## 9. Status and roadmap

Done:

- Feature entities for builds, catalog, versions, settings, skins, account,
  Discord; `LauncherApp` is only the frame and navigation.
- Typed `InstanceId` / `SkinId`; build folders named after builds; launcher
  files moved out of build folders; single-instance launcher.
- Minecraft rules OrbMC owns (Java requirement and selection, loader build
  picking, Forge version ids, JVM launch arguments) live in `core` with tests.
- Typed errors at the build boundaries: `JavaError`, `InstallError`,
  `LaunchError` (`instances/worker.rs`). Mod and catalog helpers still return
  `Result<_, String>`: their callers only show the message. Convert them when
  a caller needs to tell failures apart.
- Downloads: game files and Java go through the `mc-launcher-core` engine;
  files OrbMC fetches itself (mods, resource packs, shaders) go through
  `downloads::download_file` — streaming, size/SHA-1 checks, atomic
  completion, retries, progress and cancellation via `TransferHandle`.
- Ely.by accounts: password sign-in (with a one-time code for 2FA), token
  checked and refreshed before every launch, game started with
  authlib-injector pointing at `ely.by`.
- Catalog: mods, modpacks, texture packs and shaders share one page
  (`Page::Catalog`) with kind tabs.
- Java: the exact major version a game declares is used; if it is not
  installed, the matching Mojang JRE is downloaded. A newer compatible Java
  is only the fallback when that download fails.

Planned (not present yet — do not look for it):

1. **Accounts** (`auth`): Microsoft sign-in. Ely.by sign-in is done; a
   local Yggdrasil server in the launcher (LAN, own servers) is still being
   designed.
2. Single instance and tray on Linux and macOS.
