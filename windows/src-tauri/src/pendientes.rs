// The link between the Hub's tasks and Jinx's pendientes (her list, ~/.hermes/state/pendientes.json
// on the server). What Alan tells Jinx shows up in the Tasks tab, and what he creates there is
// handed to her list, so her briefings and nudges know about it; finishing it on either side
// finishes it on the other.
//
// Coucou never edits her file. Reading is a plain `cat`; every change goes through her own
// module (`pendientes_store`: its `add` and `resolve`), run on the server by a FIXED script. The
// request travels inside that script as base64, so nothing the user typed is ever part of a
// command line, and each request is checked before it is sent.

use std::process::{Command, Stdio};
use std::sync::Mutex;

use serde::Serialize;
use serde_json::{json, Value};

use crate::tasks::{self, JinxRow, Task};

static SYNC: Mutex<()> = Mutex::new(());

/// Her pendientes file, relative to the server's home.
const FILE: &str = ".hermes/state/pendientes.json";

const SCRIPT: &str = r#"
import base64, json, os, sys
sys.path.insert(0, os.path.expanduser("~/.hermes/plugins/pendientes"))
import pendientes_store as ps
req = json.loads(base64.b64decode("__DATA__").decode("utf-8"))
op = req["op"]
if op == "add":
    out = ps.add(req["titulo"], due=req.get("due"), categoria="personal", source="coucou",
                 source_id=req["source_id"], nota=req.get("nota", ""))
elif op == "resolve":
    out = ps.resolve(req["id"], req["accion"])
else:
    out = {"error": "unknown op"}
print(json.dumps(out, ensure_ascii=False))
"#;

// ── Reading her list ───────────────────────────────────────────────────────────

