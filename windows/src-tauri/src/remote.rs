// The phone as a second island. When it is turned on (Settings → Phone), what Claude Code does is also
// written, as short JSON lines, to a private Unix socket (`$XDG_RUNTIME_DIR/coucou-remote.sock`), and
// a permission request or a question can be answered from the other end.
//
// Nothing here listens on the network. The socket belongs to the user and is closed to everyone else;
// the phone reaches it through ssh, with a key that is allowed to run exactly one command
// (`coucou-hook --remote`, which only pipes its input and output to this socket). Losing the phone
// then loses the right to watch and answer Claude Code, and nothing more.
//
// What a phone may do is as narrow as what the island's own buttons do: answer a request that is
// really waiting (allow or deny), answer a question that is really waiting, or hand it back to the
// terminal. It cannot start anything, read files or run commands. Claude Code is never slowed: the
// events are copied after the island has them, and a phone that is slow or gone changes nothing.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tokio::sync::broadcast;

static ENABLED: AtomicBool = AtomicBool::new(false);
static STARTED: AtomicBool = AtomicBool::new(false);
static BUS: OnceLock<broadcast::Sender<String>> = OnceLock::new();
/// Requests waiting for a human: id → what was published for them (replayed to a phone that connects late).
static PENDING: Mutex<Option<HashMap<String, Value>>> = Mutex::new(None);

const MAX_DETAIL: usize = 300;
/// A list of tasks from the phone: twice the app's own limit (tombstones travel too).
const MAX_SYNC_TASKS: usize = crate::tasks::MAX_TASKS * 2;

fn bus() -> &'static broadcast::Sender<String> {
    BUS.get_or_init(|| broadcast::channel(256).0)
}

/// Turns the phone link on or off; the socket is served from the first time it is turned on.
pub fn sync_enabled(app: &AppHandle, on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
    if !on {
        // Anything that was still waiting for the phone is the island's alone again.
        if let Some(map) = PENDING.lock().unwrap().as_mut() {
            map.clear();
        }
    }
    #[cfg(target_os = "linux")]
    if on && !STARTED.swap(true, Ordering::SeqCst) {
        let app = app.clone();
        tauri::async_runtime::spawn(async move { serve(app).await });
    }
    #[cfg(not(target_os = "linux"))]
    let _ = app;
}

fn on() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

// ── What is published ───────────────────────────────────────────────────────────

