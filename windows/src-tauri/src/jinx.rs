// Jinx — Hermes agent over its API server, as a character in the island.
//
// The island talks to Hermes (`/v1/runs`) the way the Hermes desktop client does:
// one POST starts a run, its server-sent events stream back the reply, the tool
// calls and — the point of doing it here — the approval requests, which the island
// answers with a click. Nothing else is contacted; the address and the API key
// live in the Secret Service like every other key, never on disk.
//
// Events go to the island as `jinx`, one JSON object each:
//   { kind: "delta",    text }                      a piece of the reply
//   { kind: "tool",     name, preview }             Jinx started using a tool
//   { kind: "approval", runId, requestId, command, description }
//   { kind: "resolved", requestId }                 answered (here or elsewhere)
//   { kind: "done",     text }                      the run finished
//   { kind: "error",    message }                   it did not

use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::claude::ChatContext;
use crate::island::WINDOW_LABEL;
use crate::log;
use crate::secrets;

pub const URL_KEY: &str = "jinx-url";
pub const API_KEY: &str = "jinx-api-key";

/// Said to Jinx once per run of Coucou when the Hub is shared with her: where to look.
const HUB_HINT: &str = "[Coucou: las notas y el calendario de Alan están en ~/.hermes/state/coucou/ \
(notes.md, calendar.json, context.json), puestos al día desde su escritorio. Léelos cuando vengan al caso; \
son suyos, no los edites.]";
static HINT_SENT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The input with the hint in front of it the first time (`first` says whether it is).
pub fn with_hint(input: String, first: bool) -> String {
    if first { format!("{HUB_HINT}\n\n{input}") } else { input }
}

/// One stable session so Jinx keeps the thread between island chats.
const SESSION_ID: &str = "coucou";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(8);
/// Hermes sends a keepalive comment well inside this; silence for longer means the
/// connection is gone, not that Jinx is thinking.
const SILENCE_TIMEOUT: Duration = Duration::from_secs(150);
const MAX_FRAME: usize = 1 << 20;
/// Hermes runs on another machine and takes text: a file reaches Jinx as its contents.
const MAX_INLINE_TEXT: u64 = 200_000;
const NOT_TEXT: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "heic", "zip", "gz", "tar", "7z", "rar", "mp3", "wav",
    "mp4", "mov", "mkv", "exe", "bin", "iso", "doc", "docx", "xls", "xlsx", "ppt", "pptx",
];

/// At most `MAX_INLINE_TEXT` bytes, cut on a character boundary, with a note when cut.
fn clip(mut text: String) -> String {
    let max = MAX_INLINE_TEXT as usize;
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
    text.push_str("\n[… cut: the file is longer than Jinx is sent]");
    text
}

/// A PDF's text through poppler's `pdftotext`, which most desktops already have.
fn pdf_text(path: &str, name: &str) -> Result<String, String> {
    let out = std::process::Command::new("pdftotext")
        .args(["-layout", path, "-"])
        .output()
        .map_err(|_| "Jinx reads PDFs through pdftotext (poppler-utils), which is not installed".to_string())?;
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if !out.status.success() || text.is_empty() {
        return Err(format!("\"{name}\" has no text Jinx can read (a scan?); it can only go to Mochi"));
    }
    Ok(clip(text))
}

