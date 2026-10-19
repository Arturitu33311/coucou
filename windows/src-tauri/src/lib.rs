// Coucou for Windows — app wiring and the commands the island calls.

mod activities;
mod agent_hooks;
mod agents;
mod bars;
mod calendar;
mod chat;
mod claude;
mod claude_sessions;
mod clipboard;
mod codex_plan;
mod config_file;
mod desktop;
#[cfg(target_os = "linux")]
mod dictation;
mod drive;
mod files;
mod github;
mod hooks;
mod i18n;
mod identity;
mod integrations;
mod island;
mod jinx;
mod jinx_gateway;
mod local_chat;
mod log;
mod music;
mod net;
mod hub_notes;
mod notifs;
mod openai_compat;
mod opencode;
mod phone_link;
mod pipe;
mod pendientes;
mod presence;
mod pomodoro;
mod remote;
mod quick;
mod platform;
#[cfg(target_os = "linux")]
mod portal;
mod recap;
mod secrets;
#[cfg(target_os = "linux")]
mod peripherals;
mod sensitive;
mod session_window;
mod settings;
mod share;
mod shortcuts;
mod sounds;
mod spotify;
mod sysmon;
#[cfg(target_os = "linux")]
mod sysstate;
mod tasks;
mod tray;
mod weather;
#[cfg(windows)]
mod webview_drop;
mod workspaces;

use std::process::Command;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_autostart::{ManagerExt, MacosLauncher};

use chat::{Chat, ChatContext, ChatReply, ModelInfo};
use files::DroppedFile;
use hooks::{HookPreview, HookStatus};
use island::{PollGate, ScreenInfo};
use pipe::Pending;
use settings::Settings;

pub struct Shared {
    pub settings: Mutex<Settings>,
    pub gate: Arc<PollGate>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootInfo {
    settings: Settings,
    screen: ScreenInfo,
    version: String,
    hook_path: String,
    /// False where the OS has no global cursor (Wayland): the page then reports
    /// the cursor from its own mouse events.
    cursor_poll: bool,
}

/// Is this the device in use (and what the other one hears and sees)? The page paints Mochi and
/// Coucou only while it is, and keeps asking nothing else of it.
#[tauri::command]
fn presence_state() -> serde_json::Value {
    presence::state()
}

/// The page says whether this device is playing something: the other one's Mochi dances to it.
#[tauri::command]
fn presence_local_music(app: AppHandle, playing: bool) {
    presence::set_local_music(&app, playing);
}

/// Claude Code is (or is no longer) rate-limited here: the phone's Mochi is tired with it.
#[tauri::command]
fn presence_local_limit(limited: bool) {
    presence::set_local_limit(limited);
}

/// Something arrived on this device (a notification): the phone's Mochi is startled too.
#[tauri::command]
fn presence_local_news() {
    presence::set_local_news();
}

#[tauri::command]
fn boot(app: AppHandle, shared: State<Shared>) -> BootInfo {
    let mut settings = shared.settings.lock().unwrap().clone();
    // The real state of ~/.claude/settings.json wins over whatever we stored.
    let hooks_status = hooks::status();
    settings.hooks_installed = hooks_status.installed;
    settings.plan_relay_installed = hooks_status.plan_relay_installed;
    let screen = island::screen_info(&app, &settings.screen);
    BootInfo {
        settings,
        screen,
        version: env!("CARGO_PKG_VERSION").to_string(),
        hook_path: settings::hook_exe_path().to_string_lossy().to_string(),
        cursor_poll: platform::CURSOR_POLL,
    }
}

#[tauri::command]
fn save_settings(app: AppHandle, shared: State<Shared>, settings: Settings) {
    let (screen_changed, autostart_changed, shortcuts_changed) = {
        let mut current = shared.settings.lock().unwrap();
        let screen_changed = current.screen != settings.screen || current.island_offset_x != settings.island_offset_x;
        let autostart_changed = current.autostart != settings.autostart;
        let shortcuts_changed = current.shortcuts != settings.shortcuts;
        // Where Mochi sits on the desktop is desktop.rs's to say, not a webview's.
        let mut settings = settings.clone();
        settings.desktop_mochi = current.desktop_mochi.clone();
        // Same for the phone link: only its own commands turn it on or off.
        settings.phone_link = current.phone_link;
        settings.phone_chat = current.phone_chat;
        settings.phone_details = current.phone_details;
        settings.phone_answers = current.phone_answers;
        settings.phone_diffs = current.phone_diffs;
        settings.phone_usage = current.phone_usage;
        settings.phone_services = current.phone_services.clone();
        settings.phone_relay = current.phone_relay;
        settings.phone_relay_url = current.phone_relay_url.clone();
        settings.phone_chat_models = current.phone_chat_models.clone();
        *current = settings;
        (screen_changed, autostart_changed, shortcuts_changed)
    };
    let settings = shared.settings.lock().unwrap().clone();
    if let Err(err) = settings::save(&settings) {
        log::line(format!("could not save settings: {err}"));
    }
    // The Music pill's switch decides whether the players are watched at all.
    music::sync_enabled(&app, settings.active_integrations.iter().any(|x| x == "integration_music"));
    notifs::sync_enabled(&app, settings.notification_peek);
    remote::sync_enabled(&app, settings.remote_enabled);
    clipboard::sync_enabled(&app, settings.clipboard_history);
    if autostart_changed {
        let manager = app.autolaunch();
        let result = if settings.autostart { manager.enable() } else { manager.disable() };
        if let Err(err) = result {
            eprintln!("[coucou] autostart: {err}");
        }
    }
    if screen_changed {
        let collapsed = shared.gate.collapsed.load(Ordering::Relaxed);
        island::apply_geometry(&app, &settings.screen, collapsed);
    }
    integrations::settings_saved(&app, &settings.active_integrations);
    spotify::sync(&app, &settings.active_integrations);
    if shortcuts_changed {
        shortcuts::apply(&app, &settings.shortcuts);
    }
    if i18n::set_picked(&settings.language) {
        language_changed(&app);
    }
    // Keep the other window in step (island ⇄ settings window).
    let _ = app.emit("settings-changed", settings);
}

/// The island reports the system's languages at launch, for "System" in
/// Settings → Language (WebView2 and WebKitGTK know them best).
#[tauri::command]
fn set_system_languages(app: AppHandle, languages: Vec<String>) {
    if i18n::set_system(languages) {
        language_changed(&app);
    }
}

/// What Rust labels itself follows the new language: the tray menu and the
/// settings window's title. The webviews switch on their own.
fn language_changed(app: &AppHandle) {
    tray::retitle(app);
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.set_title(&i18n::t("Settings — Coucou"));
    }
}

