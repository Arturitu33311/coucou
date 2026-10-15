// OpenCode sessions in the island, next to the Claude Code ones (see agents.rs).
//
// The island never speaks OpenCode's server protocol directly: everything goes
// through its own command line, which is stable and needs no token:
//   * `opencode session list --format json`   all sessions (newest first);
//   * `opencode session export <id>`          the full conversation of one;
//   * `opencode run --session <id> <text>`    a message into one (detached: the
//                                            reply arrives through the export);
//   * `opencode run <prompt>`                 a new session in a folder.
//
// Differences with Claude Code that the island shows honestly:
//   * OpenCode reports no live working/done state: a session touched in the last
//     two minutes reads as working, anything older as done.
//   * There is no stop: OpenCode sessions run to the end on their own.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

use crate::agents::{Messages, Msg, clip};
use crate::platform::home_dir;

/// Run opencode and read its stdout as JSON. Through a temp file, not a pipe:
/// a piped stdout is cut at 256 KB, and a long session's export is bigger.
fn capture_json(args: &[&str], dir: Option<&str>) -> Result<Value, String> {
    let path = std::env::temp_dir().join(format!("coucou-op-{}.json", std::process::id()));
    let file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
    let mut cmd = Command::new(opencode_bin()?);
    cmd.args(args).stdin(Stdio::null()).stdout(file).stderr(Stdio::null());
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    let status = cmd.status().map_err(|e| format!("cannot run opencode: {e}"))?;
    let result = if !status.success() {
        Err("opencode did not answer".into())
    } else {
        let raw = std::fs::read(&path).map_err(|e| format!("unreadable opencode output: {e}"))?;
        serde_json::from_slice(&raw).map_err(|e| format!("unreadable opencode output: {e}"))
    };
    let _ = std::fs::remove_file(&path);
    result
}

/// Sessions touched this recently count as working (see above).
const WORKING_MS: i64 = 120_000;
/// The chat shows the recent past of a long session.
const FIRST_MESSAGES: usize = 40;

/// An OpenCode id looks like `ses_ee6c1abb6ffe4pcgvp8WPiZiAE`.
pub fn is_opencode(id: &str) -> bool {
    let rest = id.strip_prefix("ses_").unwrap_or("");
    !rest.is_empty()
        && rest.len() <= 32
        && rest.bytes().all(|b| b.is_ascii_alphanumeric())
}

