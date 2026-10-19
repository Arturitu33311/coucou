// Jinx runs the island: her orders, from the server, executed here.
//
// Jinx (Hermes) lives on the home server and cannot reach this PC, so the server keeps her orders in a queue
// (jinx/vga-panel/server/mochi_presence.py, `/api/island/*`) and this PC asks it for them with one long request that
// the server answers the moment an order arrives (or after 25 s with nothing). Nothing is ever pushed in: the
// laptop may be off, on another network, asleep.
//
// What she may ask is a closed list (the same as island_commands.py; island-vectors.json pins both): the music, the
// pills, Mochi's mood, the Hub, and opening or closing apps. It is checked three times (her tool, the server, here)
// because what she reads — web pages, files, messages — is not to be trusted. Never here: approving or denying an
// agent's permission, sending mail, settings, keys, addresses, files, commands. Closing an app asks its window to close
// (it asks to save what is unsaved) and only for the apps the user listed in Settings → Jinx and Hub.
//
// The actions themselves are « OK Coucou »'s tools (voice::run_tool): one layer, two ways in.

use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, Manager};

use crate::island::WINDOW_LABEL;
use crate::log;
use crate::secrets;

const EMOTES: [&str; 19] = [
    "happy", "love", "proud", "surprised", "excited", "confused", "shy", "sad", "curious", "bored", "scared", "grumpy",
    "smug", "laughing", "crying", "relieved", "determined", "flustered", "sleepy",
];
const HUB_TOOLS: [&str; 12] = [
    "today", "system", "server", "pomodoro", "tasks", "workspaces", "notes", "clipboard", "calendar", "weather", "quick",
    "shelf",
];
const MUSIC_OPS: [&str; 5] = ["play_pause", "next", "previous", "set_volume", "search"];
const PILL_OPS: [&str; 4] = ["list", "add", "remove", "set_main"];
const NAME_MAX: usize = 60;
const QUERY_MAX: usize = 100;
const NAMES_MAX: usize = 4;
/// More than this many orders in a minute are refused here too, whatever the server let through.
const RATE_PER_MINUTE: usize = 30;

/// One line of printable text, no control characters, at most `limit` characters; None when nothing is left.
fn text(value: &Value, limit: usize) -> Option<String> {
    let raw = value.as_str()?;
    let clean = raw
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if clean.is_empty() {
        None
    } else {
        Some(clean.chars().take(limit).collect())
    }
}

fn number(value: &Value) -> Option<f64> {
    value.as_f64().filter(|_| !value.is_boolean())
}

/// The order as it may be run, or why not. Only the keys that are known, with their type and size bounded. The same
/// rules as `island_commands.validate` (jinx/jinx-assist), which `island-vectors.json` holds both to.
pub fn validate(action: &str, args: &Value) -> Result<Value, String> {
    let get = |k: &str| args.get(k).unwrap_or(&Value::Null).clone();
    match action {
        "state" => Ok(json!({})),
        "music" => {
            let op = get("action");
            let op = op.as_str().filter(|o| MUSIC_OPS.contains(o)).ok_or("music needs a known action")?;
            match op {
                "set_volume" => {
                    let v = number(&get("volume")).ok_or("set_volume needs volume (0-100)")?;
                    Ok(json!({ "action": op, "volume": v.clamp(0.0, 100.0) }))
                }
                "search" => {
                    let q = text(&get("query"), QUERY_MAX).ok_or("search needs query")?;
                    Ok(json!({ "action": op, "query": q }))
                }
                _ => Ok(json!({ "action": op })),
            }
        }
        "pills" => {
            let op = match args.get("action") {
                None => "list",
                Some(v) => v.as_str().filter(|o| PILL_OPS.contains(o)).ok_or("pills needs a known action")?,
            };
            if op == "list" {
                return Ok(json!({ "action": op }));
            }
            let raw = match args.get("names") {
                Some(Value::String(s)) => vec![Value::String(s.clone())],
                Some(Value::Array(a)) => a.clone(),
                _ => vec![],
            };
            let mut names: Vec<String> = raw.iter().filter_map(|n| text(n, NAME_MAX)).take(NAMES_MAX).collect();
            if names.is_empty() {
                return Err(format!("{op} needs names"));
            }
            if op == "set_main" {
                names.truncate(1);
            }
            Ok(json!({ "action": op, "names": names }))
        }
        "mochi" => {
            let emote = get("emote");
            let emote = emote.as_str().filter(|e| EMOTES.contains(e)).ok_or("mochi needs a known emote")?;
            let seconds = args.get("seconds").and_then(number).unwrap_or(3.0).clamp(1.0, 8.0);
            Ok(json!({ "emote": emote, "seconds": seconds }))
        }
        "hub" => {
            let tool = get("tool");
            let tool = tool.as_str().filter(|t| HUB_TOOLS.contains(t)).ok_or("hub needs a known tool")?;
            Ok(json!({ "tool": tool }))
        }
        "open_app" | "close_app" => {
            let name = text(&get("name"), NAME_MAX).ok_or("the app needs a name")?;
            Ok(json!({ "name": name }))
        }
        other => Err(format!("unknown action: {}", other.chars().take(30).collect::<String>())),
    }
}

