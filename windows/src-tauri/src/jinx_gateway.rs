// Jinx through the laptop's gateway (`~/bin/jinx-chat`, jinx/jinx-assist/), the same way the phones
// reach her: one JSON request on stdin, NDJSON events on stdout. The PC writes as `src: "pc"` to the
// thread it shares with the phones and Telegram (`~/jinx-emergencia/telegram/thread.jsonl`), and the
// gateway picks the brain (Hermes when it answers, else the fallback Jinx) and mirrors to Telegram.
//
// Here the gateway runs locally — no ssh. Its answers reach the island as the same `jinx` events the
// Hermes API path sends (jinx.rs), so the island does not care which one answered:
//   { kind: "done", text, mode }    mode: hermes | or | local | cached
//   { kind: "error", message }

use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};

use crate::island::WINDOW_LABEL;
use crate::log;

/// What the gateway calls this computer in the thread (it also allows "s21" and "s22").
pub const SRC: &str = "pc";
/// A reply may go through Hermes over ssh (85 s) and then the fallback brain: give it room.
const SEND_TIMEOUT: Duration = Duration::from_secs(150);
const HISTORY_TIMEOUT: Duration = Duration::from_secs(20);
/// The gateway prints a few lines; anything this long is not it.
const MAX_OUTPUT: usize = 4 << 20;

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Where the gateway is: `COUCOU_JINX_CHAT`, else `~/bin/jinx-chat` (what install.sh writes).
pub fn path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("COUCOU_JINX_CHAT").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    let p = crate::platform::home_dir().join("bin").join("jinx-chat");
    p.is_file().then_some(p)
}

/// Which way Jinx is reached, from the `jinxBackend` setting: "gateway", "hermes", or "auto" (the
/// gateway when it is installed, as on the phones; the Hermes API otherwise).
pub fn use_gateway(setting: &str) -> bool {
    match setting {
        "hermes" => false,
        "gateway" => true,
        _ => path().is_some(),
    }
}

/// A message id the gateway can deduplicate a retry by. Unique per process run and message.
pub fn new_cid() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    format!("pc-{nanos:x}-{:x}-{}", std::process::id(), COUNTER.fetch_add(1, Ordering::Relaxed))
}

/// The JSON objects in the gateway's output, one per line; anything else is skipped.
pub fn parse_events(out: &str) -> Vec<Value> {
    out.lines()
        .filter_map(|l| serde_json::from_str::<Value>(l.trim()).ok())
        .filter(|v| v.get("ev").and_then(Value::as_str).is_some())
        .collect()
}

/// A `send`'s outcome: the answer and the brain that gave it, or what went wrong.
pub fn outcome(events: &[Value]) -> Result<(String, String), String> {
    for ev in events.iter().rev() {
        match ev["ev"].as_str() {
            Some("done") => {
                let text = ev["text"].as_str().unwrap_or_default().to_string();
                let mode = ev["mode"].as_str().unwrap_or_default().to_string();
                return Ok((text, mode));
            }
            Some("error") => return Err(ev["msg"].as_str().unwrap_or("Jinx did not answer").to_string()),
            _ => {}
        }
    }
    Err("Jinx's gateway gave no answer".into())
}

/// Runs the gateway with one request and returns its events. Blocking: call it off the main thread.
fn run(request: &Value, timeout: Duration) -> Result<Vec<Value>, String> {
    let gateway = path().ok_or("Jinx's gateway is not installed (~/bin/jinx-chat; see jinx/jinx-assist/install.sh)")?;
    let mut child = Command::new(&gateway)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("Cannot start Jinx's gateway: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(format!("{request}\n").as_bytes());
    }
    let mut stdout = child.stdout.take().ok_or("Jinx's gateway has no output")?;
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = (&mut stdout).take(MAX_OUTPUT as u64).read_to_end(&mut buf);
        buf
    });
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Jinx took too long to answer".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Err(e.to_string()),
        }
    }
    let out = reader.join().unwrap_or_default();
    Ok(parse_events(&String::from_utf8_lossy(&out)))
}

