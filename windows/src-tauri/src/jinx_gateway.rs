// Jinx through the laptop's gateway (`~/bin/jinx-chat`, jinx/jinx-assist/), the same way the phones
// reach her: one JSON request on stdin, NDJSON events on stdout. The PC writes as `src: "pc"` to the
// thread it shares with the phones and Telegram (`~/jinx-emergencia/telegram/thread.jsonl`), and the
// gateway picks the brain (Hermes when it answers, else the fallback Jinx) and mirrors to Telegram.
//
// Here the gateway runs locally — no ssh. Its answers reach the island as the same `jinx` events the
// Hermes API path sends (jinx.rs), so the island does not care which one answered:
//   { kind: "done", text, mode, mood? }    mode: hermes | or | local | cached; mood: one of EMOTES, when sent
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

/// The server whose gateway is asked first (an ssh host): `COUCOU_JINX_SERVER` ("" turns it off), else
/// `server` when `~/.ssh/config` has that host (what the deploy scripts use). None: only this laptop's.
pub fn server_host() -> Option<String> {
    if let Ok(h) = std::env::var("COUCOU_JINX_SERVER") {
        return (!h.trim().is_empty()).then(|| h.trim().to_string());
    }
    let config = std::fs::read_to_string(crate::platform::home_dir().join(".ssh").join("config")).unwrap_or_default();
    has_host(&config, "server").then(|| "server".to_string())
}

/// `Host` lines of an ssh config name `host` (one of their patterns, exactly). Pure.
pub fn has_host(config: &str, host: &str) -> bool {
    config.lines().any(|l| {
        let mut words = l.split_whitespace();
        matches!(words.next(), Some(k) if k.eq_ignore_ascii_case("host")) && words.any(|w| w == host)
    })
}

/// The gateway's command on the server (run by its shell, from the home directory): what install.sh writes there.
const SERVER_CMD: &str = "bin/jinx-chat";

/// Which way Jinx is reached, from the `jinxBackend` setting: "gateway", "hermes", or "auto" (the
/// gateway when there is one, on the server or here, as on the phones; the Hermes API otherwise).
pub fn use_gateway(setting: &str) -> bool {
    match setting {
        "hermes" => false,
        "gateway" => true,
        _ => path().is_some() || server_host().is_some(),
    }
}

// ── Server first, this laptop only when the server fails ─────────────────────────
//
// Same rules as the phone's Failover (ChatApi.kt):
//  - the server could not be reached → this laptop, and the server is skipped for DOWN;
//  - the server answered but not as Jinx's gateway (not installed there yet) → this laptop, skipped for NO_GATEWAY;
//  - the server's gateway answered → that answer, even an error;
//  - a `send` that broke after reaching the server → that error, never here (it may be in the server's thread).

const DOWN: Duration = Duration::from_secs(120);
const NO_GATEWAY: Duration = Duration::from_secs(600);

/// How one host's attempt went.
#[derive(Debug, PartialEq)]
pub enum Attempt {
    /// It printed this (it may or may not be the gateway).
    Output(String),
    /// It could not be reached at all: nothing was sent.
    Unreachable(String),
    /// It was reached, then broke (a timeout, a crash).
    Broken(String),
}

/// Whether the output is Jinx's gateway (JSON lines with "ev"). Pure.
pub fn is_gateway_output(raw: &str) -> bool {
    !parse_events(raw).is_empty()
}

/// The failover's memory: until when the server is skipped.
#[derive(Default)]
pub struct Failover {
    skip_until: Option<Instant>,
}

impl Failover {
    /// One request: the server, then (when the rules say) this laptop. Pure but for the two closures.
    pub fn ask(
        &mut self,
        op: &str,
        now: Instant,
        server: impl FnOnce() -> Attempt,
        local: impl FnOnce() -> Attempt,
    ) -> Result<String, String> {
        let here = |a: Attempt| match a {
            Attempt::Output(o) => Ok(o),
            Attempt::Unreachable(e) | Attempt::Broken(e) => Err(e),
        };
        if self.skip_until.is_some_and(|t| now < t) {
            return here(local());
        }
        match server() {
            Attempt::Output(o) if is_gateway_output(&o) => {
                self.skip_until = None;
                Ok(o)
            }
            Attempt::Output(_) => {
                self.skip_until = Some(now + NO_GATEWAY);
                here(local())
            }
            Attempt::Unreachable(_) => {
                self.skip_until = Some(now + DOWN);
                here(local())
            }
            Attempt::Broken(e) if op == "send" => Err(e),
            Attempt::Broken(_) => here(local()),
        }
    }
}

static FAILOVER: std::sync::Mutex<Failover> = std::sync::Mutex::new(Failover { skip_until: None });

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