/// What Jinx is sent for a chat turn: the message, with the dropped file's text or
/// the window it is about in front of it. Err says why this cannot go to Jinx.
pub fn with_context(text: &str, context: Option<&ChatContext>) -> Result<String, String> {
    match context {
        None => Ok(text.to_string()),
        Some(ChatContext::Window { app_name, title, url }) => {
            let mut head = format!("Context — App: {app_name}, Window: {title}");
            if let Some(url) = url {
                head.push_str(&format!(", URL: {url}"));
            }
            Ok(format!("{head}\n\n{text}"))
        }
        Some(ChatContext::File { name, path }) => {
            let ext = std::path::Path::new(path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            let cannot = || {
                format!("Jinx can read text and code files up to {} KB; \"{name}\" can only go to Mochi", MAX_INLINE_TEXT / 1000)
            };
            if NOT_TEXT.contains(&ext.as_str()) {
                return Err(cannot());
            }
            if ext == "pdf" {
                let body = pdf_text(path, name)?;
                return Ok(format!("File: {name} (text of the PDF)\n```\n{body}\n```\n\n{text}"));
            }
            let len = std::fs::metadata(path).map_err(|e| format!("Cannot read \"{name}\": {e}"))?.len();
            if len > MAX_INLINE_TEXT {
                return Err(cannot());
            }
            let body = std::fs::read_to_string(path).map_err(|_| cannot())?;
            Ok(format!("File: {name}\n```\n{body}\n```\n\n{text}"))
        }
    }
}

#[derive(Default)]
pub struct Jinx {
    /// The run the island is following, for Stop.
    run: Mutex<Option<String>>,
}

/// `http(s)://host[:port]` without a trailing slash, or an explanation.
pub fn clean_base(raw: &str) -> Result<String, String> {
    let url = raw.trim().trim_end_matches('/');
    if !(url.starts_with("http://") || url.starts_with("https://")) || url.len() < 9 {
        return Err("Jinx address must start with http:// or https://".into());
    }
    Ok(url.to_string())
}

fn config() -> Result<(String, String), String> {
    // Debug builds only (compiled out of releases): lets the test harness point the
    // island at a stand-in server without a Secret Service in the way.
    #[cfg(debug_assertions)]
    if let (Ok(url), Ok(key)) = (
        std::env::var("COUCOU_TEST_JINX_URL"),
        std::env::var("COUCOU_TEST_JINX_KEY"),
    ) {
        return Ok((clean_base(&url)?, key));
    }
    let url = secrets::get(URL_KEY).ok_or("Jinx is not set up: add its address in Settings")?;
    let key = secrets::get(API_KEY).ok_or("Jinx is not set up: add its API key in Settings")?;
    Ok((clean_base(&url)?, key))
}

fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .connect_timeout(CONNECT_TIMEOUT)
        .build()
        .map_err(|e| e.to_string())
}

fn emit(app: &AppHandle, payload: Value) {
    let _ = app.emit_to(WINDOW_LABEL, "jinx", payload);
}

/// Hermes' error bodies are `{"error":{"message":…}}`.
fn error_text(status: reqwest::StatusCode, body: &str) -> String {
    let message = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(str::to_string));
    match (status.as_u16(), message) {
        (401, _) => "Jinx rejected the API key".into(),
        (_, Some(m)) => m,
        (code, None) => format!("Jinx answered {code}"),
    }
}

/// Is Jinx reachable and is the key accepted? Returns what to tell the user.
pub async fn test() -> Result<String, String> {
    let (base, key) = config()?;
    let client = client()?;
    let health = client
        .get(format!("{base}/health"))
        .timeout(Duration::from_secs(8))
        .send()
        .await
        .map_err(|e| format!("Cannot reach Jinx: {e}"))?;
    let version = health
        .json::<Value>()
        .await
        .ok()
        .and_then(|v| v["version"].as_str().map(str::to_string))
        .unwrap_or_default();
    let models = client
        .get(format!("{base}/v1/models"))
        .bearer_auth(&key)
        .timeout(Duration::from_secs(8))
        .send()
        .await
        .map_err(|e| format!("Cannot reach Jinx: {e}"))?;
    if !models.status().is_success() {
        let status = models.status();
        let body = models.text().await.unwrap_or_default();
        return Err(error_text(status, &body));
    }
    Ok(if version.is_empty() { "Connected".into() } else { format!("Connected — Hermes {version}") })
}

/// Starts a run and follows it in the background.
pub async fn send(app: AppHandle, jinx: &Jinx, text: String, context: Option<ChatContext>, shared: bool) -> Result<(), String> {
    let input = with_context(&text, context.as_ref())?;
    let input = with_hint(input, shared && !HINT_SENT.load(std::sync::atomic::Ordering::Relaxed));
    let (base, key) = config()?;
    let client = client()?;
    let response = client
        .post(format!("{base}/v1/runs"))
        .bearer_auth(&key)
        .json(&json!({ "input": input, "session_id": SESSION_ID }))
        .timeout(Duration::from_secs(20))
        .send()
        .await
        .map_err(|e| format!("Cannot reach Jinx: {e}"))?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(error_text(status, &body));
    }
    let run_id = serde_json::from_str::<Value>(&body)
        .ok()
        .and_then(|v| v["run_id"].as_str().or_else(|| v["id"].as_str()).map(str::to_string))
        .ok_or("Jinx did not say which run it started")?;
    if shared {
        HINT_SENT.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    *jinx.run.lock().unwrap() = Some(run_id.clone());
    log::line("jinx run started".to_string());
    tauri::async_runtime::spawn(follow(app, base, key, run_id));
    Ok(())
}