/// Hidden island → shrink the window to the invisible wake strip and park the
/// cursor poll; anything else → full panel and 60 Hz polling.
#[tauri::command]
fn set_collapsed(app: AppHandle, shared: State<Shared>, collapsed: bool) {
    let pref = shared.settings.lock().unwrap().screen.clone();
    shared.gate.collapsed.store(collapsed, Ordering::Relaxed);
    island::apply_geometry(&app, &pref, collapsed);
    // The wake strip must always take the mouse, and a resize invalidates the flag.
    island::refresh_click_through(&app, &shared.gate);
    shared.gate.set_active(!collapsed);
    platform::set_pointer_watch(!collapsed);
}

/// The front end pushes the island shape; Rust decides click-through from it.
#[tauri::command]
fn set_island_rect(app: AppHandle, shared: State<Shared>, x: f64, y: f64, width: f64, height: f64) {
    shared.gate.set_rect(island::IslandRect { x, y, w: width, h: height });
    // Without the cursor poll the input region is the click-through: it follows the island.
    if !platform::CURSOR_POLL {
        island::refresh_click_through(&app, &shared.gate);
    }
}

#[tauri::command]
fn focus_window(app: AppHandle, focused: bool) {
    let Some(win) = island::window(&app) else { return };
    platform::set_activating(&win, focused);
    if focused {
        let _ = win.set_focus();
    }
}

#[tauri::command]
fn reposition(app: AppHandle, shared: State<Shared>) {
    let pref = shared.settings.lock().unwrap().screen.clone();
    let collapsed = shared.gate.collapsed.load(Ordering::Relaxed);
    island::apply_geometry(&app, &pref, collapsed);
}

/// The displays the island can be pinned to, for Settings.
#[tauri::command]
fn list_monitors(app: AppHandle) -> Vec<island::MonitorChoice> {
    island::monitor_choices(&app)
}

#[tauri::command]
fn open_url(url: String) {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return;
    }
    platform::open_url(&url);
}

/// "Open terminal" opens the working folder in VS Code when `code` is on PATH,
/// and falls back to the file manager otherwise.
#[tauri::command]
fn open_in_vscode(path: Option<String>) -> bool {
    // No shell anywhere near this. The path is a project folder chosen by
    // whoever is using Claude Code, and a shell would happily read `&`, `^`, `%`
    // or `$` in a folder name as syntax. Finding the launcher ourselves and
    // handing the path over as a separate argument keeps it a path.
    let path = path.filter(|p| !p.is_empty());
    // It arrives in a hook payload: only an existing folder, given by its full
    // path, goes any further. `code` would read `--something` as an option, and
    // xdg-open would launch a file with whatever handles its type.
    if let Some(p) = path.as_deref() {
        let p = std::path::Path::new(p);
        if !(p.is_absolute() && p.is_dir()) {
            return false;
        }
    }
    if let Some(code) = platform::find_on_path("code") {
        let mut cmd = Command::new(code);
        if let Some(p) = path.as_deref() {
            cmd.arg(p);
        }
        if platform::no_console(&mut cmd).spawn().is_ok() {
            return true;
        }
    }
    if let Some(p) = path.as_deref() {
        platform::reveal_folder(p);
    }
    false
}

/// "Open terminal": brings forward the terminal or editor window the session
/// runs in, when it was found (see session_window.rs); otherwise opens the
/// folder in VS Code, as before.
#[tauri::command]
fn open_session(session_id: Option<String>, path: Option<String>) -> bool {
    let Some(owners) = session_id.as_deref().and_then(session_window::lookup) else {
        return open_in_vscode(path);
    };
    let folder = path.as_deref().map(session_window::folder_name).unwrap_or_default().to_string();
    #[cfg(windows)]
    {
        if platform::focus_session_window(&owners, &folder) {
            return true;
        }
        open_in_vscode(path)
    }
    // Linux asks the display server, KWin or the terminal, which can take a
    // moment: never on the UI thread a sync command runs on. VS Code still
    // opens when none of them could bring the window forward.
    #[cfg(target_os = "linux")]
    {
        std::thread::spawn(move || {
            if !platform::focus_session_window(&owners, &folder) {
                open_in_vscode(path);
            }
        });
        true
    }
}

/// The Claude Desktop pill's target: the Claude app (Windows only — it has no
/// Linux build).
#[tauri::command]
fn open_claude_desktop() -> bool {
    platform::open_claude_desktop()
}

/// The file behind a live diff, if it may be handed to the editor: an existing
/// regular file given by its full path. Anything else — a relative path, a
/// folder, a path `code` could read as an option — goes no further.
fn diff_file(path: &str) -> Option<&std::path::Path> {
    let p = std::path::Path::new(path);
    (p.is_absolute() && p.is_file()).then_some(p)
}

/// The diff card's ↗: opens the edited file in VS Code when `code` is on PATH,
/// otherwise shows its folder. The file itself is never opened by its type —
/// xdg-open or Explorer would run a script that Claude just wrote.
#[tauri::command]
fn open_file_in_vscode(path: String) -> bool {
    let Some(file) = diff_file(&path) else { return false };
    if let Some(code) = platform::find_on_path("code") {
        let mut cmd = Command::new(code);
        cmd.arg(file);
        if platform::no_console(&mut cmd).spawn().is_ok() {
            return true;
        }
    }
    if let Some(folder) = file.parent().filter(|d| d.is_dir()) {
        platform::reveal_folder(&folder.to_string_lossy());
    }
    false
}

#[tauri::command]
fn quit_app(app: AppHandle) {
    app.exit(0);
}

/// Tray → Pause. Paused means paused: the pollers stop talking to the network,
/// not just the island stopping showing things.
#[tauri::command]
fn set_paused(app: AppHandle, paused: bool) {
    integrations::set_paused(paused);
    music::set_paused(&app, paused);
}

// ── Claude Code hooks ─────────────────────────────────────────────────────────

#[tauri::command]
fn hooks_status() -> HookStatus {
    hooks::status()
}

/// Pill ID → whether that agent's hooks reach Coucou. Read-only.
#[tauri::command]
fn agent_hooks_status() -> std::collections::HashMap<String, bool> {
    agent_hooks::status()
}

/// Returns the diff the user has to look at before anything is written.
#[tauri::command]
fn hooks_preview(install: bool) -> Result<HookPreview, String> {
    hooks::preview(install)
}

/// Only ever called from an explicit click in the settings window.
#[tauri::command]
fn hooks_apply(
    app: AppHandle,
    shared: State<Shared>,
    install: bool,
    fingerprint: String,
) -> Result<String, String> {
    // The fingerprint comes from the preview the user actually looked at, so a
    // settings.json that changed in between is refused rather than overwritten.
    let backup = hooks::write(install, &fingerprint)?;
    let updated = {
        let mut current = shared.settings.lock().unwrap();
        current.hooks_installed = install;
        if let Err(err) = settings::save(&current) {
            log::line(format!("could not save settings: {err}"));
        }
        current.clone()
    };
    let _ = app.emit("settings-changed", updated);
    Ok(backup)
}

// ── Other agents' hooks and plugins ──────────────────────────────────────────

#[tauri::command]
fn agent_hooks_list() -> Vec<agents::AgentStatus> {
    agents::list()
}