/// The mood names Jinx's gateway may send: Mochi's emotes (the phone's `JinxMood.EMOTES`, the gateway's
/// `jinx_thread.EMOTES`). Anything else is dropped, and the island reads the mood from her text instead.
pub const EMOTES: [&str; 19] = [
    "happy", "love", "proud", "surprised", "excited", "confused", "shy", "sad", "curious", "bored", "scared",
    "grumpy", "smug", "laughing", "crying", "relieved", "determined", "flustered", "sleepy",
];

pub fn valid_emote(v: &Value) -> Option<String> {
    v.as_str().filter(|m| EMOTES.contains(m)).map(str::to_string)
}

/// A `send`'s outcome: the answer, the brain that gave it and — when the reply carried one — her mood,
/// or what went wrong.
pub fn outcome(events: &[Value]) -> Result<(String, String, Option<String>), String> {
    for ev in events.iter().rev() {
        match ev["ev"].as_str() {
            Some("done") => {
                let text = ev["text"].as_str().unwrap_or_default().to_string();
                let mode = ev["mode"].as_str().unwrap_or_default().to_string();
                return Ok((text, mode, valid_emote(&ev["mood"])));
            }
            Some("error") => return Err(ev["msg"].as_str().unwrap_or("Jinx did not answer").to_string()),
            _ => {}
        }
    }
    Err("Jinx's gateway gave no answer".into())
}

/// Runs the gateway with one request (the server's first, see [`Failover`]) and returns its events.
/// Blocking: call it off the main thread.
fn run(request: &Value, timeout: Duration) -> Result<Vec<Value>, String> {
    let op = request["op"].as_str().unwrap_or("?").to_string();
    let local = || match path() {
        Some(gateway) => attempt(Command::new(gateway), request, timeout, false),
        None => Attempt::Unreachable("Jinx's gateway is not on the server nor here (~/bin/jinx-chat; see jinx/jinx-assist/install.sh)".into()),
    };
    let raw = match server_host() {
        Some(host) => {
            let server = || {
                let mut ssh = Command::new("ssh");
                ssh.args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=5", &host, SERVER_CMD]);
                attempt(ssh, request, timeout, true)
            };
            // (The lock is held through the request: one at a time, as the gateway itself serialises them.)
            let mut f = FAILOVER.lock().unwrap_or_else(|e| e.into_inner());
            f.ask(&op, Instant::now(), server, local)?
        }
        None => match local() {
            Attempt::Output(o) => o,
            Attempt::Unreachable(e) | Attempt::Broken(e) => return Err(e),
        },
    };
    Ok(parse_events(&raw))
}

/// Runs one gateway command with the request on its stdin. Over ssh, exit 255 is ssh's own failure: the
/// server was not reached (nothing was sent).
fn attempt(mut cmd: Command, request: &Value, timeout: Duration, over_ssh: bool) -> Attempt {
    let mut child = match cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn() {
        Ok(c) => c,
        Err(e) => return Attempt::Unreachable(format!("Cannot start Jinx's gateway: {e}")),
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(format!("{request}\n").as_bytes());
    }
    let Some(mut stdout) = child.stdout.take() else {
        return Attempt::Broken("Jinx's gateway has no output".into());
    };
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = (&mut stdout).take(MAX_OUTPUT as u64).read_to_end(&mut buf);
        buf
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Attempt::Broken("Jinx took too long to answer".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(e) => return Attempt::Broken(e.to_string()),
        }
    };
    let out = String::from_utf8_lossy(&reader.join().unwrap_or_default()).into_owned();
    if over_ssh && status.code() == Some(255) && !is_gateway_output(&out) {
        return Attempt::Unreachable("the server does not answer".into());
    }
    Attempt::Output(out)
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