pub async fn approve(run_id: &str, request_id: &str, choice: &str) -> Result<(), String> {
    if !matches!(choice, "once" | "session" | "always" | "deny") {
        return Err("unknown approval choice".into());
    }
    let (base, key) = config()?;
    let mut body = json!({ "choice": choice });
    if !request_id.is_empty() {
        body["request_id"] = json!(request_id);
    }
    let response = client()?
        .post(format!("{base}/v1/runs/{run_id}/approval"))
        .bearer_auth(&key)
        .json(&body)
        .timeout(Duration::from_secs(15))
        .send()
        .await
        .map_err(|e| format!("Cannot reach Jinx: {e}"))?;
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    let text = response.text().await.unwrap_or_default();
    Err(error_text(status, &text))
}

pub async fn stop(jinx: &Jinx) -> Result<(), String> {
    let Some(run_id) = jinx.run.lock().unwrap().clone() else { return Ok(()) };
    let (base, key) = config()?;
    let _ = client()?
        .post(format!("{base}/v1/runs/{run_id}/stop"))
        .bearer_auth(&key)
        .timeout(Duration::from_secs(10))
        .send()
        .await;
    Ok(())
}

/// Splits complete SSE frames (`…\n\n`) off the front of `buf`; each frame's
/// `data:` lines are one JSON object. Comments (keepalives) and junk are skipped.
fn take_frames(buf: &mut Vec<u8>) -> Vec<Value> {
    let mut out = Vec::new();
    while let Some(end) = buf.windows(2).position(|w| w == b"\n\n") {
        let frame: Vec<u8> = buf.drain(..end + 2).collect();
        let text = String::from_utf8_lossy(&frame);
        let data: Vec<&str> = text
            .lines()
            .filter_map(|l| l.strip_prefix("data:"))
            .map(|l| l.strip_prefix(' ').unwrap_or(l))
            .collect();
        if data.is_empty() {
            continue;
        }
        if let Ok(v) = serde_json::from_str::<Value>(&data.join("\n")) {
            out.push(v);
        }
    }
    out
}

/// What the island is told about one Hermes event; None for events it ignores.
fn translate(run_id: &str, ev: &Value) -> Option<Value> {
    let s = |k: &str| ev[k].as_str().unwrap_or("").to_string();
    Some(match ev["event"].as_str()? {
        "message.delta" => json!({ "kind": "delta", "text": s("delta") }),
        "tool.started" => json!({ "kind": "tool", "name": s("tool_name"), "preview": s("preview") }),
        "approval.request" => json!({
            "kind": "approval",
            "runId": run_id,
            "requestId": s("request_id"),
            "command": s("command"),
            "description": s("description"),
        }),
        "approval.responded" => json!({ "kind": "resolved", "requestId": s("request_id") }),
        "run.completed" => json!({ "kind": "done", "text": s("output") }),
        "run.failed" => json!({
            "kind": "error",
            "message": ev["error"].as_str().unwrap_or("Jinx could not finish").to_string(),
        }),
        "run.cancelled" | "run.interrupted" => json!({ "kind": "error", "message": "Stopped" }),
        _ => return None,
    })
}

async fn follow(app: AppHandle, base: String, key: String, run_id: String) {
    let result = stream(&app, &base, &key, &run_id).await;
    if let Err(message) = result {
        emit(&app, json!({ "kind": "error", "message": message }));
    }
}