/// Letters and digits only, lower case: how app names are compared ("Document Viewer" = "document-viewer").
fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

/// Is this app (by the name its own launcher gives it) one the user let Jinx close? Whole names only: "files" does not
/// allow "Profiles". Coucou itself is never closable.
pub fn close_allowed(app_name: &str, allowed: &str) -> bool {
    let wanted = norm(app_name);
    if wanted.is_empty() || wanted.starts_with("coucou") {
        return false;
    }
    allowed.split(',').map(norm).any(|a| !a.is_empty() && a == wanted)
}

/// The ids of the windows `wmctrl -lx` lists whose class is one of `names` (whole names, case aside). Pure.
/// A line is `0x03e00003  0 spotify.Spotify   host  Title`: the class is `instance.Class`.
pub fn windows_of(wmctrl_lx: &str, names: &[String]) -> Vec<String> {
    let wanted: HashSet<String> = names.iter().map(|n| n.to_lowercase()).filter(|n| !n.is_empty()).collect();
    wmctrl_lx
        .lines()
        .filter_map(|line| {
            let mut f = line.split_whitespace();
            let id = f.next()?;
            let _desktop = f.next()?;
            let class = f.next()?.to_lowercase();
            let (instance, kind) = class.split_once('.').unwrap_or((class.as_str(), ""));
            (id.starts_with("0x") && (wanted.contains(instance) || wanted.contains(kind) || wanted.contains(&class)))
                .then(|| id.to_string())
        })
        .collect()
}

/// A launcher's `.desktop` entry, as far as closing its window needs.
#[derive(Debug, Clone, PartialEq)]
pub struct DesktopApp {
    pub label: String,
    /// The names its windows may carry as their class: StartupWMClass, the file's id and its parts, the program.
    pub classes: Vec<String>,
}

/// What a `.desktop` file says, or None when it is hidden or has no name. `id` is the file name without `.desktop`.
pub fn parse_desktop(id: &str, body: &str) -> Option<DesktopApp> {
    let mut name = None;
    let mut wm_class = None;
    let mut exec = None;
    let mut in_entry = false;
    for line in body.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        if line == "NoDisplay=true" || line == "Hidden=true" {
            return None;
        }
        if name.is_none() {
            name = line.strip_prefix("Name=").map(str::trim).map(String::from);
        }
        if let Some(v) = line.strip_prefix("StartupWMClass=") {
            wm_class = Some(v.trim().to_string());
        }
        if exec.is_none() {
            exec = line.strip_prefix("Exec=").map(str::trim).map(String::from);
        }
    }
    let label = name.filter(|n| !n.is_empty())?;
    let mut classes: Vec<String> = Vec::new();
    let mut add = |c: String| {
        let c = c.to_lowercase();
        if !c.is_empty() && !classes.contains(&c) {
            classes.push(c);
        }
    };
    if let Some(c) = wm_class {
        add(c);
    }
    add(id.to_string());
    // com.spotify.Client → "spotify": the long, specific parts of a reverse-DNS id.
    const GENERIC: [&str; 7] = ["client", "desktop", "gnome", "app", "linux", "stable", "flatpak"];
    for part in id.split('.') {
        if part.len() >= 5 && !GENERIC.contains(&part.to_lowercase().as_str()) {
            add(part.to_string());
        }
    }
    // The program in `Exec=`, unless it only launches another (flatpak, env, sh…): closing "flatpak" is not closing it.
    if let Some(program) = exec.as_deref().and_then(|e| e.split_whitespace().next()) {
        let base = program.rsplit('/').next().unwrap_or(program);
        if !["flatpak", "env", "sh", "bash", "snap", "gtk-launch", "xdg-open", "python", "python3", "java"].contains(&base) {
            add(base.to_string());
        }
    }
    Some(DesktopApp { label, classes })
}