fn opencode_bin() -> Result<PathBuf, String> {
    let on_path = std::env::var_os("PATH").and_then(|dirs| {
        std::env::split_paths(&dirs)
            .map(|d| d.join("opencode"))
            .find(|p| p.is_file())
    });
    on_path
        .or_else(|| {
            let p = home_dir().join(".opencode").join("bin").join("opencode");
            p.is_file().then_some(p)
        })
        .ok_or_else(|| "OpenCode (`opencode`) was not found".to_string())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// All sessions as the agents view lists them: id, name, cwd, kind, state, sessionId.
///
/// `opencode api session.list` talks to the background service, so it sees every
/// project (the `session list` command only sees the current folder). When the
/// service is not there, the folder command in the home directory is the fallback.
pub fn list() -> Result<Value, String> {
    match api_list() {
        ok @ Ok(_) => ok,
        Err(_) => cli_list().map_err(|_| "OpenCode is not answering (`opencode api session.list`)".to_string()),
    }
}

fn api_list() -> Result<Value, String> {
    let v = capture_json(&["api", "session.list"], None)
        .map_err(|_| "`opencode api session.list` failed".to_string())?;
    let all: Vec<Value> = v["data"]
        .as_array()
        .map(|rows| rows.iter().map(entry).collect())
        .unwrap_or_default();
    Ok(Value::Array(all))
}

fn cli_list() -> Result<Value, String> {
    let home = home_dir().to_string_lossy().to_string();
    let rows: Vec<Value> = capture_json(&["session", "list", "--format", "json"], Some(&home))
        .map_err(|_| "`opencode session list` failed".to_string())?
        .as_array()
        .cloned()
        .unwrap_or_default();
    Ok(Value::Array(rows.iter().map(entry).collect()))
}

fn entry(s: &Value) -> Value {
    let now = now_ms();
    // The API nests these one level deeper than the folder command.
    let dir = s["location"]["directory"].as_str().or_else(|| s["directory"].as_str()).unwrap_or("");
    let title = s["title"].as_str().unwrap_or("");
    let updated = s["time"]["updated"].as_i64().or_else(|| s["updated"].as_i64()).unwrap_or(0);
    let created = &s["time"]["created"];
    let created = if created.is_number() { created.clone() } else { s["created"].clone() };
    let state = if updated > 0 && now - updated < WORKING_MS { "working" } else { "done" };
    let id = s["id"].as_str().unwrap_or("").to_string();
    json!({
        "id": id,
        "name": title,
        "cwd": dir,
        "kind": "opencode",
        "status": "",
        "state": state,
        "sessionId": id,
        "waitingFor": Value::Null,
        "startedAt": created,
    })
}

/// What was said in a session: `offset` counts converted messages (0 = the recent past).
pub fn messages(id: &str, offset: u64) -> Result<Messages, String> {
    if !is_opencode(id) {
        return Err("not an OpenCode session".into());
    }
    // The export is written while the session may be writing it: a half-written
    // file reads as a parse error, so retries are cheaper than a stale thread.
    // (The next poll beat tries again anyway; this just avoids a visible gap.)
    let mut last = String::new();
    for _ in 0..4 {
        match export(id) {
            Ok(v) => return sliced(&v, offset),
            Err(e) => {
                last = e;
                std::thread::sleep(std::time::Duration::from_millis(700));
            }
        }
    }
    Err(last)
}

fn export(id: &str) -> Result<Value, String> {
    capture_json(&["session", "export", id], None)
        .map_err(|_| "that OpenCode session is gone".to_string())
}

fn sliced(v: &Value, offset: u64) -> Result<Messages, String> {
    let mut all: Vec<Msg> = v["messages"]
        .as_array()
        .map(|ms| ms.iter().flat_map(convert).collect())
        .unwrap_or_default();
    if offset == 0 && all.len() > FIRST_MESSAGES {
        let total = all.len() as u64;
        all.drain(..all.len() - FIRST_MESSAGES);
        return Ok(Messages { offset: total, messages: all });
    }
    let skip = (offset as usize).min(all.len());
    let done = all.len() as u64;
    all.drain(..skip);
    Ok(Messages { offset: done, messages: all })
}

/// One export message into island lines.
fn convert(m: &Value) -> Vec<Msg> {
    match m["type"].as_str() {
        Some("user") => {
            let t = m["text"].as_str().unwrap_or("").trim();
            if t.is_empty() { Vec::new() } else { vec![Msg { role: "user", text: clip(t.to_string()) }] }
        }
        Some("assistant") => m["content"]
            .as_array()
            .map(|blocks| {
                blocks
                    .iter()
                    .filter_map(|b| match b["type"].as_str() {
                        Some("text") => b["text"]
                            .as_str()
                            .filter(|t| !t.trim().is_empty())
                            .map(|t| Msg { role: "assistant", text: clip(t.trim().to_string()) }),
                        Some("tool") => Some(Msg { role: "tool", text: tool_line(b) }),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// One line of a short, readable target for a tool call: `read · /home/alan/x`.
fn tool_line(block: &Value) -> String {
    let name = block["name"].as_str().unwrap_or("tool");
    let input = &block["state"]["input"];
    let target = ["path", "file", "file_path", "command", "pattern", "url", "description"]
        .iter()
        .find_map(|k| input[*k].as_str())
        .unwrap_or("");
    let target: String = target.lines().next().unwrap_or("").chars().take(70).collect();
    if target.is_empty() { name.to_string() } else { format!("{name} · {target}") }
}

/// The folder a session lives in (a message is sent from there).
fn directory_of(id: &str) -> Option<String> {
    list().ok()?.as_array()?.iter().find(|s| s["id"] == id).and_then(|s| {
        let d = s["cwd"].as_str().unwrap_or("");
        (!d.is_empty()).then(|| d.to_string())
    })
}

/// A message into a session, through `opencode run` left to finish on its own.
/// The reply shows up in the thread as the export grows.
pub fn send(id: &str, text: &str) -> Result<(), String> {
    if !is_opencode(id) {
        return Err("not an OpenCode session".into());
    }
    let clean: String = text
        .chars()
        .map(|c| if c == '\n' || c == '\t' { ' ' } else { c })
        .filter(|c| !c.is_control())
        .take(8 * 1024)
        .collect();
    if clean.trim().is_empty() {
        return Ok(());
    }
    let dir = directory_of(id).ok_or("that OpenCode session is gone")?;
    Command::new(opencode_bin()?)
        .args(["run", "--session", id, &clean])
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("cannot message that session: {e}"))?;
    Ok(())
}

/// A new session in `cwd`; returns its id once it exists.
pub fn start(cwd: &str, prompt: &str) -> Result<String, String> {
    let home = home_dir();
    let dir = if cwd.is_empty() { home.to_string_lossy().to_string() } else { cwd.to_string() };
    if !std::path::Path::new(&dir).is_dir() {
        return Err("That folder does not exist".into());
    }
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err("Say what the agent should do".into());
    }
    let before = now_ms();
    Command::new(opencode_bin()?)
        .args(["run", prompt])
        .current_dir(&dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("cannot run opencode: {e}"))?;
    // The session exists long before the run finishes: take the newest one here.
    for _ in 0..24 {
        std::thread::sleep(std::time::Duration::from_millis(500));
        if let Ok(Value::Array(all)) = list() {
            if let Some(id) = all.iter().find_map(|s| {
                let created = s["startedAt"].as_i64().unwrap_or(0);
                (s["kind"] == "opencode" && created >= before - 2_000 && s["cwd"] == dir)
                    .then(|| s["id"].as_str().unwrap_or("").to_string())
                    .filter(|id| !id.is_empty())
            }) {
                return Ok(id);
            }
        }
    }
    Err("OpenCode started, but the session has not shown up yet — it is in the list".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_opencode_ids_take_the_opencode_path() {
        assert!(is_opencode("ses_ee6c1abb6ffe4pcgvp8WPiZiAE"));
        assert!(!is_opencode("4e9bc392"));
        assert!(!is_opencode("ses_"));
        assert!(!is_opencode("ses_xx; rm -rf ~"));
        assert!(!is_opencode(""));
    }

    #[test]
    fn a_list_entry_becomes_an_agent_row() {
        // `opencode api session.list` shape…
        let s = json!({
            "id": "ses_abc123", "title": "Fix it",
            "location": {"directory": "/home/alan/x"},
            "time": {"updated": now_ms(), "created": 1},
        });
        let e = entry(&s);
        assert_eq!(e["kind"], "opencode");
        assert_eq!(e["name"], "Fix it");
        assert_eq!(e["sessionId"], "ses_abc123");
        assert_eq!(e["state"], "working");
        // …and the `session list` folder-command shape.
        let s = json!({
            "id": "ses_old", "title": "Old", "directory": "/home/alan",
            "updated": 1, "created": 1,
        });
        let e = entry(&s);
        assert_eq!(e["cwd"], "/home/alan");
        assert_eq!(e["state"], "done");
    }

    #[test]
    fn an_export_becomes_messages_and_the_rest_is_dropped() {
        let v = json!({"messages": [
            {"type": "user", "text": "hola"},
            {"type": "assistant", "content": [
                {"type": "reasoning", "text": "..."},
                {"type": "text", "text": "Voy"},
                {"type": "tool", "name": "read", "state": {"input": {"path": "/home/alan/a.ts"}}},
            ]},
            {"type": "idle"},
        ]});
        let ms: Vec<Msg> = v["messages"].as_array().unwrap().iter().flat_map(convert).collect();
        assert_eq!(ms.len(), 3);
        assert_eq!(ms[0], Msg { role: "user", text: "hola".into() });
        assert_eq!(ms[1], Msg { role: "assistant", text: "Voy".into() });
        assert_eq!(ms[2], Msg { role: "tool", text: "read · /home/alan/a.ts".into() });
    }
}