async fn stream(app: &AppHandle, base: &str, key: &str, run_id: &str) -> Result<(), String> {
    let mut response = client()?
        .get(format!("{base}/v1/runs/{run_id}/events"))
        .bearer_auth(key)
        .header("Accept", "text/event-stream")
        .send()
        .await
        .map_err(|e| format!("Cannot reach Jinx: {e}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(error_text(status, &body));
    }
    let mut buf: Vec<u8> = Vec::new();
    let mut finished = false;
    loop {
        let chunk = tokio::time::timeout(SILENCE_TIMEOUT, response.chunk())
            .await
            .map_err(|_| "Lost the connection to Jinx".to_string())?
            .map_err(|e| format!("Lost the connection to Jinx: {e}"))?;
        let Some(chunk) = chunk else { break };
        buf.extend_from_slice(&chunk);
        if buf.len() > MAX_FRAME {
            return Err("Jinx sent an event too large to read".into());
        }
        for ev in take_frames(&mut buf) {
            if let Some(payload) = translate(run_id, &ev) {
                if matches!(payload["kind"].as_str(), Some("done") | Some("error")) {
                    finished = true;
                }
                emit(app, payload);
            }
        }
        if finished {
            return Ok(());
        }
    }
    if finished {
        Ok(())
    } else {
        Err("Jinx closed the connection before finishing".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_are_split_on_blank_lines_and_partial_ones_wait() {
        let mut buf = b": keepalive\n\ndata: {\"event\":\"message.delta\",\"delta\":\"Ho\"}\n\ndata: {\"event\":\"mess".to_vec();
        let frames = take_frames(&mut buf);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0]["delta"], "Ho");
        buf.extend_from_slice(b"age.delta\",\"delta\":\"la\"}\n\n");
        let frames = take_frames(&mut buf);
        assert_eq!(frames[0]["delta"], "la");
        assert!(buf.is_empty());
    }

    #[test]
    fn multibyte_text_split_across_chunks_survives() {
        let full = "data: {\"event\":\"message.delta\",\"delta\":\"año ✓\"}\n\n".as_bytes();
        let cut = full.iter().position(|b| *b == 0xC3).unwrap() + 1; // inside "ñ"
        let mut buf = full[..cut].to_vec();
        assert!(take_frames(&mut buf).is_empty());
        buf.extend_from_slice(&full[cut..]);
        assert_eq!(take_frames(&mut buf)[0]["delta"], "año ✓");
    }

    #[test]
    fn events_become_what_the_island_understands() {
        let ev = json!({"event":"approval.request","request_id":"r1","command":"rm -rf x","description":"delete"});
        let p = translate("run9", &ev).unwrap();
        assert_eq!(p["kind"], "approval");
        assert_eq!(p["runId"], "run9");
        assert_eq!(p["requestId"], "r1");
        assert_eq!(p["command"], "rm -rf x");
        assert_eq!(translate("r", &json!({"event":"run.completed","output":"hola"})).unwrap()["text"], "hola");
        assert_eq!(translate("r", &json!({"event":"run.failed"})).unwrap()["kind"], "error");
        assert!(translate("r", &json!({"event":"run.started"})).is_none());
    }

    #[test]
    fn a_text_file_travels_as_its_contents_and_anything_else_is_refused_plainly() {
        let dir = std::env::temp_dir().join(format!("coucou-jinx-ctx-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let note = dir.join("nota.txt");
        std::fs::write(&note, "compra leche").unwrap();
        let ctx = ChatContext::File { name: "nota.txt".into(), path: note.to_string_lossy().into() };
        let sent = with_context("¿qué dice?", Some(&ctx)).unwrap();
        assert!(sent.contains("compra leche") && sent.ends_with("¿qué dice?"));

        let png = ChatContext::File { name: "a.png".into(), path: dir.join("a.png").to_string_lossy().into() };
        assert!(with_context("x", Some(&png)).unwrap_err().contains("only go to Mochi"));
        assert_eq!(clip("ñ".repeat(150_000)).len() < 300_100, true);
        assert_eq!(clip("corto".into()), "corto");

        let big = dir.join("big.txt");
        std::fs::write(&big, vec![b'a'; 200_001]).unwrap();
        let big = ChatContext::File { name: "big.txt".into(), path: big.to_string_lossy().into() };
        assert!(with_context("x", Some(&big)).is_err());

        let win = ChatContext::Window { app_name: "Brave".into(), title: "Docs".into(), url: Some("https://x.dev".into()) };
        assert!(with_context("hola", Some(&win)).unwrap().starts_with("Context — App: Brave, Window: Docs, URL: https://x.dev"));
        assert_eq!(with_context("hola", None).unwrap(), "hola");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_hint_goes_in_front_once() {
        let first = with_hint("hola".into(), true);
        assert!(first.starts_with("[Coucou:") && first.ends_with("\n\nhola"));
        assert_eq!(with_hint("hola".into(), false), "hola");
    }

    #[test]
    fn the_address_is_cleaned_and_must_be_http() {
        assert_eq!(clean_base(" http://100.1.2.3:8642/ ").unwrap(), "http://100.1.2.3:8642");
        assert!(clean_base("100.1.2.3:8642").is_err());
        assert!(clean_base("file:///etc/passwd").is_err());
    }

    #[test]
    fn errors_say_what_hermes_said() {
        let body = r#"{"error":{"message":"Run has no pending approval"}}"#;
        assert_eq!(error_text(reqwest::StatusCode::CONFLICT, body), "Run has no pending approval");
        assert_eq!(error_text(reqwest::StatusCode::UNAUTHORIZED, body), "Jinx rejected the API key");
        assert_eq!(error_text(reqwest::StatusCode::BAD_GATEWAY, ""), "Jinx answered 502");
    }
}