/// The installed app whose name is `wanted` (normalised): exact name first, then the start, then anywhere in it.
#[cfg(target_os = "linux")]
fn find_desktop(wanted: &str) -> Option<DesktopApp> {
    let home = crate::platform::home_dir();
    let dirs = [
        std::path::PathBuf::from("/usr/share/applications"),
        std::path::PathBuf::from("/var/lib/flatpak/exports/share/applications"),
        std::path::PathBuf::from("/var/lib/snapd/desktop/applications"),
        home.join(".local/share/applications"),
        home.join(".local/share/flatpak/exports/share/applications"),
    ];
    let mut best: Option<(usize, DesktopApp)> = None;
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().is_none_or(|x| x != "desktop") {
                continue;
            }
            let Ok(body) = std::fs::read_to_string(&path) else { continue };
            let id = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
            let Some(app) = parse_desktop(&id, &body) else { continue };
            let n = norm(&app.label);
            let score = if n == wanted { 0 } else if n.starts_with(wanted) { 1 } else if n.contains(wanted) { 2 } else { continue };
            if best.as_ref().is_none_or(|(s, _)| score < *s) {
                best = Some((score, app));
            }
        }
    }
    best.map(|(_, a)| a)
}

/// Asks an app to close its window. (done, what to tell Jinx)
#[cfg(target_os = "linux")]
fn close_app(name: &str, allowed: &str) -> (bool, String) {
    let wanted = norm(name);
    let Some(app) = find_desktop(&wanted) else { return (false, format!("No app named {name} on this computer.")) };
    if !close_allowed(&app.label, allowed) {
        return (false, format!("{} is not in the list of apps Jinx may close (Settings → Jinx and Hub).", app.label));
    }
    let listing = match std::process::Command::new("wmctrl").arg("-lx").output() {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).to_string(),
        Ok(_) => return (false, "wmctrl could not list the windows.".into()),
        Err(_) => return (false, "wmctrl is not installed on the PC (sudo apt install wmctrl), so apps cannot be closed.".into()),
    };
    let ids = windows_of(&listing, &app.classes);
    if ids.is_empty() {
        return (false, format!("{} has no open window to close.", app.label));
    }
    // Politely: each window is asked to close, the way its own close button does.
    for id in &ids {
        let _ = std::process::Command::new("wmctrl").args(["-ic", id]).status();
    }
    (true, format!("Asked {} to close ({} window{}).", app.label, ids.len(), if ids.len() == 1 { "" } else { "s" }))
}

#[cfg(not(target_os = "linux"))]
fn close_app(_name: &str, _allowed: &str) -> (bool, String) {
    (false, "Closing apps is only supported on Linux for now.".into())
}

/// What the island is told to do on its page (Mochi's mood, a Hub tool): src/island/island-cmd.ts.
fn tell_island(app: &AppHandle, payload: Value) {
    let _ = app.emit_to(WINDOW_LABEL, "island-cmd", payload);
}