/// The tail of the shared thread: `{id, ts, src, text, …}`, oldest first.
pub fn history(n: u32) -> Result<Vec<Value>, String> {
    let events = run(&json!({ "op": "history", "n": n.clamp(1, 500) }), HISTORY_TIMEOUT)?;
    for ev in &events {
        match ev["ev"].as_str() {
            Some("history") => return Ok(ev["items"].as_array().cloned().unwrap_or_default()),
            Some("error") => return Err(ev["msg"].as_str().unwrap_or("error").to_string()),
            _ => {}
        }
    }
    Err("Jinx's gateway gave no history".into())
}

/// Is the gateway there and answering? What to tell the user.
pub fn test() -> Result<String, String> {
    let lines = history(500)?.len();
    Ok(format!("Connected — gateway, shared thread with {lines} messages"))
}

/// Sends `input` (already with its context) and reports the answer as `jinx` events.
pub fn send(app: AppHandle, input: String) {
    let cid = new_cid();
    log::line("jinx: message through the gateway".to_string());
    tauri::async_runtime::spawn_blocking(move || {
        let request = json!({ "op": "send", "text": input, "cid": cid, "src": SRC });
        let payload = match run(&request, SEND_TIMEOUT).and_then(|events| outcome(&events)) {
            Ok((text, mode)) => json!({ "kind": "done", "text": text, "mode": mode }),
            Err(message) => {
                log::line("jinx: the gateway did not answer".to_string());
                json!({ "kind": "error", "message": message })
            }
        };
        let _ = app.emit_to(WINDOW_LABEL, "jinx", payload);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_answer_is_the_done_event() {
        let out = "{\"ev\":\"ack\"}\nnoise\n{\"ev\":\"done\",\"mode\":\"hermes\",\"text\":\"Hola\",\"mirrored\":true}\n";
        let events = parse_events(out);
        assert_eq!(events.len(), 2);
        assert_eq!(outcome(&events), Ok(("Hola".to_string(), "hermes".to_string())));
    }

    #[test]
    fn an_error_or_silence_is_said_as_such() {
        let events = parse_events("{\"ev\":\"ack\"}\n{\"ev\":\"error\",\"msg\":\"no pude generar respuesta\"}\n");
        assert_eq!(outcome(&events), Err("no pude generar respuesta".to_string()));
        assert!(outcome(&parse_events("{\"ev\":\"ack\"}\n")).is_err());
        assert!(outcome(&parse_events("")).is_err());
        // A JSON line without `ev` is not an event.
        assert!(parse_events("{\"x\":1}\n").is_empty());
    }

    #[test]
    fn message_ids_never_repeat() {
        let a = new_cid();
        let b = new_cid();
        assert_ne!(a, b);
        assert!(a.starts_with("pc-"));
    }

    #[test]
    fn the_setting_picks_the_way() {
        assert!(!use_gateway("hermes"));
        assert!(use_gateway("gateway"));
    }

    /// The real round trip, against a stand-in gateway script.
    #[cfg(unix)]
    #[test]
    fn a_stand_in_gateway_answers_a_send_and_a_history() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("coucou-gw-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let script = dir.join("jinx-chat");
        std::fs::write(
            &script,
            "#!/bin/sh\nread line\ncase \"$line\" in\n\
             *'\"op\": \"history\"'*|*'\"op\":\"history\"'*) echo '{\"ev\":\"history\",\"items\":[{\"id\":\"a\",\"ts\":1,\"src\":\"s21\",\"text\":\"hola\"}]}' ;;\n\
             *'\"src\":\"pc\"'*) echo '{\"ev\":\"ack\"}'; echo '{\"ev\":\"done\",\"mode\":\"or\",\"text\":\"ok pc\"}' ;;\n\
             *) echo '{\"ev\":\"error\",\"msg\":\"src\"}' ;;\nesac\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("COUCOU_JINX_CHAT", &script);
        let req = json!({ "op": "send", "text": "x", "cid": new_cid(), "src": SRC });
        let got = run(&req, Duration::from_secs(10)).and_then(|e| outcome(&e));
        let items = history(5);
        std::env::remove_var("COUCOU_JINX_CHAT");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(got, Ok(("ok pc".to_string(), "or".to_string())));
        assert_eq!(items.unwrap()[0]["src"], "s21");
    }
}
