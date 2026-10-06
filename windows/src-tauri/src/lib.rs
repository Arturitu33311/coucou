// Coucou for Windows — app wiring and the commands the island calls.

mod agents;
mod bars;
mod calendar;
mod claude;
mod drive;
mod files;
mod hooks;
mod integrations;
mod island;
mod jinx;
mod log;
mod music;
mod notes;
mod notifs;
mod pipe;
mod pomodoro;
mod quick;
mod platform;
mod secrets;
mod settings;
mod share;
mod sysmon;
mod tray;
mod weather;

use std::process::Command;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_autostart::{ManagerExt, MacosLauncher};

use claude::{Chat, ChatContext, ChatReply};
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

#[tauri::command]
fn boot(app: AppHandle, shared: State<Shared>) -> BootInfo {
    let mut settings = shared.settings.lock().unwrap().clone();
    // The real state of ~/.claude/settings.json wins over whatever we stored.
    settings.hooks_installed = hooks::status().installed;
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
    let (screen_changed, autostart_changed) = {
        let mut current = shared.settings.lock().unwrap();
        let screen_changed = current.screen != settings.screen || current.island_offset_x != settings.island_offset_x;
        let autostart_changed = current.autostart != settings.autostart;
        *current = settings.clone();
        (screen_changed, autostart_changed)
    };
    if let Err(err) = settings::save(&settings) {
        eprintln!("[coucou] could not save settings: {err}");
    }
    // The Music pill's switch decides whether the players are watched at all.
    music::sync_enabled(&app, settings.active_integrations.iter().any(|x| x == "integration_music"));
    notifs::sync_enabled(&app, settings.notification_peek);
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
    // Keep the other window in step (island ⇄ settings window).
    let _ = app.emit("settings-changed", settings);
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
        let _ = settings::save(&current);
        current.clone()
    };
    let _ = app.emit("settings-changed", updated);
    Ok(backup)
}

#[tauri::command]
fn approval_decision(app: AppHandle, request_id: String, decision: String) {
    pipe::answer(&app, &request_id, &decision);
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

/// One chat turn. The API key and any file bytes stay on the Rust side.
#[tauri::command]
async fn chat_send(
    shared: State<'_, Shared>,
    chat: State<'_, Chat>,
    query: String,
    context: Option<ChatContext>,
) -> Result<ChatReply, String> {
    let model = shared.settings.lock().unwrap().model.clone();
    claude::send(&chat, &model, query, context).await
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
    notes::load()
}

/// Where the Hub's data goes for Jinx: the server, if one is set and sharing is on.
fn share_target(shared: &State<Shared>) -> Option<String> {
    let s = shared.settings.lock().unwrap();
    (s.jinx_share && !s.server_host.trim().is_empty()).then(|| s.server_host.trim().to_string())
}

#[tauri::command]
fn notes_save(shared: State<Shared>, text: String) -> Result<(), String> {
    notes::save(&text)?;
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
            None => drive::drive_upload(&remote, &folder, &file, &name, None),
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
    let hub_shared = share_target(&shared).is_some();
    jinx::send(app, &jinx, text, context, hub_shared).await
}

/// `choice`: once | session | always | deny.
#[tauri::command]
async fn jinx_approve(run_id: String, request_id: String, choice: String) -> Result<(), String> {
    jinx::approve(&run_id, &request_id, &choice).await
}

#[tauri::command]
async fn jinx_stop(jinx: State<'_, jinx::Jinx>) -> Result<(), String> {
    jinx::stop(&jinx).await
}

/// Reachable, and is the key accepted?
#[tauri::command]
async fn jinx_test() -> Result<String, String> {
    jinx::test().await
}

// ── Claude Code sessions (see agents.rs) ──────────────────────────────────────

#[tauri::command]
async fn agents_list() -> Result<serde_json::Value, String> {
    tauri::async_runtime::spawn_blocking(agents::list).await.map_err(|e| e.to_string())?
}

/// The 5-hour / 7-day usage percentages, when the statusline has written them.
#[tauri::command]
fn rate_limits() -> Option<serde_json::Value> {
    agents::limits()
}

#[tauri::command]
async fn agent_messages(session_id: String, offset: u64) -> Result<agents::Messages, String> {
    tauri::async_runtime::spawn_blocking(move || agents::messages(&session_id, offset))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn agent_send(id: String, text: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || agents::send(&id, &text)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn agent_start(cwd: String, prompt: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || agents::start(&cwd, &prompt)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
async fn agent_stop(id: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || agents::stop(&id)).await.map_err(|e| e.to_string())?
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
fn secret_set(key: String, value: String) -> Result<(), String> {
    secrets::set(&key, &value)
}

#[tauri::command]
fn secret_clear(key: String) -> Result<(), String> {
    secrets::clear(&key)
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

/// Lets the island write to the same log as the Rust side.
#[tauri::command]
fn log_line(message: String) {
    log::line(format!("ui  {message}"));
}

// ── Settings window ───────────────────────────────────────────────────────────

/// WebView2 allows exactly one browser environment per app, and its options are
/// fixed by whichever webview is created first. Every window must therefore ask
/// for the *same* arguments as the island (see `additionalBrowserArgs` in
/// tauri.conf.json) — a mismatch makes the second window come up blank, with no
/// error anywhere.
const BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --autoplay-policy=no-user-gesture-required";

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
        .title("Settings — Coucou")
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
    let gate = Arc::new(PollGate::new());

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            // `coucou --toggle` (bound to a keyboard shortcut) opens the island, or closes it if it is open.
            let what = if argv.iter().any(|a| a == "--toggle") { "toggle" } else { "open" };
            let _ = app.emit_to(island::WINDOW_LABEL, "tray", what.to_string());
        }))
        .plugin(tauri_plugin_autostart::init(MacosLauncher::LaunchAgent, None))
        .manage(Shared {
            settings: Mutex::new(loaded.clone()),
            gate: gate.clone(),
        })
        .manage(Pending::default())
        .manage(Chat::default())
        .manage(jinx::Jinx::default())
        .invoke_handler(tauri::generate_handler![
            boot,
            save_settings,
            set_collapsed,
            set_island_rect,
            focus_window,
            reposition,
            open_url,
            open_in_vscode,
            quit_app,
            hooks_status,
            hooks_preview,
            hooks_apply,
            approval_decision,
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
            sys_sample,
            server_sample,
            agents_list,
            rate_limits,
            agent_messages,
            agent_send,
            agent_start,
            agent_stop,
            jinx_send,
            jinx_approve,
            jinx_stop,
            jinx_test,
            ingest_file,
            secret_present,
            secret_set,
            secret_clear,
            refresh_integration,
            open_n8n,
            open_settings_window,
            set_paused,
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            tray::build(&handle)?;
            // Before the island: see create_settings_window.
            create_settings_window(&handle);

            if let Some(win) = island::window(&handle) {
                platform::make_non_activating(&win);
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
            integrations::start(handle.clone());
            music::sync_enabled(
                &handle,
                loaded.active_integrations.iter().any(|x| x == "integration_music"),
            );
            notifs::sync_enabled(&handle, loaded.notification_peek);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Coucou");
}