/// Runs one validated order. (done, what to tell Jinx, data)
pub fn execute(app: &AppHandle, action: &str, args: &Value) -> (bool, String, Option<Value>) {
    let (enabled, allowed) = app
        .try_state::<crate::Shared>()
        .map(|s| {
            let s = s.settings.lock().unwrap();
            (s.jinx_island, s.jinx_close_apps.clone())
        })
        .unwrap_or((true, String::new()));
    if !enabled {
        return (false, "Jinx's control of the island is off in Settings → Jinx and Hub.".into(), None);
    }
    match action {
        "state" => match crate::voice::ask_island(app, "info", json!({})) {
            Some(raw) => match serde_json::from_str::<Value>(&raw) {
                Ok(v) => (true, String::new(), Some(v)),
                Err(_) => (false, "the island answered something unreadable".into(), None),
            },
            None => (false, "the island did not answer".into(), None),
        },
        "music" => {
            let (message, done) = crate::voice::run_tool(app, "music", args);
            (done, message, None)
        }
        "pills" => {
            let (message, done) = crate::voice::run_tool(app, "pills", args);
            if args["action"] == "list" {
                (true, String::new(), serde_json::from_str(&message).ok())
            } else {
                (done, message, None)
            }
        }
        "open_app" => {
            let (message, done) = crate::voice::run_tool(app, "open_app", args);
            (done, message, None)
        }
        "close_app" => {
            let (done, message) = close_app(args["name"].as_str().unwrap_or(""), &allowed);
            (done, message, None)
        }
        "mochi" => {
            tell_island(app, json!({ "kind": "emote", "emote": args["emote"], "seconds": args["seconds"] }));
            (true, "done".into(), None)
        }
        "hub" => {
            tell_island(app, json!({ "kind": "hub", "tool": args["tool"] }));
            (true, "done".into(), None)
        }
        _ => (false, "unknown action".into(), None),
    }
}

/// Validates and runs what the server handed over; the answer to post back.
fn handle(app: &AppHandle, command: &Value) -> Value {
    let id = command["id"].as_i64().unwrap_or(0);
    let action = command["action"].as_str().unwrap_or("");
    let (ok, message, data) = match validate(action, &command["args"]) {
        Ok(args) => execute(app, action, &args),
        Err(why) => (false, format!("refused here: {why}"), None),
    };
    // The log says what was asked, never what it was about (an app, a search).
    log::line(format!("jinx island: {action} {}", if ok { "done" } else { "not done" }));
    let mut out = json!({ "id": id, "ok": ok, "message": message });
    if let Some(d) = data {
        out["data"] = d;
    }
    out
}

/// Keeps at most `RATE_PER_MINUTE` orders in the last minute; false means this one is over the limit.
fn within_rate(stamps: &mut VecDeque<Instant>, now: Instant) -> bool {
    while stamps.front().is_some_and(|t| now.duration_since(*t) > Duration::from_secs(60)) {
        stamps.pop_front();
    }
    if stamps.len() >= RATE_PER_MINUTE {
        return false;
    }
    stamps.push_back(now);
    true
}

/// Starts asking the server for Jinx's orders. Safe to call once at launch; does nothing until the presence server's
/// address and key are in the keyring.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let client = match reqwest::Client::builder().timeout(Duration::from_secs(40)).build() {
            Ok(c) => c,
            Err(_) => return,
        };
        let mut done: VecDeque<i64> = VecDeque::new();
        let mut stamps: VecDeque<Instant> = VecDeque::new();
        let mut backoff = 5u64;
        loop {
            let (Some(url), Some(key)) = (secrets::get("presence-url"), secrets::get("presence-key")) else {
                tokio::time::sleep(Duration::from_secs(30)).await;
                continue;
            };
            let base = url.trim_end_matches('/').to_string();
            let polled = client
                .get(format!("{base}/api/island/poll?device=pc&wait=25"))
                .header("X-Mochi-Key", &key)
                .send()
                .await;
            let body = match polled {
                Ok(r) if r.status().is_success() => r.json::<Value>().await.ok(),
                _ => None,
            };
            let Some(body) = body else {
                // Server down, or an older one without the queue: ask again later, not in a tight loop.
                tokio::time::sleep(Duration::from_secs(backoff)).await;
                backoff = (backoff * 2).min(120);
                continue;
            };
            backoff = 5;
            for command in body["commands"].as_array().cloned().unwrap_or_default() {
                let id = command["id"].as_i64().unwrap_or(0);
                if done.contains(&id) {
                    continue;
                }
                done.push_back(id);
                if done.len() > 200 {
                    done.pop_front();
                }
                let answer = if within_rate(&mut stamps, Instant::now()) {
                    let handle = app.clone();
                    let cmd = command.clone();
                    tauri::async_runtime::spawn_blocking(move || handle_blocking(&handle, &cmd))
                        .await
                        .unwrap_or_else(|_| json!({ "id": id, "ok": false, "message": "the island failed" }))
                } else {
                    json!({ "id": id, "ok": false, "message": "too many orders in a minute" })
                };
                let _ = client
                    .post(format!("{base}/api/island/result"))
                    .header("X-Mochi-Key", &key)
                    .json(&answer)
                    .send()
                    .await;
            }
        }
    });
}