/// The last `n` characters' worth of `s`, cut on a character boundary.
fn clip(s: &str, n: usize) -> String {
    let one_line = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= n {
        return one_line;
    }
    let mut out: String = one_line.chars().take(n.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// "coucou" for "/home/alan/dev/coucou".
fn project_of(payload: &Value) -> String {
    let cwd = payload.get("cwd").and_then(Value::as_str).unwrap_or("");
    cwd.trim_end_matches('/').rsplit('/').next().unwrap_or("").to_string()
}

/// What a tool is about to do, in a few words: the command, the file, the search.
pub fn detail_of(tool: &str, input: &Value) -> String {
    let _ = tool;
    for key in ["command", "file_path", "path", "pattern", "url", "query", "description", "prompt"] {
        if let Some(s) = input.get(key).and_then(Value::as_str) {
            if !s.trim().is_empty() {
                return clip(s, MAX_DETAIL);
            }
        }
    }
    clip(&input.to_string(), MAX_DETAIL)
}

fn base(payload: &Value) -> Value {
    json!({
        "session": payload.get("session_id").and_then(Value::as_str).unwrap_or(""),
        "project": project_of(payload),
    })
}

fn merged(mut a: Value, extra: Value) -> Value {
    if let (Some(a), Some(e)) = (a.as_object_mut(), extra.as_object()) {
        for (k, v) in e {
            a.insert(k.clone(), v.clone());
        }
    }
    a
}

/// What a plain hook event looks like to the phone (the prompt's own text never goes).
fn event_line(payload: &Value) -> Value {
    let event = payload.get("hook_event_name").and_then(Value::as_str).unwrap_or("");
    let tool = payload.get("tool_name").and_then(Value::as_str).unwrap_or("");
    let mut line = merged(base(payload), json!({ "t": "event", "event": event }));
    if !tool.is_empty() {
        line["tool"] = json!(tool);
        line["detail"] = json!(detail_of(tool, payload.get("tool_input").unwrap_or(&Value::Null)));
    }
    if event == "Notification" {
        if let Some(m) = payload.get("message").and_then(Value::as_str) {
            line["message"] = json!(clip(m, MAX_DETAIL));
        }
    }
    line
}

/// A permission request: the tool and what it wants to do.
fn approval_line(id: &str, payload: &Value, timeout_s: u64) -> Value {
    let tool = payload.get("tool_name").and_then(Value::as_str).unwrap_or("Tool");
    merged(
        base(payload),
        json!({
            "t": "approval",
            "request_id": id,
            "tool": tool,
            "detail": detail_of(tool, payload.get("tool_input").unwrap_or(&Value::Null)),
            "timeout_s": timeout_s,
        }),
    )
}

/// An AskUserQuestion: each question with its options.
fn question_line(id: &str, payload: &Value, timeout_s: u64) -> Value {
    let qs = payload
        .get("tool_input")
        .and_then(|i| i.get("questions"))
        .and_then(Value::as_array)
        .map(|qs| {
            qs.iter()
                .take(4)
                .map(|q| {
                    let options: Vec<Value> = q
                        .get("options")
                        .and_then(Value::as_array)
                        .map(|os| {
                            os.iter()
                                .take(8)
                                .map(|o| {
                                    json!({
                                        "label": clip(o.get("label").and_then(Value::as_str).unwrap_or(""), 80),
                                        "description": clip(o.get("description").and_then(Value::as_str).unwrap_or(""), 160),
                                    })
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    json!({
                        "question": clip(q.get("question").and_then(Value::as_str).unwrap_or(""), 240),
                        "header": clip(q.get("header").and_then(Value::as_str).unwrap_or(""), 40),
                        "multi": q.get("multiSelect").and_then(Value::as_bool).unwrap_or(false),
                        "options": options,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    merged(base(payload), json!({ "t": "question", "request_id": id, "questions": qs, "timeout_s": timeout_s }))
}

fn publish(line: &Value) {
    if on() {
        // No phone listening: nothing to do (a send with no receiver is an error and is ignored).
        let _ = bus().send(line.to_string());
    }
}

/// This device's own beat, pushed now (a song started, news arrived) instead of at the phone's next beat.
pub fn publish_peer() {
    if on() && bus().receiver_count() > 0 {
        publish(&crate::presence::reply_line());
    }
}

/// The task list changed on this device: a phone on the link merges it at once.
pub fn tasks_changed() {
    if on() && bus().receiver_count() > 0 {
        publish(&tasks_line());
    }
}

/// The whole list (tombstones included), in the shape both sides merge.
fn tasks_line() -> Value {
    let tasks: Value = serde_json::from_str(&crate::tasks::snapshot_json()).unwrap_or_else(|_| json!([]));
    json!({ "t": "tasks", "tasks": tasks })
}

/// A hook event that waits for nobody.
pub fn publish_event(payload: &Value) {
    if on() && bus().receiver_count() > 0 {
        publish(&event_line(payload));
    }
}

/// A request that waits for a human. Remembered until it is resolved, so a phone that connects late sees it.
pub fn publish_request(id: &str, is_question: bool, payload: &Value, timeout_s: u64) {
    if !on() {
        return;
    }
    let line = if is_question { question_line(id, payload, timeout_s) } else { approval_line(id, payload, timeout_s) };
    PENDING.lock().unwrap().get_or_insert_with(HashMap::new).insert(id.to_string(), line.clone());
    publish(&line);
}

/// The request is no longer waiting (answered here or there, declined, or timed out).
pub fn publish_resolved(id: &str, decision: Option<&str>) {
    if let Some(map) = PENDING.lock().unwrap().as_mut() {
        map.remove(id);
    }
    // A question's answer is the text of whatever was typed: only the fact that it was answered goes out.
    let word = match decision {
        Some("allow") => "allow",
        Some("deny") => "deny",
        Some(_) => "answered",
        None => "none",
    };
    publish(&json!({ "t": "resolved", "request_id": id, "decision": word }));
}

fn hello() -> Value {
    let pending: Vec<Value> = PENDING.lock().unwrap().as_ref().map(|m| m.values().cloned().collect()).unwrap_or_default();
    json!({ "t": "hello", "v": 1, "pending": pending })
}

// ── What a phone may ask ────────────────────────────────────────────────────────

/// One thing the phone asked for, once it has been checked against what is really waiting.
#[derive(Debug, PartialEq)]
pub enum Action {
    Answer { id: String, decision: &'static str },
    AnswerQuestion { id: String, answers: Value },
    Decline { id: String },
    Ping,
    /// The phone's state beat (who is using it, whether it reaches the server, what it hears and sees).
    /// State only: nothing it carries can be acted on.
    Peer { beat: crate::presence::Beat, music: bool, news: bool },
    /// The phone's whole task list, to be merged with this one (newest change wins, see tasks.rs).
    TasksSync { tasks: Value },
    Reject(String),
}

/// Reads a line from the phone. Only a request that is waiting, of the right kind, can be answered.
pub fn parse_client_line(line: &str, pending: &HashMap<String, Value>) -> Action {
    let Ok(v) = serde_json::from_str::<Value>(line) else { return Action::Reject("not JSON".into()) };
    let id = v.get("request_id").and_then(Value::as_str).unwrap_or("").to_string();
    let kind = pending.get(&id).and_then(|p| p.get("t")).and_then(Value::as_str);
    match v.get("op").and_then(Value::as_str) {
        Some("ping") => Action::Ping,
        Some("peer") => match crate::presence::parse_peer(&v) {
            Some((beat, music, news)) => Action::Peer { beat, music, news },
            None => Action::Reject("not a phone beat".into()),
        },
        Some("tasks_sync") => match v.get("tasks") {
            // Bounded here; each task is bounded again by tasks::sanitize_remote before it is merged.
            Some(t) if t.as_array().map_or(false, |a| a.len() <= MAX_SYNC_TASKS) => Action::TasksSync { tasks: t.clone() },
            _ => Action::Reject("tasks must be a list".into()),
        },
        Some("answer") => match (kind, v.get("decision").and_then(Value::as_str)) {
            (Some("approval"), Some("allow")) => Action::Answer { id, decision: "allow" },
            (Some("approval"), Some("deny")) => Action::Answer { id, decision: "deny" },
            (Some("approval"), _) => Action::Reject("decision must be allow or deny".into()),
            _ => Action::Reject("no such permission request is waiting".into()),
        },
        Some("answer_question") => match (kind, v.get("answers")) {
            (Some("question"), Some(a)) if a.is_object() => Action::AnswerQuestion { id, answers: a.clone() },
            (Some("question"), _) => Action::Reject("answers must be an object".into()),
            _ => Action::Reject("no such question is waiting".into()),
        },
        Some("decline") if kind.is_some() => Action::Decline { id },
        Some("decline") => Action::Reject("nothing is waiting under that id".into()),
        _ => Action::Reject("unknown request".into()),
    }
}

// ── The socket ──────────────────────────────────────────────────────────────────

/// The phone link's socket, next to the relay's.
#[cfg(target_os = "linux")]
pub fn socket_path() -> Option<std::path::PathBuf> {
    crate::platform::relay_socket_path().map(|p| p.with_file_name("coucou-remote.sock"))
}

#[cfg(target_os = "linux")]
async fn serve(app: AppHandle) {
    use std::os::unix::fs::PermissionsExt;
    use tokio::net::UnixListener;

    use crate::log;

    let Some(path) = socket_path() else {
        log::line("phone link: no private runtime directory");
        STARTED.store(false, Ordering::SeqCst);
        return;
    };
    if path.exists() {
        if std::os::unix::net::UnixStream::connect(&path).is_ok() {
            log::line("phone link: another Coucou already serves it");
            return;
        }
        let _ = std::fs::remove_file(&path);
    }
    let listener = match UnixListener::bind(&path) {
        Ok(l) => l,
        Err(e) => {
            log::line(format!("phone link: cannot open the socket ({e})"));
            STARTED.store(false, Ordering::SeqCst);
            return;
        }
    };
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    log::line("phone link: ready");
    let uid = unsafe { libc::getuid() };
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            continue;
        };
        if !matches!(stream.peer_cred(), Ok(c) if c.uid() == uid) {
            continue;
        }
        let app = app.clone();
        tauri::async_runtime::spawn(async move { client(app, stream).await });
    }
}

#[cfg(target_os = "linux")]
async fn client(app: AppHandle, stream: tokio::net::UnixStream) {
    use std::sync::Arc;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::unix::OwnedWriteHalf;
    use tokio::sync::Mutex as AsyncMutex;

    use crate::{log, pipe};

    let (rd, wr) = stream.into_split();
    let wr = Arc::new(AsyncMutex::new(wr));
    async fn put_line(wr: &AsyncMutex<OwnedWriteHalf>, line: &str) -> bool {
        let mut w = wr.lock().await;
        w.write_all(format!("{line}\n").as_bytes()).await.is_ok() && w.flush().await.is_ok()
    }
    async fn put(wr: &AsyncMutex<OwnedWriteHalf>, v: &Value) -> bool {
        put_line(wr, &v.to_string()).await
    }
    // Subscribed before the greeting, so nothing that happens in between is missed.
    let mut rx = bus().subscribe();
    if !on() {
        let _ = put(&wr, &json!({ "t": "disabled" })).await;
        return;
    }
    log::line("phone link: a phone connected");
    if !put(&wr, &hello()).await {
        return;
    }

    // One task writes what happens; this one reads what the phone asks. Whichever ends first ends the other.
    let writer = {
        let wr = wr.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(m) => {
                        if !put_line(&wr, &m).await {
                            break;
                        }
                    }
                    // Too slow to follow: send what is still waiting, so nothing is lost.
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        if !put(&wr, &hello()).await {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            let _ = wr.lock().await.shutdown().await;
        })
    };

    let mut lines = BufReader::new(rd).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        if !on() {
            let _ = put(&wr, &json!({ "t": "disabled" })).await;
            break;
        }
        let pending = PENDING.lock().unwrap().clone().unwrap_or_default();
        let reply = match parse_client_line(&line, &pending) {
            Action::Ping => json!({ "t": "pong" }),
            Action::Peer { beat, music, news } => {
                crate::presence::on_peer(&app, beat, music, news);
                crate::presence::reply_line()
            }
            Action::TasksSync { tasks } => {
                // Each device works alone; when they meet the newest change wins, then both have the same list.
                match crate::tasks::merge_remote_json(&tasks.to_string()) {
                    Ok(true) => {
                        let _ = app.emit("tasks-changed", ());
                        log::line("phone link: tasks merged");
                    }
                    Ok(false) => {}
                    Err(e) => log::line(format!("phone link: tasks not merged ({e})")),
                }
                tasks_line()
            }
            Action::Answer { id, decision } => {
                log::line(format!("phone link: {decision} for id={id}"));
                pipe::answer(&app, &id, decision);
                json!({ "t": "ack", "request_id": id, "ok": true })
            }
            Action::AnswerQuestion { id, answers } => {
                log::line(format!("phone link: a question answered for id={id}"));
                pipe::answer_question(&app, &id, answers);
                json!({ "t": "ack", "request_id": id, "ok": true })
            }
            Action::Decline { id } => {
                log::line(format!("phone link: handed back to the terminal id={id}"));
                pipe::decline(&app, &id);
                json!({ "t": "ack", "request_id": id, "ok": true })
            }
            Action::Reject(why) => json!({ "t": "ack", "ok": false, "why": why }),
        };
        if !put(&wr, &reply).await {
            break;
        }
    }
    writer.abort();
    // A phone that left is not there: this device shows Mochi on its own again, at once.
    crate::presence::on_link_down(&app, crate::presence::PHONE);
    log::line("phone link: the phone left");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The bus, the switch and the pending list are global: the tests that touch them go one at a time.
    static ONE_AT_A_TIME: Mutex<()> = Mutex::new(());

    fn pending(items: &[(&str, &str)]) -> HashMap<String, Value> {
        items.iter().map(|(id, t)| (id.to_string(), json!({ "t": t, "request_id": id }))).collect()
    }

    #[test]
    fn a_tool_is_summarised_by_what_it_is_about_to_do() {
        assert_eq!(detail_of("Bash", &json!({ "command": "cargo test --lib" })), "cargo test --lib");
        assert_eq!(detail_of("Edit", &json!({ "file_path": "/a/b.rs", "old_string": "x" })), "/a/b.rs");
        assert_eq!(detail_of("Grep", &json!({ "pattern": "fn main" })), "fn main");
        assert_eq!(detail_of("X", &json!({ "weird": 1 })), r#"{"weird":1}"#);
        let long = detail_of("Bash", &json!({ "command": format!("echo {}\nsecond line", "a".repeat(500)) }));
        assert_eq!(long.chars().count(), MAX_DETAIL);
        assert!(long.ends_with('…') && !long.contains('\n'));
    }

    /// The phone's own test reads this same file: what each side writes, the other reads.
    #[test]
    fn the_wire_fixtures_are_read_and_written_the_same_way_on_both_sides() {
        let doc: Value = serde_json::from_str(include_str!("../../scripts/phone/wire-fixtures.json")).unwrap();
        let none = HashMap::new();
        for key in ["peer", "tasks_sync"] {
            let line = doc["phone_to_laptop"][key].to_string();
            assert!(!matches!(parse_client_line(&line, &none), Action::Reject(_)), "{key} is read");
        }
        let keys = |v: &Value| {
            let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
            k.sort();
            k
        };
        assert_eq!(keys(&crate::presence::reply_line()), keys(&doc["laptop_to_phone"]["peer"]), "the laptop's beat has the keys the phone reads");
    }

    #[test]
    fn an_event_carries_the_project_the_tool_and_never_the_prompt() {
        let e = event_line(&json!({
            "hook_event_name": "PreToolUse", "session_id": "s1", "cwd": "/home/alan/dev/coucou/",
            "tool_name": "Bash", "tool_input": { "command": "ls" }, "prompt": "mi contraseña es 1234",
        }));
        assert_eq!(e["t"], "event");
        assert_eq!((e["project"].as_str(), e["session"].as_str(), e["tool"].as_str(), e["detail"].as_str()), (Some("coucou"), Some("s1"), Some("Bash"), Some("ls")));
        assert!(!e.to_string().contains("contraseña"));
        let n = event_line(&json!({ "hook_event_name": "Notification", "message": "Claude needs your permission" }));
        assert_eq!(n["message"], "Claude needs your permission");
        assert!(n.get("tool").is_none());
    }

    #[test]
    fn a_question_keeps_its_options_within_limits() {
        let q = question_line("9-1", &json!({
            "session_id": "s", "cwd": "/x/p",
            "tool_input": { "questions": [{ "question": "¿Cuál?", "header": "Plan", "multiSelect": true,
                "options": [{ "label": "A", "description": "primera" }, { "label": "B" }] }] },
        }), 120);
        assert_eq!(q["t"], "question");
        assert_eq!(q["questions"][0]["multi"], true);
        assert_eq!(q["questions"][0]["options"].as_array().unwrap().len(), 2);
        assert_eq!(q["questions"][0]["options"][1]["description"], "");
        assert_eq!(q["timeout_s"], 120);
    }

    #[test]
    fn a_phone_can_only_answer_what_is_waiting_in_the_right_way() {
        let p = pending(&[("a1", "approval"), ("q1", "question")]);
        let go = |l: &str| parse_client_line(l, &p);
        assert_eq!(go(r#"{"op":"answer","request_id":"a1","decision":"allow"}"#), Action::Answer { id: "a1".into(), decision: "allow" });
        assert_eq!(go(r#"{"op":"answer","request_id":"a1","decision":"deny"}"#), Action::Answer { id: "a1".into(), decision: "deny" });
        // "always" is not a phone's to grant, and anything else is refused.
        assert!(matches!(go(r#"{"op":"answer","request_id":"a1","decision":"always"}"#), Action::Reject(_)));
        assert!(matches!(go(r#"{"op":"answer","request_id":"a1"}"#), Action::Reject(_)));
        // Nothing waiting under that id, or a question answered as a permission.
        assert!(matches!(go(r#"{"op":"answer","request_id":"nope","decision":"allow"}"#), Action::Reject(_)));
        assert!(matches!(go(r#"{"op":"answer","request_id":"q1","decision":"allow"}"#), Action::Reject(_)));
        // A question needs an object of answers, and a permission cannot take them.
        assert!(matches!(go(r#"{"op":"answer_question","request_id":"q1","answers":{"¿Cuál?":"A"}}"#), Action::AnswerQuestion { .. }));
        assert!(matches!(go(r#"{"op":"answer_question","request_id":"q1","answers":"A"}"#), Action::Reject(_)));
        assert!(matches!(go(r#"{"op":"answer_question","request_id":"a1","answers":{}}"#), Action::Reject(_)));
        assert_eq!(go(r#"{"op":"decline","request_id":"q1"}"#), Action::Decline { id: "q1".into() });
        assert!(matches!(go(r#"{"op":"decline","request_id":"zz"}"#), Action::Reject(_)));
        assert_eq!(go(r#"{"op":"ping"}"#), Action::Ping);
        // A beat is state, and only the phone may send one.
        assert!(matches!(go(r#"{"op":"peer","device":"s21","idle":3,"screen_on":true,"server_ok":false,"music":true}"#), Action::Peer { music: true, .. }));
        assert!(matches!(go(r#"{"op":"peer","device":"laptop","idle":3}"#), Action::Reject(_)));
        assert!(matches!(go(r#"{"op":"peer"}"#), Action::Reject(_)));
        // Its task list is taken in as a list, and only as a bounded one.
        assert!(matches!(go(r#"{"op":"tasks_sync","tasks":[{"id":"a","title":"x"}]}"#), Action::TasksSync { .. }));
        assert!(matches!(go(r#"{"op":"tasks_sync","tasks":"x"}"#), Action::Reject(_)));
        assert!(matches!(go(r#"{"op":"tasks_sync"}"#), Action::Reject(_)));
        let too_many = format!(r#"{{"op":"tasks_sync","tasks":[{}]}}"#, vec!["{}"; MAX_SYNC_TASKS + 1].join(","));
        assert!(matches!(go(&too_many), Action::Reject(_)));
        // Anything else is not a thing a phone can do.
        for bad in [r#"{"op":"run","cmd":"ls"}"#, r#"{"op":"allow_all"}"#, "no json", "[]", ""] {
            assert!(matches!(go(bad), Action::Reject(_)), "{bad}");
        }
    }

    #[test]
    fn requests_are_remembered_until_resolved_and_a_late_phone_sees_them() {
        let _g = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        ENABLED.store(true, Ordering::Relaxed);
        publish_request("t-1", false, &json!({ "tool_name": "Bash", "tool_input": { "command": "ls" } }), 108);
        let h = hello();
        assert!(h["pending"].as_array().unwrap().iter().any(|p| p["request_id"] == "t-1"));
        publish_resolved("t-1", Some("allow"));
        assert!(!hello()["pending"].as_array().unwrap().iter().any(|p| p["request_id"] == "t-1"));
        ENABLED.store(false, Ordering::Relaxed);
    }

    #[test]
    fn a_question_answer_is_never_published() {
        let _g = ONE_AT_A_TIME.lock().unwrap_or_else(|e| e.into_inner());
        ENABLED.store(true, Ordering::Relaxed);
        let mut rx = bus().subscribe();
        publish_resolved("q-9", Some(r#"{"permissionDecision":"answer","answers":{"p":"secreto"}}"#));
        let line = rx.try_recv().unwrap();
        assert!(!line.contains("secreto"));
        assert!(line.contains("answered"));
        ENABLED.store(false, Ordering::Relaxed);
    }
}