/// The diff the user has to look at before anything is written.
#[tauri::command]
fn agent_hooks_preview(agent: String, install: bool) -> Result<config_file::Plan, String> {
    agents::preview(&agent, install)
}

/// Only ever called from an explicit click in the settings window, with the
/// fingerprint of the preview the user looked at.
#[tauri::command]
fn agent_hooks_apply(agent: String, install: bool, fingerprint: String) -> Result<String, String> {
    let backups = agents::apply(&agent, install, &fingerprint)?;
    let done = if install { "installed" } else { "removed" };
    log::line(format!("agent hooks {done} for {agent}"));
    Ok(backups)
}

// ── Plan usage ────────────────────────────────────────────────────────────────

/// The diff of putting the plan usage relay into (or taking it out of) the
/// status line, before anything is written.
#[tauri::command]
fn status_line_preview(install: bool) -> Result<HookPreview, String> {
    hooks::status_line_preview(install)
}

/// Only ever called from an explicit click in the settings window.
#[tauri::command]
fn status_line_apply(
    app: AppHandle,
    shared: State<Shared>,
    install: bool,
    fingerprint: String,
) -> Result<String, String> {
    let backup = hooks::status_line_write(install, &fingerprint)?;
    let updated = {
        let mut current = shared.settings.lock().unwrap();
        current.plan_relay_installed = install;
        // As on the Mac: taking the relay out turns the pill off with it.
        if !install {
            current.show_plan_in_notch = false;
        }
        if let Err(err) = settings::save(&current) {
            log::line(format!("could not save settings: {err}"));
        }
        current.clone()
    };
    let _ = app.emit("settings-changed", updated);
    Ok(backup)
}

/// Codex plan usage, asked of the Codex CLI (`codex app-server`) when its pill
/// shows. Off the main thread: it can take a few seconds.
#[tauri::command]
async fn codex_plan_usage() -> Option<serde_json::Value> {
    tauri::async_runtime::spawn_blocking(codex_plan::read).await.ok().flatten()
}

#[tauri::command]
fn approval_decision(app: AppHandle, request_id: String, decision: String) {
    recap::record_decision(&app, &request_id, &decision);
    pipe::answer(&app, &request_id, &decision);
}

/// An option picked on the island for a question Claude Code asked.
#[tauri::command]
fn approval_answer(
    app: AppHandle,
    request_id: String,
    answers: std::collections::HashMap<String, serde_json::Value>,
) {
    // An answered question is not an Allow / Deny: nothing for the recap.
    recap::forget_request(&app, &request_id);
    pipe::answer_question(&app, &request_id, serde_json::to_value(answers).unwrap_or_default());
}

/// The island has the card on screen, so the long wait for a human may begin.
/// Until this arrives the relay only waits a few hundred milliseconds, which is
/// what stops a paused or unresponsive island from freezing Claude Code.
#[tauri::command]
fn approval_ack(app: AppHandle, request_id: String) {
    pipe::acknowledge(&app, &request_id);
}

/// Nobody can act on this request — the island is paused, or another card is
/// already up. Claude Code falls back to asking in the terminal immediately.
#[tauri::command]
fn approval_decline(app: AppHandle, request_id: String) {
    recap::forget_request(&app, &request_id);
    pipe::decline(&app, &request_id);
}

/// The human answered an AskUserQuestion card. `answers` maps each question's
/// text to the chosen label, or to a list of labels for a multi-select question.
/// "Reply in terminal" is `approval_decline`: no answer, the terminal asks.
#[tauri::command]
fn question_answer(app: AppHandle, request_id: String, answers: serde_json::Value) {
    pipe::answer_question(&app, &request_id, answers);
}

// ── Chat, files and secrets ───────────────────────────────────────────────────

/// One chat turn with the provider picked in the chat view. API keys and any
/// file bytes stay on the Rust side.
#[tauri::command]
async fn chat_send(
    app: AppHandle,
    shared: State<'_, Shared>,
    chat: State<'_, Chat>,
    query: String,
    context: Option<ChatContext>,
) -> Result<ChatReply, String> {
    let settings = shared.settings.lock().unwrap().clone();
    chat::send(&app, &chat, &settings, query, context).await
}

/// The models a provider offers, for the picker in the chat view. Only asked
/// once the user picked that provider, and only with its key or address.
#[tauri::command]
async fn chat_models(shared: State<'_, Shared>, provider: String) -> Result<Vec<ModelInfo>, String> {
    let settings = shared.settings.lock().unwrap().clone();
    chat::models(&settings, &provider).await
}

/// Settings → Local models → Connect: does the server answer, and with which models?
#[tauri::command]
async fn local_connect(provider: String, url: String) -> Result<local_chat::Connected, String> {
    local_chat::connect(&provider, &url).await
}

/// Stores the custom server's key for the address typed next to it; it is only
/// ever sent to that address.
#[tauri::command]
fn local_set_key(url: String, key: String) -> Result<(), String> {
    local_chat::set_custom_key(&url, &key)
}

/// An audio file the user chose for a sound, as base64 (the page decodes it). Only audio
/// files, and nothing large: it is a short sound, not a library.
#[tauri::command]
fn read_sound(path: String) -> Result<String, String> {
    const MAX: u64 = 6 * 1024 * 1024;
    let p = std::path::Path::new(&path);
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if !["wav", "ogg", "oga", "mp3", "flac", "opus", "m4a", "aac"].contains(&ext.as_str()) {
        return Err("Choose an audio file (wav, ogg, mp3, flac, opus or m4a)".into());
    }
    let meta = std::fs::metadata(p).map_err(|_| "That file cannot be read".to_string())?;
    if !meta.is_file() || meta.len() > MAX {
        return Err("The sound must be a file under 6 MB".into());
    }
    let bytes = std::fs::read(p).map_err(|e| e.to_string())?;
    Ok(claude::base64_for(&bytes))
}

// ── Hub: the calendar and Jinx's pending tasks (see calendar.rs) ───────────────

#[tauri::command]
async fn calendar_events(days: u32) -> Result<Vec<calendar::Event>, String> {
    tauri::async_runtime::spawn_blocking(move || calendar::events(days.clamp(1, 31)))
        .await
        .map_err(|e| e.to_string())?
}

/// Jinx's open pending tasks, read from her state file on the server (needs the server set).
#[tauri::command]
async fn jinx_tasks(shared: State<'_, Shared>) -> Result<Vec<calendar::Task>, String> {
    let host = {
        let s = shared.settings.lock().unwrap();
        s.server_host.trim().to_string()
    };
    if host.is_empty() {
        return Err("not-configured".into());
    }
    tauri::async_runtime::spawn_blocking(move || sysmon::remote_cat(&host, ".hermes/state/pendientes.json").map(|t| calendar::parse_tasks(&t)))
        .await
        .map_err(|e| e.to_string())?
}

// ── Hub: notes, and what Jinx can see of the Hub (see notes.rs, share.rs) ──────

#[tauri::command]
fn notes_load() -> String {
    hub_notes::load()
}