fn handle_blocking(app: &AppHandle, command: &Value) -> Value {
    handle(app, command)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The vectors the server's validator (island_commands.py) is held to as well.
    #[test]
    fn the_rules_are_the_servers() {
        let vectors: Vec<Value> = serde_json::from_str(include_str!("../../scripts/phone/island-vectors.json")).unwrap();
        assert!(vectors.len() > 25);
        for v in vectors {
            let got = validate(v["action"].as_str().unwrap_or(""), &v["args"]);
            match (v["ok"].as_bool().unwrap(), got) {
                (true, Ok(clean)) => assert_eq!(clean, v["clean"], "{v}"),
                (false, Err(_)) => {}
                (want, got) => panic!("{v}: wanted ok={want}, got {got:?}"),
            }
        }
    }

    #[test]
    fn only_listed_apps_close_by_their_whole_name_and_never_coucou() {
        let list = "Spotify, VLC, Files, Document Viewer";
        assert!(close_allowed("Spotify", list));
        assert!(close_allowed("document-viewer", list));
        assert!(close_allowed("vlc", list));
        assert!(!close_allowed("Profiles", list), "a name that only contains an entry");
        assert!(!close_allowed("Firefox", list));
        assert!(!close_allowed("", list));
        assert!(!close_allowed("Spotify", ""), "nothing listed, nothing closes");
        assert!(!close_allowed("Coucou", "Coucou, Spotify"), "never itself");
        assert!(!close_allowed("Coucou Island", "Coucou Island"));
    }

    #[test]
    fn windows_are_found_by_the_whole_class_of_the_app() {
        let listing = "\
0x03e00003  0 spotify.Spotify          host Spotify Premium
0x04a00007  0 Navigator.firefox        host Mozilla Firefox
0x04a00099  1 gnome-terminal-server.Gnome-terminal host Terminal
0x05000001  0 spotify-helper.Other     host something else
";
        let names = vec!["spotify".to_string()];
        assert_eq!(windows_of(listing, &names), vec!["0x03e00003"]);
        assert_eq!(windows_of(listing, &["firefox".to_string()]), vec!["0x04a00007"], "the class half");
        assert!(windows_of(listing, &["gnome".to_string()]).is_empty(), "no partial matches");
        assert!(windows_of(listing, &[]).is_empty());
        assert!(windows_of("garbage line\n\n", &names).is_empty());
    }

    #[test]
    fn a_launcher_says_which_window_class_is_its() {
        let body = "[Desktop Entry]\nName=Spotify\nExec=spotify %U\nStartupWMClass=spotify\n[Desktop Action x]\nName=Other\n";
        let app = parse_desktop("spotify", body).unwrap();
        assert_eq!(app.label, "Spotify");
        assert!(app.classes.contains(&"spotify".to_string()));

        let flatpak = "[Desktop Entry]\nName=Spotify\nExec=flatpak run --branch=stable com.spotify.Client\n";
        let app = parse_desktop("com.spotify.Client", flatpak).unwrap();
        assert!(app.classes.contains(&"spotify".to_string()), "the specific part of the id");
        assert!(!app.classes.contains(&"flatpak".to_string()), "never the program that only launches it");
        assert!(!app.classes.contains(&"client".to_string()), "nothing generic");

        assert!(parse_desktop("x", "[Desktop Entry]\nName=Hidden\nNoDisplay=true\n").is_none());
        assert!(parse_desktop("x", "[Desktop Entry]\nExec=foo\n").is_none(), "no name");
    }

    #[test]
    fn the_rate_is_bounded_here_too() {
        let mut stamps = VecDeque::new();
        let t = Instant::now();
        for _ in 0..RATE_PER_MINUTE {
            assert!(within_rate(&mut stamps, t));
        }
        assert!(!within_rate(&mut stamps, t));
        assert!(within_rate(&mut stamps, t + Duration::from_secs(61)));
    }
}
