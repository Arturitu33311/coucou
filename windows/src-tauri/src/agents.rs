// Claude Code sessions in the island: the ones that are running right now (what
// `claude agents` lists), their state, what has been said in them, a message into
// one, and new background agents.
//
// Everything goes through Claude Code's own command line, so there is no API key
// and nothing to keep in step with its internals:
//   * `claude agents --json`   the running sessions and their state
//                              (working / done / blocked, and what a blocked one waits for);
//   * the session transcript   ~/.claude/projects/<folder>/<session id>.jsonl, read as it grows;
//   * `claude attach <id>`     run in a pseudo-terminal just long enough to type one message
//                              (there is no other way into a live session);
//   * `claude --bg <prompt>`   a new background agent;  `claude stop <id>` ends one.
//
// A session that is waiting for an answer is never typed into: its question or
// permission request is the island's card (hooks), or the terminal's.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Serialize;
use serde_json::{json, Value};

use crate::platform::home_dir;

/// A transcript is read from its end when it is big: the island shows the recent past.
const TAIL_BYTES: u64 = 256 * 1024;
const FIRST_MESSAGES: usize = 40;
const MAX_MESSAGE: usize = 8 * 1024;

#[derive(Serialize, Debug, PartialEq, Clone)]
pub struct Msg {
    /// "user" | "assistant" | "tool", and the queue's own: "queued" (typed while the agent was
    /// busy), "dequeued" (it ran, or was taken back; `text` names it when known), "queue-clear".
    pub role: &'static str,
    pub text: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Messages {
    pub offset: u64,
    pub messages: Vec<Msg>,
}

fn claude_bin() -> Result<PathBuf, String> {
    let on_path = std::env::var_os("PATH").and_then(|dirs| {
        std::env::split_paths(&dirs)
            .map(|d| d.join("claude"))
            .find(|p| p.is_file())
    });
    let home = home_dir();
    on_path
        .or_else(|| [".local/bin/claude", ".claude/local/claude", ".npm-global/bin/claude"]
            .iter()
            .map(|rel| home.join(rel))
            .find(|p| p.is_file()))
        .ok_or_else(|| "Claude Code (`claude`) was not found".to_string())
}

/// Short ids are 8 hex characters; nothing else is ever passed to a command line.
fn valid_id(id: &str) -> bool {
    (6..=12).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The running sessions: id, name, folder, kind, status, state, waitingFor, sessionId.
pub fn list() -> Result<Value, String> {
    let out = Command::new(claude_bin()?)
        .args(["agents", "--json"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run claude: {e}"))?;
    if !out.status.success() {
        return Err("`claude agents` failed".into());
    }
    let all: Vec<Value> = serde_json::from_slice(&out.stdout).map_err(|e| format!("unreadable agent list: {e}"))?;
    Ok(Value::Array(
        all.into_iter()
            .map(|a| {
                json!({
                    "id": a["id"], "name": a["name"], "cwd": a["cwd"], "kind": a["kind"],
                    "status": a["status"], "state": a["state"], "sessionId": a["sessionId"],
                    "waitingFor": a["waitingFor"], "startedAt": a["startedAt"],
                })
            })
            .collect(),
    ))
}

/// (state, sessionId) of a running agent.
fn find_agent(id: &str) -> Result<(String, String), String> {
    let list = list()?;
    list.as_array()
        .and_then(|a| a.iter().find(|x| x["id"] == id))
        .map(|x| {
            (
                x["state"].as_str().unwrap_or("").to_string(),
                x["sessionId"].as_str().unwrap_or("").to_string(),
            )
        })
        .ok_or_else(|| "That agent is no longer running".to_string())
}

/// Claude Code's usage limits as the statusline script last wrote them
/// (~/.claude/rate_limits.json): used percentage and reset time of the 5-hour and 7-day windows.
pub fn limits() -> Option<Value> {
    let raw = std::fs::read_to_string(home_dir().join(".claude").join("rate_limits.json")).ok()?;
    parse_limits(&raw)
}

fn parse_limits(raw: &str) -> Option<Value> {
    let v: Value = serde_json::from_str(raw).ok()?;
    let window = |k: &str| {
        let pct = v[k]["used_percentage"].as_f64()?;
        Some(json!({ "pct": pct.round(), "resetsAt": v[k]["resets_at"] }))
    };
    let (five, seven) = (window("five_hour"), window("seven_day"));
    if five.is_none() && seven.is_none() {
        return None;
    }
    Some(json!({ "five": five, "seven": seven, "ts": v["ts"] }))
}

// ── Transcript ────────────────────────────────────────────────────────────────

fn transcript_path(projects: &Path, session_id: &str) -> Option<PathBuf> {
    if !session_id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-') || session_id.len() < 8 {
        return None;
    }
    std::fs::read_dir(projects).ok()?.flatten().find_map(|dir| {
        let p = dir.path().join(format!("{session_id}.jsonl"));
        p.is_file().then_some(p)
    })
}

pub(crate) fn clip(mut s: String) -> String {
    if s.len() > MAX_MESSAGE {
        let mut end = MAX_MESSAGE;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        s.truncate(end);
        s.push('…');
    }
    s
}

/// One line of a short, readable target for a tool call: `Edit · src/a.ts`.
fn tool_line(block: &Value) -> String {
    let name = block["name"].as_str().unwrap_or("tool");
    let input = &block["input"];
    let target = ["file_path", "path", "command", "pattern", "url", "description"]
        .iter()
        .find_map(|k| input[*k].as_str())
        .unwrap_or("");
    let target: String = target.lines().next().unwrap_or("").chars().take(70).collect();
    if target.is_empty() { name.to_string() } else { format!("{name} · {target}") }
}

/// Text that is the harness talking, not the user.
fn is_noise(text: &str) -> bool {
    let t = text.trim_start();
    t.starts_with("<command-") || t.starts_with("<local-command") || t.starts_with("<system-reminder")
        || t.starts_with("Caveat:") || t.starts_with("[Request interrupted")
}

/// The conversation lines of one transcript record (none for everything that is not talk).
fn parse_line(line: &str) -> Vec<Msg> {
    let Ok(v) = serde_json::from_str::<Value>(line) else { return Vec::new() };
    if v["isSidechain"].as_bool().unwrap_or(false) || v["isMeta"].as_bool().unwrap_or(false) {
        return Vec::new();
    }
    let content = &v["message"]["content"];
    let mut out = Vec::new();
    match v["type"].as_str() {
        // Claude Code's own queue: what was typed while it was busy and has not run yet.
        Some("queue-operation") => {
            let text = v["content"].as_str().unwrap_or("").trim().to_string();
            match v["operation"].as_str() {
                Some("enqueue") if !text.is_empty() => out.push(Msg { role: "queued", text: clip(text) }),
                Some("dequeue") | Some("remove") => out.push(Msg { role: "dequeued", text: clip(text) }),
                Some("popAll") => out.push(Msg { role: "queue-clear", text: String::new() }),
                _ => {}
            }
        }
        Some("user") => {
            let texts: Vec<&str> = match content {
                Value::String(s) => vec![s.as_str()],
                Value::Array(items) => items
                    .iter()
                    .filter(|b| b["type"] == "text")
                    .filter_map(|b| b["text"].as_str())
                    .collect(),
                _ => Vec::new(),
            };
            for t in texts.into_iter().filter(|t| !t.trim().is_empty() && !is_noise(t)) {
                out.push(Msg { role: "user", text: clip(t.trim().to_string()) });
            }
        }
        Some("assistant") => {
            if let Value::Array(items) = content {
                for b in items {
                    match b["type"].as_str() {
                        Some("text") => {
                            if let Some(t) = b["text"].as_str().filter(|t| !t.trim().is_empty()) {
                                out.push(Msg { role: "assistant", text: clip(t.trim().to_string()) });
                            }
                        }
                        Some("tool_use") => out.push(Msg { role: "tool", text: tool_line(b) }),
                        _ => {}
                    }
                }
            }
        }
        _ => {}
    }
    out
}

/// What was said in a session since `offset` (0 = the recent past). Only whole
/// lines are consumed, so a half-written record is read on the next call.
pub fn messages(session_id: &str, offset: u64) -> Result<Messages, String> {
    messages_from(&home_dir().join(".claude").join("projects"), session_id, offset)
}

fn messages_from(projects: &Path, session_id: &str, offset: u64) -> Result<Messages, String> {
    let path = transcript_path(projects, session_id).ok_or("No transcript for that session")?;
    let mut file = std::fs::File::open(&path).map_err(|e| e.to_string())?;
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    let initial = offset == 0;
    let start = if offset > len {
        0 // the file was replaced: start again
    } else if initial && len > TAIL_BYTES {
        len - TAIL_BYTES
    } else {
        offset
    };
    file.seek(SeekFrom::Start(start)).map_err(|e| e.to_string())?;
    let mut raw = Vec::new();
    file.read_to_end(&mut raw).map_err(|e| e.to_string())?;
    let Some(last_nl) = raw.iter().rposition(|b| *b == b'\n') else {
        return Ok(Messages { offset: start, messages: Vec::new() });
    };
    let consumed = &raw[..=last_nl];
    let text = String::from_utf8_lossy(consumed);
    let mut lines = text.lines();
    if start > 0 && initial {
        lines.next(); // began in the middle of a record
    }
    let mut messages: Vec<Msg> = lines.flat_map(parse_line).collect();
    if initial && messages.len() > FIRST_MESSAGES {
        messages.drain(..messages.len() - FIRST_MESSAGES);
    }
    Ok(Messages { offset: start + consumed.len() as u64, messages })
}

// ── Actions ───────────────────────────────────────────────────────────────────

/// Did this transcript, past `offset`, record `text` as sent (typed into the session, or queued)?
fn has_sent(path: &Path, offset: u64, text: &str) -> bool {
    let Ok(mut file) = std::fs::File::open(path) else { return false };
    if file.seek(SeekFrom::Start(offset)).is_err() {
        return false;
    }
    let mut raw = Vec::new();
    if file.read_to_end(&mut raw).is_err() {
        return false;
    }
    String::from_utf8_lossy(&raw).lines().any(|l| line_has_text(l, text))
}

fn line_has_text(line: &str, text: &str) -> bool {
    let Ok(v) = serde_json::from_str::<Value>(line) else { return false };
    let want = text.trim();
    let same = |seen: &str| {
        let seen = seen.trim();
        seen == want || (!seen.is_empty() && want.len() > 60 && want.starts_with(seen.get(..60).unwrap_or(seen)))
    };
    match v["type"].as_str() {
        Some("queue-operation") => v["operation"] == "enqueue" && v["content"].as_str().is_some_and(same),
        Some("user") => match &v["message"]["content"] {
            Value::String(s) => same(s),
            Value::Array(items) => items.iter().any(|b| b["type"] == "text" && b["text"].as_str().is_some_and(same)),
            _ => false,
        },
        _ => false,
    }
}

/// A message into a running session, typed through `claude attach` in a pseudo-terminal.
/// The text and the Enter go in separately (together they look like a paste, whose
/// newline is not a submit), and the transcript is how we know it was really sent: if
/// it was not, the Enter is repeated, and the line is cleared rather than left in the
/// session's input box for someone to find.
#[cfg(target_os = "linux")]
pub fn send(id: &str, text: &str) -> Result<(), String> {
    use std::io::Write;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    if !valid_id(id) {
        return Err("invalid agent id".into());
    }
    let (state, session_id) = find_agent(id)?;
    if state == "blocked" {
        return Err("This agent is waiting for an answer: answer its card, or in its terminal".into());
    }
    let clean: String = text
        .chars()
        .map(|c| if c == '\n' || c == '\t' { ' ' } else { c })
        .filter(|c| !c.is_control())
        .take(MAX_MESSAGE)
        .collect();
    if clean.trim().is_empty() {
        return Ok(());
    }
    let transcript = transcript_path(&home_dir().join(".claude").join("projects"), &session_id);
    let before = transcript.as_ref().and_then(|p| std::fs::metadata(p).ok()).map(|m| m.len()).unwrap_or(0);

    let claude = claude_bin()?;
    let quoted = format!("'{}'", claude.to_string_lossy().replace('\'', r"'\''"));
    // `script` gives the command a terminal; the size is set first because the
    // interface draws itself for it.
    let mut child = Command::new("script")
        .args(["-qfc", &format!("stty cols 120 rows 40; exec {quoted} attach {id}"), "/dev/null"])
        .env("TERM", "xterm-256color")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("cannot open a terminal for the agent: {e}"))?;

    // The interface's prompt (❯) is how we know it is ready; its output is otherwise discarded.
    let ready = Arc::new(AtomicBool::new(false));
    if let Some(mut out) = child.stdout.take() {
        let ready = ready.clone();
        std::thread::spawn(move || {
            let mut buf = [0u8; 8192];
            while let Ok(n) = out.read(&mut buf) {
                if n == 0 {
                    break;
                }
                if buf[..n].windows(3).any(|w| w == "❯".as_bytes()) {
                    ready.store(true, Ordering::Relaxed);
                }
            }
        });
    }
    let deadline = Instant::now() + Duration::from_secs(8);
    while !ready.load(Ordering::Relaxed) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    let finish = |child: &mut std::process::Child| {
        let _ = child.kill();
        let _ = child.wait();
    };
    if !ready.load(Ordering::Relaxed) {
        finish(&mut child);
        return Err("Could not reach the agent's session".into());
    }
    std::thread::sleep(Duration::from_millis(500));

    let mut type_in = |bytes: &[u8]| -> bool {
        child
            .stdin
            .as_mut()
            .map(|stdin| stdin.write_all(bytes).and_then(|_| stdin.flush()).is_ok())
            .unwrap_or(false)
    };
    if !type_in(clean.as_bytes()) {
        finish(&mut child);
        return Err("Could not type into the agent's session".into());
    }
    std::thread::sleep(Duration::from_millis(350));
    type_in(b"\r");

    // Without a transcript to check there is nothing to confirm against.
    let Some(path) = transcript else {
        std::thread::sleep(Duration::from_millis(1500));
        finish(&mut child);
        return Ok(());
    };
    let mut sent = false;
    'attempts: for attempt in 0..3 {
        let until = Instant::now() + Duration::from_millis(2500);
        while Instant::now() < until {
            if has_sent(&path, before, &clean) {
                sent = true;
                break 'attempts;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        if attempt < 2 {
            type_in(b"\r");
        }
    }
    if !sent {
        // Ctrl+U: take the unsent text back out of the input box.
        type_in(b"\x15");
        std::thread::sleep(Duration::from_millis(300));
    } else {
        std::thread::sleep(Duration::from_millis(300));
    }
    finish(&mut child);
    if sent {
        Ok(())
    } else {
        Err("The message was typed but the agent did not take it; it was cleared, try again".into())
    }
}

#[cfg(not(target_os = "linux"))]
pub fn send(_id: &str, _text: &str) -> Result<(), String> {
    Err("Talking to a running agent is only available on Linux for now".into())
}

/// A new background agent in `cwd`; returns its short id.
pub fn start(cwd: &str, prompt: &str) -> Result<String, String> {
    // No folder chosen: the home directory, as a terminal would start in.
    let home = home_dir();
    let dir = if cwd.is_empty() { home.as_path() } else { Path::new(cwd) };
    if !dir.is_dir() {
        return Err("That folder does not exist".into());
    }
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err("Say what the agent should do".into());
    }
    let out = Command::new(claude_bin()?)
        .arg("--bg")
        .arg(prompt)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run claude: {e}"))?;
    first_id(&String::from_utf8_lossy(&out.stdout))
        .ok_or_else(|| "Claude Code did not start the agent".to_string())
}

pub fn stop(id: &str) -> Result<(), String> {
    if !valid_id(id) {
        return Err("invalid agent id".into());
    }
    Command::new(claude_bin()?)
        .args(["stop", id])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// The first 8-hex-digit token of `claude --bg`'s output (colours stripped).
fn first_id(out: &str) -> Option<String> {
    let mut plain = String::new();
    let mut chars = out.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for n in chars.by_ref() {
                if n.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            plain.push(c);
        }
    }
    plain
        .split(|c: char| !c.is_ascii_alphanumeric())
        .find(|t| t.len() == 8 && t.bytes().all(|b| b.is_ascii_hexdigit()) && t.bytes().any(|b| b.is_ascii_digit()))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_limits_file_becomes_two_percentages() {
        let raw = r#"{"device":"x","ts":1,"five_hour":{"used_percentage":8.6,"resets_at":5},"seven_day":{"used_percentage":59,"resets_at":9}}"#;
        let l = parse_limits(raw).unwrap();
        assert_eq!(l["five"]["pct"], 9.0);
        assert_eq!(l["seven"]["resetsAt"], 9);
        assert!(parse_limits("{}").is_none());
        assert!(parse_limits("nope").is_none());
        assert!(parse_limits(r#"{"seven_day":{"used_percentage":3}}"#).unwrap()["five"].is_null());
    }

    #[test]
    fn the_agents_queue_is_visible_as_queued_messages() {
        let enq = r#"{"type":"queue-operation","operation":"enqueue","content":"haz esto luego"}"#;
        assert_eq!(parse_line(enq), vec![Msg { role: "queued", text: "haz esto luego".into() }]);
        let deq = r#"{"type":"queue-operation","operation":"dequeue"}"#;
        assert_eq!(parse_line(deq), vec![Msg { role: "dequeued", text: String::new() }]);
        let rem = r#"{"type":"queue-operation","operation":"remove","content":"absorbido","reason":"absorbed_mid_turn"}"#;
        assert_eq!(parse_line(rem), vec![Msg { role: "dequeued", text: "absorbido".into() }]);
        assert_eq!(parse_line(r#"{"type":"queue-operation","operation":"popAll","content":"x"}"#)[0].role, "queue-clear");
        assert!(parse_line(r#"{"type":"queue-operation","operation":"enqueue","content":"  "}"#).is_empty());
    }

    #[test]
    fn a_message_counts_as_sent_once_the_transcript_has_it() {
        let enq = r#"{"type":"queue-operation","operation":"enqueue","content":"hola mundo"}"#;
        assert!(line_has_text(enq, "hola mundo "));
        assert!(!line_has_text(enq, "otra cosa"));
        let user = r#"{"type":"user","message":{"content":[{"type":"text","text":"hola mundo"}]}}"#;
        assert!(line_has_text(user, "hola mundo"));
        assert!(!line_has_text(r#"{"type":"queue-operation","operation":"dequeue"}"#, "hola mundo"));
        assert!(!line_has_text("not json", "hola mundo"));
    }

    #[test]
    fn only_short_hex_ids_reach_a_command_line() {
        assert!(valid_id("4e9bc392"));
        assert!(!valid_id("4e9bc392; rm -rf ~"));
        assert!(!valid_id("../x"));
        assert!(!valid_id(""));
    }

    #[test]
    fn the_new_agents_id_is_found_through_the_colours() {
        let out = "backgrounded · \u{1b}[36mbbdade63\u{1b}[39m\n\u{1b}[2m  claude agents   list sessions\u{1b}[22m";
        assert_eq!(first_id(out).as_deref(), Some("bbdade63"));
        assert_eq!(first_id("nothing here"), None);
    }

    #[test]
    fn talk_becomes_messages_and_everything_else_is_dropped() {
        let user = r#"{"type":"user","message":{"role":"user","content":"hola"}}"#;
        assert_eq!(parse_line(user), vec![Msg { role: "user", text: "hola".into() }]);

        let blocks = r#"{"type":"user","message":{"content":[{"type":"text","text":"y esto"},{"type":"tool_result","content":"x"}]}}"#;
        assert_eq!(parse_line(blocks)[0].text, "y esto");

        let tool = r#"{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"..."},{"type":"text","text":"Voy"},{"type":"tool_use","name":"Edit","input":{"file_path":"src/a.ts"}}]}}"#;
        let m = parse_line(tool);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0], Msg { role: "assistant", text: "Voy".into() });
        assert_eq!(m[1], Msg { role: "tool", text: "Edit · src/a.ts".into() });

        assert!(parse_line(r#"{"type":"user","isMeta":true,"message":{"content":"x"}}"#).is_empty());
        assert!(parse_line(r#"{"type":"assistant","isSidechain":true,"message":{"content":[{"type":"text","text":"x"}]}}"#).is_empty());
        assert!(parse_line(r#"{"type":"user","message":{"content":"<command-name>/clear</command-name>"}}"#).is_empty());
        assert!(parse_line("not json").is_empty());
        assert!(parse_line(r#"{"type":"summary"}"#).is_empty());
    }

    #[test]
    fn a_half_written_record_waits_for_the_next_read() {
        let dir = std::env::temp_dir().join(format!("coucou-agents-{}", std::process::id()));
        let projects = dir.join("projects");
        let proj = projects.join("-tmp-x");
        std::fs::create_dir_all(&proj).unwrap();
        let sid = "aaaaaaaa-1111-2222-3333-444444444444";
        let file = proj.join(format!("{sid}.jsonl"));
        let first = "{\"type\":\"user\",\"message\":{\"content\":\"uno\"}}\n";
        let half = "{\"type\":\"assistant\",\"message\":{\"content\":[{\"type\":\"te";
        std::fs::write(&file, format!("{first}{half}")).unwrap();

        let a = messages_from(&projects, sid, 0).unwrap();
        assert_eq!(a.messages.len(), 1);
        assert_eq!(a.offset, first.len() as u64);

        let rest = "xt\",\"text\":\"dos\"}]}}\n";
        std::fs::write(&file, format!("{first}{half}{rest}")).unwrap();
        let b = messages_from(&projects, sid, a.offset).unwrap();
        assert_eq!(b.messages, vec![Msg { role: "assistant", text: "dos".into() }]);
        assert!(messages_from(&projects, "../../etc/passwd", 0).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