/// Where the Hub's data goes for Jinx: the server, if one is set and sharing is on.
fn share_target(shared: &State<Shared>) -> Option<String> {
    let s = shared.settings.lock().unwrap();
    (s.jinx_share && !s.server_host.trim().is_empty()).then(|| s.server_host.trim().to_string())
}

#[tauri::command]
fn notes_save(shared: State<Shared>, text: String) -> Result<(), String> {
    hub_notes::save(&text)?;
    if let Some(host) = share_target(&shared) {
        share::schedule(&host, "notes.md", text);
    }
    Ok(())
}

/// A snapshot for Jinx (calendar, timer…). Only the files the Hub makes.
#[tauri::command]
fn share_push(shared: State<Shared>, name: String, content: String) -> Result<(), String> {
    if !["calendar.json", "pomodoro.json", "readme.md"].contains(&name.as_str()) || content.len() > 200_000 {
        return Err("not a Hub file".into());
    }
    if let Some(host) = share_target(&shared) {
        share::schedule(&host, &name, content);
    }
    Ok(())
}

#[tauri::command]
fn share_status() -> share::Status {
    share::status()
}

// ── Hub: quick switches (see quick.rs) ──────────────────────────────────────────

#[tauri::command]
async fn quick_state() -> Result<quick::Quick, String> {
    tauri::async_runtime::spawn_blocking(quick::read).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn quick_set(what: String, value: i64) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || quick::set(&what, value)).await.map_err(|e| e.to_string())?
}

// ── Hub: the weather (see weather.rs) ──────────────────────────────────────────

/// "off" until the user turns the Weather tab's network use on in Settings.
#[tauri::command]
async fn weather_get(shared: State<'_, Shared>, city: String) -> Result<weather::Weather, String> {
    if !shared.settings.lock().unwrap().weather_on {
        return Err("off".into());
    }
    if city.trim().is_empty() {
        return Err("no-city".into());
    }
    weather::fetch(&city).await
}

// ── Hub: uploading a shelf file (see drive.rs), directly, no model involved ─────