/// Jinx's background mood now (`op: mood`: from her `emotion_state` on the server, with the hour): an emote
/// from [`EMOTES`], or `None` when she has none or the gateway could not read her state.
pub fn background_mood() -> Result<Option<String>, String> {
    let events = run(&json!({ "op": "mood" }), HISTORY_TIMEOUT)?;
    for ev in &events {
        match ev["ev"].as_str() {
            Some("mood") => return Ok(valid_emote(&ev["emote"])),
            Some("error") => return Err(ev["msg"].as_str().unwrap_or("error").to_string()),
            _ => {}
        }
    }
    Err("Jinx's gateway gave no mood".into())
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
            Ok((text, mode, mood)) => {
                let mut done = json!({ "kind": "done", "text": text, "mode": mode });
                if let Some(mood) = mood {
                    done["mood"] = json!(mood);
                }
                done
            }
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
        assert_eq!(outcome(&events), Ok(("Hola".to_string(), "hermes".to_string(), None)));
    }

    #[test]
    fn the_mood_is_the_one_the_gateway_sent_if_it_is_a_known_emote() {
        let done = |mood: &str| parse_events(&format!("{{\"ev\":\"done\",\"mode\":\"hermes\",\"text\":\"x\"{mood}}}\n"));
        assert_eq!(outcome(&done(",\"mood\":\"laughing\"")).unwrap().2, Some("laughing".to_string()));
        // not one of the nineteen (or not a string): no mood, the island reads her text
        assert_eq!(outcome(&done(",\"mood\":\"furious\"")).unwrap().2, None);
        assert_eq!(outcome(&done(",\"mood\":\"Happy\"")).unwrap().2, None);
        assert_eq!(outcome(&done(",\"mood\":3")).unwrap().2, None);
        assert_eq!(outcome(&done(",\"mood\":null")).unwrap().2, None);
        assert_eq!(outcome(&done("")).unwrap().2, None);
        assert_eq!(EMOTES.len(), 19);
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

    const OK: &str = "{\"ev\":\"done\",\"mode\":\"hermes\",\"text\":\"hola\"}";
    const LOCAL: &str = "{\"ev\":\"done\",\"mode\":\"or\",\"text\":\"aquí\"}";

    #[test]
    fn the_server_first_and_this_laptop_only_when_it_fails() {
        let mut f = Failover::default();
        let t0 = Instant::now();
        let local = || Attempt::Output(LOCAL.into());
        // Answered by the server: this laptop is not asked.
        assert_eq!(f.ask("send", t0, || Attempt::Output(OK.into()), || panic!("asked here")), Ok(OK.into()));
        // Out of reach: here, and the server is skipped for a while.
        assert_eq!(f.ask("send", t0, || Attempt::Unreachable("x".into()), local), Ok(LOCAL.into()));
        assert_eq!(f.ask("history", t0 + DOWN - Duration::from_secs(1), || panic!("asked the server"), local), Ok(LOCAL.into()));
        assert_eq!(f.ask("history", t0 + DOWN, || Attempt::Output(OK.into()), local), Ok(OK.into()));
        // The server without the gateway (the pendientes script, a shell error): here, skipped longer.
        let t1 = t0 + DOWN;
        assert_eq!(f.ask("history", t1, || Attempt::Output("{\"ok\":false}".into()), local), Ok(LOCAL.into()));
        assert_eq!(f.ask("history", t1 + DOWN, || panic!("asked the server"), local), Ok(LOCAL.into()));
        assert_eq!(f.ask("history", t1 + NO_GATEWAY, || Attempt::Output(OK.into()), local), Ok(OK.into()));
    }

    #[test]
    fn the_gateways_own_error_is_the_answer_and_a_broken_send_is_not_sent_twice() {
        let mut f = Failover::default();
        let t = Instant::now();
        let err = "{\"ev\":\"error\",\"msg\":\"src desconocido\"}";
        assert_eq!(f.ask("send", t, || Attempt::Output(err.into()), || panic!("asked here")), Ok(err.into()));
        assert_eq!(f.ask("send", t, || Attempt::Broken("timeout".into()), || panic!("sent twice")), Err("timeout".into()));
        // A read is safe to ask here instead.
        assert_eq!(f.ask("history", t, || Attempt::Broken("timeout".into()), || Attempt::Output(LOCAL.into())), Ok(LOCAL.into()));
    }

    #[test]
    fn the_server_host_comes_from_the_ssh_config() {
        assert!(has_host("Host server\n  HostName 100.92.254.125\n", "server"));
        assert!(has_host("host laptop server\n", "server"));
        assert!(!has_host("Host servidor\n", "server"));
        assert!(!has_host("  HostName server\n", "server"));
        assert!(!has_host("", "server"));
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
             *'\"op\":\"mood\"'*) echo '{\"ev\":\"mood\",\"emote\":\"sleepy\",\"source\":\"state\",\"emotion_state\":null}' ;;\n\
             *'\"src\":\"pc\"'*) echo '{\"ev\":\"ack\"}'; echo '{\"ev\":\"done\",\"mode\":\"or\",\"text\":\"ok pc\",\"mood\":\"proud\"}' ;;\n\
             *) echo '{\"ev\":\"error\",\"msg\":\"src\"}' ;;\nesac\n",
        )
        .unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
        std::env::set_var("COUCOU_JINX_CHAT", &script);
        // Only the stand-in: no server is asked from a test.
        std::env::set_var("COUCOU_JINX_SERVER", "");
        let req = json!({ "op": "send", "text": "x", "cid": new_cid(), "src": SRC });
        let got = run(&req, Duration::from_secs(10)).and_then(|e| outcome(&e));
        let items = history(5);
        let mood = background_mood();
        std::env::remove_var("COUCOU_JINX_CHAT");
        std::env::remove_var("COUCOU_JINX_SERVER");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(got, Ok(("ok pc".to_string(), "or".to_string(), Some("proud".to_string()))));
        assert_eq!(items.unwrap()[0]["src"], "s21");
        assert_eq!(mood, Ok(Some("sleepy".to_string())));
    }
}