/// Every row of her file that has an id and a title (open or closed: the sync needs both).
pub fn parse_rows(json: &str) -> Vec<JinxRow> {
    let Ok(v) = serde_json::from_str::<Value>(json) else { return Vec::new() };
    v["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|i| {
                    Some(JinxRow {
                        id: i["id"].as_str()?.to_string(),
                        titulo: i["titulo"].as_str()?.to_string(),
                        due: i["due"].as_str().map(str::to_string),
                        categoria: i["categoria"].as_str().map(str::to_string),
                        source: i["source"].as_str().map(str::to_string),
                        status: i["status"].as_str().unwrap_or("pendiente").to_string(),
                        // Her gateway v2 says when the row last changed; older ones do not.
                        updated_at: i["updated_at"].as_str().and_then(crate::tasks::iso_to_unix),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

// ── Asking her store to change something ───────────────────────────────────────

fn plain_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10 && b[4] == b'-' && b[7] == b'-' && b.iter().enumerate().all(|(i, c)| i == 4 || i == 7 || c.is_ascii_digit())
}

/// Only these requests, only in this shape, are ever sent.
fn validate(req: &Value) -> Result<(), String> {
    let text_ok = |v: &Value, max: usize| v.as_str().is_some_and(|s| !s.trim().is_empty() && s.chars().count() <= max && !s.contains('\0'));
    match req["op"].as_str() {
        Some("add") => {
            let id_ok = req["source_id"].as_str().is_some_and(|s| !s.is_empty() && s.len() <= 40 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'));
            let due_ok = req["due"].is_null() || req["due"].as_str().is_some_and(plain_date);
            let nota_ok = req["nota"].is_null() || req["nota"].as_str().is_some_and(|s| s.chars().count() <= 120 && !s.contains('\0'));
            if text_ok(&req["titulo"], tasks::MAX_TITLE) && id_ok && due_ok && nota_ok {
                Ok(())
            } else {
                Err("not a valid pendiente".into())
            }
        }
        Some("resolve") => {
            let id_ok = req["id"].as_str().is_some_and(|s| !s.is_empty() && s.len() <= 16 && s.bytes().all(|b| b.is_ascii_hexdigit()));
            let action_ok = matches!(req["accion"].as_str(), Some("hecho" | "descartado" | "reabrir"));
            if id_ok && action_ok {
                Ok(())
            } else {
                Err("not a valid change".into())
            }
        }
        _ => Err("unknown request".into()),
    }
}

fn base64(bytes: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        out.push(T[(n >> 18) as usize & 63] as char);
        out.push(T[(n >> 12) as usize & 63] as char);
        out.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        out.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    out
}

/// The script that carries `req` (validated by the caller).
fn script_for(req: &Value) -> String {
    SCRIPT.replace("__DATA__", &base64(req.to_string().as_bytes()))
}

/// Runs one change on the server, over the shared ssh connection. Returns what her store answered.
pub fn request(host: &str, req: &Value) -> Result<Value, String> {
    run(host, "PYTHONUTF8=1 python3 -", req)
}

/// `remote` is the fixed command that runs the script read from stdin.
fn run(host: &str, remote: &str, req: &Value) -> Result<Value, String> {
    if !crate::sysmon::safe_word(host) {
        return Err("The server's name is not valid".into());
    }
    validate(req)?;
    let mut child = Command::new("ssh")
        .args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=6", "-o", "ControlMaster=auto", "-o", "ControlPersist=120"])
        .args(["-o", &format!("ControlPath={}", crate::sysmon::control_path())])
        .arg(host)
        .arg(remote)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "ssh is not installed".to_string())?;
    {
        use std::io::Write;
        let mut stdin = child.stdin.take().ok_or("no stdin")?;
        stdin.write_all(script_for(req).as_bytes()).map_err(|e| e.to_string())?;
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("Jinx could not be reached on {host}"));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let last = text.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
    let v: Value = serde_json::from_str(last).map_err(|_| "Jinx's answer was not understood".to_string())?;
    match v["error"].as_str() {
        Some(e) => Err(e.to_string()),
        None => Ok(v),
    }
}

/// Closes (or dismisses) one of her pendientes.
pub fn resolve(host: &str, id: &str, action: &str) -> Result<(), String> {
    request(host, &json!({ "op": "resolve", "id": id, "accion": action })).map(|_| ())
}

fn when_note(remind_at: Option<i64>) -> (Option<String>, String) {
    let Some(t) = remind_at else { return (None, "Coucou".into()) };
    let off = crate::pomodoro::local_offset(t);
    let local = t + off;
    let secs = local.rem_euclid(86_400);
    (Some(tasks::civil_date(local.div_euclid(86_400))), format!("Coucou · {:02}:{:02}", secs / 3600, secs % 3600 / 60))
}

/// Hands one local task to her list (an upsert on the task's id: asking twice makes one pendiente).
fn push(host: &str, t: &Task) -> Result<String, String> {
    let (due, nota) = when_note(t.remind_at);
    let v = request(host, &json!({ "op": "add", "titulo": t.title, "due": due, "nota": nota, "source_id": t.id }))?;
    v["id"].as_str().map(str::to_string).ok_or_else(|| "Jinx gave no id".to_string())
}

// ── The sync ───────────────────────────────────────────────────────────────────

#[derive(Serialize, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub tasks: Vec<Task>,
    /// Her open pendientes that are not ours (what Alan told her).
    pub jinx: Vec<JinxRow>,
    /// Why she could not be reached (the local list is right anyway).
    pub error: Option<String>,
}

/// One round: read her list, bring ours in step (both ways), then show what is left of hers.
pub fn sync(host: &str) -> SyncResult {
    // Two rounds at once would hand the same task over twice.
    let Ok(_guard) = SYNC.try_lock() else {
        return SyncResult { tasks: tasks::load(), jinx: Vec::new(), error: Some("busy".into()) };
    };
    let read = || crate::sysmon::remote_cat(host, FILE).map(|t| parse_rows(&t));
    let rows = match read() {
        Ok(r) => r,
        Err(e) => return SyncResult { tasks: tasks::load(), jinx: Vec::new(), error: Some(e) },
    };
    let mut plan = tasks::Plan::default();
    let mut error = None;
    let updated = tasks::update(|l| {
        // One list on every device: every task is shared.
        l.iter_mut().for_each(|t| t.jinx = true);
        plan = tasks::reconcile(l, &rows, tasks::now());
        Ok(())
    });
    if let Err(e) = updated {
        return SyncResult { tasks: tasks::load(), jinx: Vec::new(), error: Some(e) };
    }
    let mut wrote = false;
    for id in &plan.to_add {
        if let Some(t) = tasks::load().into_iter().find(|t| &t.id == id) {
            match push(host, &t) {
                Ok(jid) => {
                    wrote = true;
                    let _ = tasks::update(|l| {
                        if let Some(x) = l.iter_mut().find(|x| x.id == t.id) {
                            x.jinx_id = Some(jid);
                        }
                        Ok(())
                    });
                }
                Err(e) => error = Some(e),
            }
        }
    }
    for (jid, action) in &plan.to_resolve {
        match resolve(host, jid, action) {
            Ok(()) => wrote = true,
            Err(e) => error = Some(e),
        }
    }
    // What was just changed is read again, so a closed pendiente does not flash back.
    let rows = if wrote { read().unwrap_or(rows) } else { rows };
    // Hers become ours: the same kind of task, with the same actions, marked with who made them.
    let now = tasks::now();
    let _ = tasks::update(|l| {
        tasks::import_unlinked(l, &rows, now, &crate::pomodoro::local_offset);
        Ok(())
    });
    let list = tasks::load();
    SyncResult { jinx: tasks::unlinked_open(&list, &rows), tasks: list, error }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn her_file_gives_every_row_with_an_id_and_a_title() {
        let json = r#"{"items":[
          {"id":"a1","titulo":"pagar luz","due":"2026-10-06","categoria":"admin","source":"manual","status":"pendiente"},
          {"id":"a2","titulo":"vieja","due":null,"status":"hecho","source":"teams"},
          {"titulo":"sin id"},{"id":"a3"}
        ]}"#;
        let rows = parse_rows(json);
        assert_eq!(rows.iter().map(|r| (r.id.as_str(), r.open())).collect::<Vec<_>>(), [("a1", true), ("a2", false)]);
        assert_eq!(rows[0].due.as_deref(), Some("2026-10-06"));
        assert!(parse_rows("not json").is_empty());
    }

    #[test]
    fn base64_matches_the_standard_alphabet_and_padding() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64("llamar a mamá ✓".as_bytes()), "bGxhbWFyIGEgbWFtw6Eg4pyT");
    }

    #[test]
    fn only_well_formed_requests_are_sent() {
        let add = |t: Value, due: Value, sid: &str| json!({ "op": "add", "titulo": t, "due": due, "nota": "Coucou", "source_id": sid });
        assert!(validate(&add(json!("llamar a mamá"), json!("2026-10-06"), "6ac5209e-0")).is_ok());
        assert!(validate(&add(json!("x"), Value::Null, "abc")).is_ok());
        assert!(validate(&add(json!("  "), Value::Null, "abc")).is_err(), "no title");
        assert!(validate(&add(json!("x"), json!("mañana"), "abc")).is_err(), "a date is a date");
        assert!(validate(&add(json!("x"), json!("2026-10-06; rm"), "abc")).is_err());
        assert!(validate(&add(json!("x"), Value::Null, "a b")).is_err(), "an id is plain");
        assert!(validate(&add(json!("x"), Value::Null, "")).is_err());
        assert!(validate(&json!({"op":"resolve","id":"8e5eeae1","accion":"hecho"})).is_ok());
        assert!(validate(&json!({"op":"resolve","id":"8e5eeae1","accion":"borrar"})).is_err());
        assert!(validate(&json!({"op":"resolve","id":"zz","accion":"hecho"})).is_err(), "ids are hex");
        assert!(validate(&json!({"op":"eval","code":"1"})).is_err());
    }

    #[test]
    fn what_was_typed_never_reaches_the_script_as_code() {
        // A title full of quotes and newlines travels only inside the base64 text.
        let req = json!({ "op": "add", "titulo": "\"\"\"; import os; os.system('x') #\n'''", "due": null, "nota": "n", "source_id": "t1" });
        let script = script_for(&req);
        assert!(!script.contains("os.system"), "the request is encoded");
        assert!(!script.contains("__DATA__"));
        let data = script.split("b64decode(\"").nth(1).unwrap().split('"').next().unwrap();
        assert!(data.bytes().all(|b| b.is_ascii_alphanumeric() || b"+/=".contains(&b)));
    }

    /// Her REAL store code on the server, but with its data in a throw-away folder (HERMES_HOME), so
    /// her list is never touched. By hand: `cargo test --lib real_store_roundtrip -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_store_roundtrip() {
        let host = "server";
        let remote = "mkdir -p /tmp/coucou-pend-test/state && HERMES_HOME=/tmp/coucou-pend-test PYTHONUTF8=1 python3 -";
        let add = json!({ "op": "add", "titulo": "llamar a mamá \"hoy\" ✓", "due": "2026-10-06", "nota": "Coucou · 17:30", "source_id": "t-rt-1" });
        let a = run(host, remote, &add).expect("add");
        println!("add: {a}");
        assert_eq!(a["status"], "creado");
        let id = a["id"].as_str().unwrap().to_string();
        // Asking again is an upsert on the task's id, not a second pendiente.
        let again = run(host, remote, &add).expect("add again");
        assert_eq!(again["id"], a["id"]);
        assert_eq!(again["status"], "actualizado");
        let done = run(host, remote, &json!({ "op": "resolve", "id": id, "accion": "hecho" })).expect("resolve");
        println!("resolve: {done}");
        assert_eq!(done["status"], "hecho");
        let missing = run(host, remote, &json!({ "op": "resolve", "id": "deadbeef", "accion": "hecho" })).expect("resolve missing");
        assert_eq!(missing["status"], "no_encontrado");
        let _ = Command::new("ssh").args([host, "cat /tmp/coucou-pend-test/state/pendientes.json | head -30; rm -rf /tmp/coucou-pend-test"]).status();
    }

    #[test]
    fn a_reminder_becomes_her_date_and_a_note_with_the_time() {
        let off = crate::pomodoro::local_offset(0);
        // 2026-10-06 17:30 local, whatever this computer's zone is.
        let t = 20_732 * 86_400 + 17 * 3600 + 30 * 60 - off;
        assert_eq!(when_note(Some(t)), (Some("2026-10-06".into()), "Coucou · 17:30".into()));
        assert_eq!(when_note(None), (None, "Coucou".into()));
    }
}