/// `dest`: "school" (the school's OneDrive, through Jinx's upload function on the server) or
/// "drive" (the personal Google Drive, through rclone).
#[tauri::command]
async fn shelf_upload(shared: State<'_, Shared>, path: String, dest: String, folder: String) -> Result<drive::Uploaded, String> {
    let file = files::inside(&files::inbox_dir(), &path)?;
    let name = file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
    let (host, remote, account) = {
        let s = shared.settings.lock().unwrap();
        (s.server_host.trim().to_string(), s.drive_remote.trim().to_string(), s.drive_account.trim().to_string())
    };
    tauri::async_runtime::spawn_blocking(move || match dest.as_str() {
        "school" if host.is_empty() => Err("Set the server in Settings → Hub: the school's OneDrive goes through Jinx".to_string()),
        "school" => drive::school_upload(&host, &file, &name, &folder),
        // GNOME's drive when the account is signed in there (what the file manager mounts), else rclone.
        "drive" => match drive::gnome_root(&account) {
            Some(_) => drive::gnome_upload(&account, &folder, &file, &name),
            None if drive::drive_state(&remote, None).connected => drive::drive_upload(&remote, &folder, &file, &name, None),
            None => Err("No Google account: add it in Settings → Online Accounts (or connect rclone)".to_string()),
        },
        _ => Err("unknown destination".to_string()),
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn drive_state(shared: State<'_, Shared>) -> Result<drive::DriveState, String> {
    let (remote, account) = {
        let s = shared.settings.lock().unwrap();
        (s.drive_remote.trim().to_string(), s.drive_account.trim().to_string())
    };
    tauri::async_runtime::spawn_blocking(move || {
        let mut st = drive::drive_state(&remote, None);
        if drive::gnome_root(&account).is_some() {
            st.installed = true;
            st.connected = true;
        }
        st
    })
    .await
    .map_err(|e| e.to_string())
}

/// The one-time Google sign-in for Drive: rclone opens the browser and waits.
#[tauri::command]
async fn drive_connect(shared: State<'_, Shared>) -> Result<(), String> {
    let remote = shared.settings.lock().unwrap().drive_remote.trim().to_string();
    tauri::async_runtime::spawn_blocking(move || drive::drive_connect(&remote, None)).await.map_err(|e| e.to_string())?
}

static SCHOOL_LOGIN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Starts Jinx's Microsoft sign-in for the school account; what it prints (the page and the code)
/// arrives as `school-login` events, and a last one says whether it worked.
#[tauri::command]
fn school_login_start(app: AppHandle, shared: State<Shared>) -> Result<(), String> {
    let host = shared.settings.lock().unwrap().server_host.trim().to_string();
    if host.is_empty() {
        return Err("Set the server in Settings → Hub first".into());
    }
    if SCHOOL_LOGIN.swap(true, Ordering::SeqCst) {
        return Err("A sign-in is already running".into());
    }
    std::thread::spawn(move || {
        let emit = |payload: serde_json::Value| {
            let _ = app.emit_to(island::WINDOW_LABEL, "school-login", payload);
        };
        let result = drive::school_login(&host, |line| emit(serde_json::json!({ "kind": "line", "text": line })));
        emit(serde_json::json!({ "kind": "done", "ok": result.is_ok(), "text": result.err().unwrap_or_default() }));
        SCHOOL_LOGIN.store(false, Ordering::SeqCst);
    });
    Ok(())
}

// ── Hub: the file shelf (the inbox of dropped files; see files.rs) ─────────────

#[tauri::command]
fn shelf_list() -> Vec<files::ShelfItem> {
    files::shelf()
}

#[tauri::command]
fn shelf_remove(path: String) -> Result<(), String> {
    files::remove(&path)
}

/// Opens a shelf file with the default application, or the shelf's folder.
#[tauri::command]
fn shelf_open(path: Option<String>) -> Result<(), String> {
    let target = match path {
        Some(p) => files::inside(&files::inbox_dir(), &p)?,
        None => files::inbox_dir(),
    };
    platform::reveal_folder(&target.to_string_lossy());
    Ok(())
}

// ── Hub: tasks and reminders (see tasks.rs), and their link with Jinx (see pendientes.rs) ──

#[tauri::command]
fn tasks_list() -> Vec<tasks::Task> {
    tasks::load()
}

/// Adds a task; `text` may carry its own time ("llamar a mamá a las 17:30", "stretch in 20 min").
/// A day picked on the calendar wins over the sentence's day, keeping its time of day
/// ("viernes a las 17:30" → Friday 17:30; a bare Friday → `hour`:00).
/// With Jinx shared (a server set, sharing on) the task is also hers: her list gets it at the sync.
#[tauri::command]
fn tasks_add(shared: State<Shared>, text: String, day: Option<i64>, hour: i64) -> Result<Vec<tasks::Task>, String> {
    let now = tasks::now();
    let (title, when) = tasks::parse_when(&text, now, &pomodoro::local_offset);
    let at = day.map(|d| tasks::with_day(d, when, hour, &pomodoro::local_offset)).or(when);
    let to_jinx = share_target(&shared).is_some();
    tasks::update(|l| {
        tasks::add(l, &title, at, now)?;
        if let Some(t) = l.last_mut() {
            t.jinx = to_jinx;
        }
        Ok(())
    })
}

#[tauri::command]
fn tasks_done(id: String, done: bool) -> Result<Vec<tasks::Task>, String> {
    tasks::update(|l| tasks::set_done(l, &id, done, tasks::now()))
}

/// Moves a reminder to `until` (unix seconds).
#[tauri::command]
fn tasks_snooze(id: String, until: i64) -> Result<Vec<tasks::Task>, String> {
    tasks::update(|l| tasks::snooze(l, &id, until))
}

#[tauri::command]
fn tasks_notified(id: String) -> Result<Vec<tasks::Task>, String> {
    tasks::update(|l| tasks::mark_notified(l, &id))
}

/// Deleting a shared task dismisses her copy too (if she cannot be reached it stays on her side, and
/// shows up in the Tasks tab among what she has, where it can be dismissed).
#[tauri::command]
fn tasks_delete(shared: State<Shared>, id: String) -> Result<Vec<tasks::Task>, String> {
    let jinx_id = tasks::load().into_iter().find(|t| t.id == id).and_then(|t| t.jinx_id);
    let list = tasks::update(|l| tasks::delete(l, &id))?;
    if let (Some(jid), Some(host)) = (jinx_id, share_target(&shared)) {
        std::thread::spawn(move || {
            let _ = pendientes::resolve(&host, &jid, "descartado");
        });
    }
    Ok(list)
}

#[tauri::command]
fn tasks_clear_done() -> Result<Vec<tasks::Task>, String> {
    tasks::update(|l| {
        tasks::clear_done(l);
        Ok(())
    })
}

/// Shares one task with Jinx, or keeps it to Coucou.
#[tauri::command]
fn tasks_set_jinx(id: String, on: bool) -> Result<Vec<tasks::Task>, String> {
    tasks::update(|l| tasks::set_jinx(l, &id, on))
}

/// One round with Jinx's pendientes: hands over what is hers to have, follows what was closed on
/// either side, and returns the pendientes she holds that are not ours. Without a server it is
/// just the local list.
#[tauri::command]
async fn tasks_sync(shared: State<'_, Shared>) -> Result<pendientes::SyncResult, String> {
    let Some(host) = share_target(&shared) else {
        return Ok(pendientes::SyncResult { tasks: tasks::load(), ..Default::default() });
    };
    tauri::async_runtime::spawn_blocking(move || pendientes::sync(&host)).await.map_err(|e| e.to_string())
}

/// Closes or dismisses one of Jinx's own pendientes ("hecho" | "descartado").
#[tauri::command]
async fn jinx_resolve(shared: State<'_, Shared>, id: String, action: String) -> Result<(), String> {
    let host = share_target(&shared).ok_or("Jinx is not shared: set the server in Settings → Hub")?;
    tauri::async_runtime::spawn_blocking(move || pendientes::resolve(&host, &id, &action)).await.map_err(|e| e.to_string())?
}

// ── The island's quiet details: battery, mains power, internet (see sysstate.rs) ──

/// What is known now; null where the system cannot say (not Linux).
#[tauri::command]
fn sys_state() -> serde_json::Value {
    #[cfg(target_os = "linux")]
    {
        serde_json::to_value(sysstate::current()).unwrap_or(serde_json::Value::Null)
    }
    #[cfg(not(target_os = "linux"))]
    {
        serde_json::Value::Null
    }
}

// ── Hub: clipboard history (see clipboard.rs; off until turned on in Settings) ──

#[tauri::command]
fn clipboard_list() -> Vec<clipboard::Entry> {
    clipboard::list()
}

#[tauri::command]
fn clipboard_pin(id: u64, pinned: bool) -> Result<Vec<clipboard::Entry>, String> {
    clipboard::pin(id, pinned)
}

#[tauri::command]
fn clipboard_delete(id: u64) -> Vec<clipboard::Entry> {
    clipboard::delete(id)
}

#[tauri::command]
fn clipboard_clear() -> Vec<clipboard::Entry> {
    clipboard::clear()
}

#[tauri::command]
fn clipboard_copy(app: AppHandle, id: u64) -> Result<(), String> {
    clipboard::copy_back(&app, id)
}

/// The batteries of paired and plugged devices (see peripherals.rs); empty where there are none.
#[tauri::command]
fn peripherals_list() -> serde_json::Value {
    #[cfg(target_os = "linux")]
    {
        serde_json::to_value(peripherals::current()).unwrap_or(serde_json::Value::Null)
    }
    #[cfg(not(target_os = "linux"))]
    {
        serde_json::json!([])
    }
}

// ── Hub: workspaces (see workspaces.rs) ───────────────────────────────────────

#[tauri::command]
fn workspaces_list() -> Vec<workspaces::Workspace> {
    workspaces::load()
}

#[tauri::command]
fn workspaces_save(list: Vec<workspaces::Workspace>) -> Result<Vec<workspaces::Workspace>, String> {
    workspaces::save(list)
}

/// The installed applications, to choose from. Reads a few hundred small files: off the UI thread.
#[tauri::command]
async fn workspaces_apps() -> Result<Vec<workspaces::App>, String> {
    tauri::async_runtime::spawn_blocking(workspaces::list_apps).await.map_err(|e| e.to_string())
}

/// Opens a workspace; what could not be opened comes back (empty: everything went).
#[tauri::command]
async fn workspaces_launch(id: String) -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(move || workspaces::launch(&id)).await.map_err(|e| e.to_string())?
}

// ── Hub: Pomodoro log and statistics (see pomodoro.rs) ─────────────────────────

#[tauri::command]
fn pomodoro_log(kind: String, seconds: u32, completed: bool) -> Result<(), String> {
    pomodoro::log(&kind, seconds, completed)
}

#[tauri::command]
fn pomodoro_stats() -> pomodoro::Stats {
    pomodoro::stats()
}

// ── Hub: system and server vitals (see sysmon.rs) ─────────────────────────────

#[tauri::command]
async fn sys_sample() -> Result<sysmon::Vitals, String> {
    tauri::async_runtime::spawn_blocking(sysmon::local).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn server_sample(shared: State<'_, Shared>) -> Result<sysmon::Vitals, String> {
    let (host, services) = {
        let s = shared.settings.lock().unwrap();
        (s.server_host.clone(), s.server_services.clone())
    };
    if host.trim().is_empty() {
        return Err("not-configured".into());
    }
    tauri::async_runtime::spawn_blocking(move || sysmon::remote(host.trim(), &services))
        .await
        .map_err(|e| e.to_string())?
}

/// What the server is busy with (see activities.rs). Off unless turned on; "not-configured" when
/// there is no server, so the page can stay quiet.
#[tauri::command]
async fn activities_fetch(shared: State<'_, Shared>) -> Result<Vec<activities::Activity>, String> {
    let (host, on) = {
        let s = shared.settings.lock().unwrap();
        (s.server_host.trim().to_string(), s.activities_on)
    };
    if !on || host.is_empty() {
        return Err("not-configured".into());
    }
    tauri::async_runtime::spawn_blocking(move || activities::fetch(&host)).await.map_err(|e| e.to_string())?
}

// ── Jinx (Hermes) ─────────────────────────────────────────────────────────────

/// Starts a run; the reply, tool calls and approval requests arrive as `jinx` events.
#[tauri::command]
async fn jinx_send(
    app: AppHandle,
    jinx: State<'_, jinx::Jinx>,
    shared: State<'_, Shared>,
    text: String,
    context: Option<ChatContext>,
) -> Result<(), String> {
    let (hub_shared, backend) = {
        let s = shared.settings.lock().unwrap();
        (s.jinx_share && !s.server_host.trim().is_empty(), s.jinx_backend.clone())
    };
    // As on the phones: through the laptop's gateway, into the thread shared with them and Telegram.
    if jinx_gateway::use_gateway(&backend) {
        let input = jinx::with_context(&text, context.as_ref())?;
        jinx_gateway::send(app, input);
        return Ok(());
    }
    jinx::send(app, &jinx, text, context, hub_shared).await
}

/// A coding agent's news for Jinx to pass on (see jinx_gateway::notify). Nothing when the gateway is not used.
#[tauri::command]
async fn jinx_notify(shared: State<'_, Shared>, text: String, kind: String) -> Result<bool, String> {
    let backend = shared.settings.lock().unwrap().jinx_backend.clone();
    if !jinx_gateway::use_gateway(&backend) {
        return Ok(false);
    }
    tauri::async_runtime::spawn_blocking(move || jinx_gateway::notify(&text, &kind))
        .await
        .map_err(|e| e.to_string())?
}

/// The tail of Jinx's shared thread (Telegram, the phones and this PC), when the gateway is used.
#[tauri::command]
async fn jinx_history(shared: State<'_, Shared>, n: u32) -> Result<Option<Vec<serde_json::Value>>, String> {
    let backend = shared.settings.lock().unwrap().jinx_backend.clone();
    if !jinx_gateway::use_gateway(&backend) {
        return Ok(None);
    }
    tauri::async_runtime::spawn_blocking(move || jinx_gateway::history(n).map(Some))
        .await
        .map_err(|e| e.to_string())?
}

/// `choice`: once | session | always | deny. Through the gateway when it is used (the question is on the
/// server), else straight to the Hermes API.
#[tauri::command]
async fn jinx_approve(shared: State<'_, Shared>, run_id: String, request_id: String, choice: String) -> Result<(), String> {
    let backend = shared.settings.lock().unwrap().jinx_backend.clone();
    if jinx_gateway::use_gateway(&backend) {
        return tauri::async_runtime::spawn_blocking(move || jinx_gateway::approve(&run_id, &request_id, &choice))
            .await
            .map_err(|e| e.to_string())?;
    }
    jinx::approve(&run_id, &request_id, &choice).await
}

/// What Jinx waits for Alan to allow, when she is reached through the gateway (the Hermes API streams its
/// questions as `jinx` events instead: empty then).
#[tauri::command]
async fn jinx_approvals(shared: State<'_, Shared>) -> Result<Vec<serde_json::Value>, String> {
    let backend = shared.settings.lock().unwrap().jinx_backend.clone();
    if !jinx_gateway::use_gateway(&backend) {
        return Ok(Vec::new());
    }
    tauri::async_runtime::spawn_blocking(jinx_gateway::approvals)
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn jinx_stop(jinx: State<'_, jinx::Jinx>) -> Result<(), String> {
    jinx::stop(&jinx).await
}

/// Reachable, and is the key accepted?
#[tauri::command]
async fn jinx_test(shared: State<'_, Shared>) -> Result<String, String> {
    let backend = shared.settings.lock().unwrap().jinx_backend.clone();
    if jinx_gateway::use_gateway(&backend) {
        return tauri::async_runtime::spawn_blocking(jinx_gateway::test).await.map_err(|e| e.to_string())?;
    }
    jinx::test().await
}

// ── Claude Code and OpenCode sessions (see agents.rs, opencode.rs) ────────────

#[tauri::command]
async fn agents_list() -> Result<serde_json::Value, String> {
    // Both CLIs are asked at once: each can take a second, and the chat polls
    // every couple of seconds. One missing is fine; none at all is an error.
    let claude = tauri::async_runtime::spawn_blocking(claude_sessions::list);
    let open = tauri::async_runtime::spawn_blocking(opencode::list);
    let (claude, open) = (claude.await, open.await);
    let mut all: Vec<serde_json::Value> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    match claude.map_err(|e| e.to_string())? {
        Ok(v) => all.extend(v.as_array().cloned().unwrap_or_default()),
        Err(e) => errors.push(e),
    }
    match open.map_err(|e| e.to_string())? {
        Ok(v) => all.extend(v.as_array().cloned().unwrap_or_default()),
        Err(e) => errors.push(e),
    }
    if all.is_empty() && !errors.is_empty() {
        return Err(errors.join("; "));
    }
    Ok(serde_json::Value::Array(all))
}

/// The 5-hour / 7-day usage percentages, when the statusline has written them.
#[tauri::command]
fn rate_limits() -> Option<serde_json::Value> {
    claude_sessions::limits()
}

#[tauri::command]
async fn agent_messages(session_id: String, offset: u64) -> Result<claude_sessions::Messages, String> {
    if opencode::is_opencode(&session_id) {
        return tauri::async_runtime::spawn_blocking(move || opencode::messages(&session_id, offset))
            .await
            .map_err(|e| e.to_string())?;
    }
    tauri::async_runtime::spawn_blocking(move || claude_sessions::messages(&session_id, offset))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn agent_send(id: String, text: String) -> Result<(), String> {
    if opencode::is_opencode(&id) {
        return tauri::async_runtime::spawn_blocking(move || opencode::send(&id, &text))
            .await
            .map_err(|e| e.to_string())?;
    }
    tauri::async_runtime::spawn_blocking(move || claude_sessions::send(&id, &text)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn agent_start(cwd: String, prompt: String, backend: String) -> Result<String, String> {
    if backend == "opencode" {
        return tauri::async_runtime::spawn_blocking(move || opencode::start(&cwd, &prompt))
            .await
            .map_err(|e| e.to_string())?;
    }
    tauri::async_runtime::spawn_blocking(move || claude_sessions::start(&cwd, &prompt)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn agent_stop(id: String) -> Result<(), String> {
    if opencode::is_opencode(&id) {
        return Err("OpenCode sessions run to the end on their own — they cannot be stopped from here".into());
    }
    tauri::async_runtime::spawn_blocking(move || claude_sessions::stop(&id)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn chat_reset(chat: State<Chat>) {
    chat.reset();
}

/// Copies a dropped file into the inbox and reports its name back.
#[tauri::command]
fn ingest_file(path: String) -> Result<DroppedFile, String> {
    files::ingest(&path)
}

/// The island may only ask whether a key exists — never read it.
#[tauri::command]
fn secret_present(key: String) -> bool {
    secrets::present(&key)
}

#[tauri::command]
fn secret_set(app: AppHandle, key: String, value: String) -> Result<(), String> {
    // Bound to its server's address: only local_set_key may store it.
    if key == local_chat::CUSTOM_KEY {
        return Err("use local_set_key".into());
    }
    let before = (key == "github-token").then(|| secrets::get(&key));
    secrets::set(&key, &value)?;
    if let Some(before) = before {
        if secrets::get(&key) != before {
            integrations::github_token_changed(&app);
        }
    }
    Ok(())
}

#[tauri::command]
fn secret_clear(app: AppHandle, key: String) -> Result<(), String> {
    let had = key == "github-token" && secrets::present(&key);
    secrets::clear(&key)?;
    if had {
        integrations::github_token_changed(&app);
    }
    Ok(())
}

/// Opens the configured n8n instance — the URL lives in the Credential Manager.
#[tauri::command]
fn open_n8n() {
    if let Some(url) = secrets::get("n8n-url") {
        open_url(url);
    }
}

/// Refresh buttons in the integration cards.
#[tauri::command]
async fn refresh_integration(app: AppHandle, id: String) {
    integrations::poll_once(app, &id).await;
}

/// The GitHub card was opened: refetch its `section` ("pulse" or "activity")
/// if it is stale. The pollers still decline while the pill is off or paused.
#[tauri::command]
fn github_refresh(section: String) {
    integrations::github_refresh_if_stale(&section);
}

/// Lets the island write to the same log as the Rust side.
#[tauri::command]
fn log_line(message: String) {
    log::line(format!("ui  {message}"));
}

// ── Global shortcuts ──────────────────────────────────────────────────────────

/// How each global shortcut went: registered, taken by another app, and so on.
#[tauri::command]
fn shortcuts_status(app: AppHandle) -> shortcuts::Report {
    shortcuts::status(&app)
}

/// Settings is recording a new combination: let go of ours meanwhile, so the
/// keys reach the recorder instead of running an action. `false` takes them back.
#[tauri::command]
fn shortcuts_suspend(app: AppHandle, shared: State<Shared>, suspended: bool) {
    if suspended {
        shortcuts::suspend(&app);
    } else {
        let stored = shared.settings.lock().unwrap().shortcuts.clone();
        shortcuts::apply(&app, &stored);
    }
}

// ── Settings window ───────────────────────────────────────────────────────────

/// WebView2 allows exactly one browser environment per app, and its options are
/// fixed by whichever webview is created first. Every window must therefore ask
/// for the *same* arguments as the island (see `additionalBrowserArgs` in
/// tauri.conf.json) — a mismatch makes the second window come up blank, with no
/// error anywhere.
pub(crate) const BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --autoplay-policy=no-user-gesture-required";

/// In a dev build the pages are served by Vite, so the second window needs the
/// absolute dev URL; a bundled build resolves it inside the app bundle.
fn settings_page_url(app: &AppHandle) -> WebviewUrl {
    #[cfg(dev)]
    if let Some(mut base) = app.config().build.dev_url.clone() {
        base.set_path("/settings.html");
        return WebviewUrl::External(base);
    }
    let _ = app;
    WebviewUrl::App("settings.html".into())
}

/// The settings window is created hidden at launch and only ever shown and
/// hidden afterwards. A WebView2 window created later — on the main thread or
/// not — silently comes up blank in this app, so the window that works is the
/// one that exists before the island's webview does.
fn create_settings_window(app: &AppHandle) {
    let url = settings_page_url(app);
    let builder = WebviewWindowBuilder::new(app, "settings", url)
        .additional_browser_args(BROWSER_ARGS)
        .title(i18n::t("Settings — Coucou"))
        .inner_size(560.0, 680.0)
        .min_inner_size(460.0, 480.0)
        .resizable(true)
        .visible(false)
        .center();
    // On Linux GNOME lists any ordinary window as a running application (Super,
    // the dock). Coucou lives in the notch and the tray, so neither window should
    // appear there; Settings is reopened from the island's gear or the tray.
    #[cfg(target_os = "linux")]
    let builder = builder.skip_taskbar(true);
    match builder.build() {
        Ok(win) => {
            // Closing it must only hide it, or it could never be reopened.
            let hidden = win.clone();
            win.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = hidden.hide();
                }
            });
        }
        Err(err) => log::line(format!("settings window failed: {err}")),
    }
}

pub fn show_settings_window(app: &AppHandle) {
    let Some(win) = app.get_webview_window("settings") else {
        log::line("settings window missing");
        return;
    };
    let _ = win.unminimize();
    let _ = win.show();
    let _ = win.set_focus();
}

#[tauri::command]
fn open_settings_window(app: AppHandle) {
    show_settings_window(&app);
}

pub fn run() {
    platform::prepare_environment();
    let loaded = settings::load();
    i18n::set_picked(&loaded.language);
    let gate = Arc::new(PollGate::new());

    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // `coucou --shortcut <action>`: what a desktop's own keyboard
            // settings run where we can't listen for keys ourselves (Wayland).
            match shortcuts::from_args(&argv) {
                Some(action) => shortcuts::dispatch(app, action),
                None => {
                    // `coucou --toggle` (bound to a keyboard shortcut) opens the island, or closes it if it is open.
                    let what = if argv.iter().any(|a| a == "--toggle") { "toggle" } else { "open" };
                    let _ = app.emit_to(island::WINDOW_LABEL, "tray", what.to_string());
                }
            }
        }))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None));
    // Where no global shortcut can work, the plugin isn't even started.
    if platform::global_shortcuts_blocked().is_none() {
        builder = builder.plugin(shortcuts::plugin());
    }

    builder
        .manage(Shared {
            settings: Mutex::new(loaded.clone()),
            gate: gate.clone(),
        })
        .manage(Pending::default())
        .manage(Chat::default())
        .manage(jinx::Jinx::default())
        .manage(shortcuts::Registry::default())
        .manage(recap::load())
        .invoke_handler(tauri::generate_handler![
            boot,
            presence_state,
            presence_local_music,
            presence_local_news,
            presence_local_limit,
            save_settings,
            set_system_languages,
            set_collapsed,
            set_island_rect,
            focus_window,
            reposition,
            list_monitors,
            open_url,
            open_in_vscode,
            open_session,
            open_claude_desktop,
            open_file_in_vscode,
            quit_app,
            hooks_status,
            agent_hooks_status,
            hooks_preview,
            hooks_apply,
            agent_hooks_list,
            agent_hooks_preview,
            agent_hooks_apply,
            status_line_preview,
            status_line_apply,
            codex_plan_usage,
            approval_decision,
            approval_answer,
            approval_ack,
            approval_decline,
            question_answer,
            music::music_state,
            music::music_control,
            music::music_art,
            music::music_lyrics,
            bars::music_bars,
            log_line,
            chat_send,
            chat_models,
            local_connect,
            local_set_key,
            chat_reset,
            read_sound,
            calendar_events,
            jinx_tasks,
            notes_load,
            notes_save,
            share_push,
            share_status,
            quick_state,
            quick_set,
            weather_get,
            shelf_upload,
            drive_state,
            drive_connect,
            school_login_start,
            shelf_list,
            shelf_remove,
            shelf_open,
            pomodoro_log,
            pomodoro_stats,
            tasks_list,
            tasks_add,
            tasks_done,
            tasks_snooze,
            tasks_notified,
            tasks_delete,
            tasks_clear_done,
            sys_state,
            peripherals_list,
            activities_fetch,
            clipboard_list,
            clipboard_pin,
            clipboard_delete,
            clipboard_clear,
            clipboard_copy,
            tasks_set_jinx,
            tasks_sync,
            jinx_resolve,
            workspaces_list,
            workspaces_save,
            workspaces_apps,
            workspaces_launch,
            sys_sample,
            server_sample,
            agents_list,
            rate_limits,
            agent_messages,
            agent_send,
            agent_start,
            agent_stop,
            jinx_send,
            jinx_history,
            jinx_notify,
            jinx_approve,
            jinx_approvals,
            jinx_stop,
            jinx_test,
            ingest_file,
            secret_present,
            secret_set,
            secret_clear,
            refresh_integration,
            github_refresh,
            open_n8n,
            open_settings_window,
            set_paused,
            shortcuts_status,
            shortcuts_suspend,
            phone_link::phone_link_status,
            phone_link::phone_link_set_enabled,
            phone_link::phone_link_pairing,
            phone_link::phone_link_new_pairing,
            phone_link::phone_link_publish,
            phone_link::phone_link_publish_question,
            phone_link::phone_link_publish_prefs,
            phone_link::phone_answers_status,
            phone_link::phone_answers_set_enabled,
            phone_link::phone_diffs_status,
            phone_link::phone_diffs_set_enabled,
            phone_link::phone_link_send_diff,
            phone_link::phone_usage_status,
            phone_link::phone_usage_set_enabled,
            phone_link::phone_relay_status,
            phone_link::phone_relay_set,
            phone_link::phone_relay_set_access,
            phone_link::phone_link_publish_usage,
            phone_link::phone_services_status,
            phone_link::phone_services_set,
            phone_link::phone_link_publish_services,
            phone_link::phone_details_status,
            phone_link::phone_details_set_enabled,
            phone_link::phone_chat_status,
            phone_link::phone_chat_set_enabled,
            phone_link::phone_chat_set_models,
            recap::recap_history,
            recap::recap_prefs,
            recap::recap_set_enabled,
            recap::recap_set_hide_projects,
            recap::recap_mark_shown,
            recap::recap_clear,
            recap::recap_save_png,
            recap::recap_reveal_saved,
            desktop::desktop_mochi_info,
            desktop::desktop_mochi_pick_up,
            desktop::desktop_mochi_carry,
            desktop::desktop_mochi_carry_end,
            desktop::desktop_mochi_drag_begin,
            desktop::desktop_mochi_drag_move,
            desktop::desktop_mochi_drag_end,
            desktop::desktop_mochi_fly_out,
            desktop::desktop_mochi_fly_home,
            desktop::desktop_mochi_set_asleep,
            sounds::custom_sounds,
            sounds::custom_sound,
            sounds::reveal_sounds_folder,
            sounds::reload_sounds,
            spotify::spotify_refresh,
            spotify::spotify_control,
            spotify::spotify_open,
            spotify::spotify_installed,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            app.manage(phone_link::PhoneLink::new(handle.clone()));
            tray::build(&handle)?;
            // Before the island: see create_settings_window.
            create_settings_window(&handle);
            // Same rule for Mochi's desktop window.
            desktop::setup(&handle);

            if let Some(win) = island::window(&handle) {
                // Where Tauri takes the drop itself (Linux), its paths are the
                // ones ingest_file may copy (files.rs).
                win.on_window_event(|event| {
                    if let tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) = event {
                        files::allow_dropped(paths.iter().map(|p| p.to_string_lossy().to_string()));
                    }
                });
                platform::make_non_activating(&win);
                #[cfg(windows)]
                webview_drop::install(&handle);
                island::apply_geometry(&handle, &loaded.screen, false);
                let _ = win.show();
            }
            gate.collapsed.store(false, Ordering::Relaxed);
            // Nothing drawn yet, so nothing takes the mouse until the page
            // reports the island's shape.
            if !platform::CURSOR_POLL {
                island::refresh_click_through(&handle, &gate);
            }
            gate.set_active(true);
            island::spawn_cursor_poll(handle.clone(), gate.clone());
            #[cfg(target_os = "linux")]
            island::spawn_gaze_poll(handle.clone(), gate.clone());

            log::line(format!("--- Coucou {} started ---", env!("CARGO_PKG_VERSION")));
            hooks::ensure_hook_exe(&handle);
            pipe::start(handle.clone());
            phone_link::start_if_enabled(&handle);
            integrations::start(handle.clone());
            music::sync_enabled(
                &handle,
                loaded.active_integrations.iter().any(|x| x == "integration_music"),
            );
            notifs::sync_enabled(&handle, loaded.notification_peek);
            remote::sync_enabled(&handle, loaded.remote_enabled);
            clipboard::sync_enabled(&handle, loaded.clipboard_history);
            // Which device is in use: Mochi and Coucou are drawn only there (rules shared with the phone).
            presence::start(&handle);
            // Handy (dictation) recording: the island says so. Event-driven, asleep otherwise.
            #[cfg(target_os = "linux")]
            dictation::start(&handle);
            // Battery, mains power and internet, announced by the system itself (no polling).
            #[cfg(target_os = "linux")]
            sysstate::start(&handle);
            // Bluetooth devices' and controllers' batteries, announced by UPower (no polling).
            #[cfg(target_os = "linux")]
            peripherals::start(&handle);
            spotify::sync(&handle, &loaded.active_integrations);
            shortcuts::apply(&handle, &loaded.shortcuts);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Coucou")
        .run(|app, event| {
            // Withdraw the phone link's announcement on the local network before the process ends.
            if let tauri::RunEvent::Exit = event {
                phone_link::stop_on_exit(app);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::diff_file;

    #[test]
    fn only_an_existing_file_by_its_full_path_reaches_the_editor() {
        let dir = std::env::temp_dir().join(format!("coucou-diff-file-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("edited.ts");
        std::fs::write(&file, "x").unwrap();

        assert!(diff_file(&file.to_string_lossy()).is_some());
        // A folder, a missing file, a relative path or an option never pass.
        assert!(diff_file(&dir.to_string_lossy()).is_none());
        assert!(diff_file(&dir.join("missing.ts").to_string_lossy()).is_none());
        assert!(diff_file("edited.ts").is_none());
        assert!(diff_file("--help").is_none());
        assert!(diff_file("").is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
