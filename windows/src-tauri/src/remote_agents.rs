// Claude Code agents that run on the home server, shown in Coucou next to the local ones. Read-only: you see
// them, what they say and whether one is waiting; you answer or stop them where they run.
//
// Same rules as activities.rs: one fixed script (remote_agents.py) on stdin over the shared ssh connection,
// nothing installed on the server, and the only things that ever reach its command line are a session id
// (hex and dashes, checked) and a byte offset. Ids are prefixed `srv:` so they never collide with a local
// session, and the transcript is read with the same parser the local ones use (claude_sessions.rs).
// Off with "Server activity" in Settings, and only with a server set.

use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

use crate::claude_sessions::{self, Messages};

const SCRIPT: &str = include_str!("remote_agents.py");
pub const PREFIX: &str = "srv:";
const MAX_AGENTS: usize = 30;

pub fn is_remote(id: &str) -> bool {
    id.starts_with(PREFIX)
}

fn valid_session(id: &str) -> bool {
    (6..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
}

fn ssh(host: &str, args: &[&str]) -> Result<Vec<u8>, String> {
    if !crate::sysmon::safe_word(host) {
        return Err("The server's name is not valid".into());
    }
    let mut child = Command::new("ssh")
        .args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=6", "-o", "ServerAliveInterval=5", "-o", "ServerAliveCountMax=2"])
        .args(["-o", "ControlMaster=auto", "-o", "ControlPersist=120"])
        .args(["-o", &format!("ControlPath={}", crate::sysmon::control_path())])
        .arg(host)
        .arg(format!("python3 - {}", args.join(" ")))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|_| "ssh is not installed".to_string())?;
    {
        let mut stdin = child.stdin.take().ok_or("no stdin")?;
        stdin.write_all(SCRIPT.as_bytes()).map_err(|e| e.to_string())?;
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("Cannot reach {host}"));
    }
    Ok(out.stdout)
}

/// The server's agents, in the island's shape: ids and session ids prefixed, `where: "server"`.
pub fn list(host: &str) -> Result<Value, String> {
    let out = ssh(host, &["list"])?;
    from_list_output(&out)
}

pub(crate) fn from_list_output(out: &[u8]) -> Result<Value, String> {
    let text = String::from_utf8_lossy(out);
    let line = text.lines().rev().find(|l| l.trim_start().starts_with('[') || l.trim_start().starts_with('{')).unwrap_or("[]");
    if let Ok(v) = serde_json::from_str::<Value>(line) {
        if let Some(e) = v.get("error").and_then(|e| e.as_str()) {
            return Err(format!("server: {e}"));
        }
    }
    let list = claude_sessions::parse_list(line.as_bytes())?;
    Ok(Value::Array(
        list.as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .filter(|a| a["id"].is_string())
            .take(MAX_AGENTS)
            .map(|mut a| {
                a["id"] = json!(format!("{PREFIX}{}", a["id"].as_str().unwrap_or_default()));
                if let Some(sid) = a["sessionId"].as_str() {
                    a["sessionId"] = json!(format!("{PREFIX}{sid}"));
                }
                a["name"] = json!(format!("{} · server", a["name"].as_str().unwrap_or("agent")));
                a["where"] = json!("server");
                a
            })
            .collect(),
    ))
}

/// What was said in a server session since `offset` (the same contract as the local one).
pub fn messages(host: &str, session_id: &str, offset: u64) -> Result<Messages, String> {
    let sid = session_id.strip_prefix(PREFIX).ok_or("not a server session")?;
    if !valid_session(sid) {
        return Err("Not a session id".into());
    }
    let out = ssh(host, &["messages", sid, &offset.to_string()])?;
    from_messages_output(&out, offset)
}

pub(crate) fn from_messages_output(out: &[u8], asked: u64) -> Result<Messages, String> {
    let nl = out.iter().position(|b| *b == b'\n').ok_or("Unreadable answer from the server")?;
    let start: i64 = String::from_utf8_lossy(&out[..nl]).trim().parse().map_err(|_| "Unreadable answer from the server")?;
    if start < 0 {
        return Err("No transcript for that session".into());
    }
    Ok(claude_sessions::messages_from_raw(start as u64, asked == 0, &out[nl + 1..]))
}

pub const READ_ONLY: &str = "Agents on the server are read-only here: answer or stop them where they run";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_servers_agents_get_prefixed_ids_and_a_where() {
        let out = br#"noise
[{"id":"ab12cd34","name":"web","cwd":"/srv/web","kind":"interactive","status":"running","state":"working","sessionId":"0f0f0f0f-1111-2222-3333-444444444444","waitingFor":null,"startedAt":1}]"#;
        let v = from_list_output(out).unwrap();
        let a = &v[0];
        assert_eq!(a["id"], "srv:ab12cd34");
        assert_eq!(a["sessionId"], "srv:0f0f0f0f-1111-2222-3333-444444444444");
        assert_eq!(a["name"], "web · server");
        assert_eq!(a["where"], "server");
    }

    #[test]
    fn an_error_from_the_server_script_is_an_error_here() {
        assert!(from_list_output(br#"{"error":"claude is not installed on the server"}"#).unwrap_err().contains("not installed"));
    }

    #[test]
    fn an_empty_or_garbled_list_is_not_a_crash() {
        assert_eq!(from_list_output(b"[]").unwrap(), json!([]));
        assert_eq!(from_list_output(b"").unwrap(), json!([]));
        assert!(from_list_output(b"[not json").is_err());
    }

    #[test]
    fn only_session_ids_reach_the_command_line() {
        assert!(valid_session("0f0f0f0f-1111-2222-3333-444444444444"));
        assert!(!valid_session("a; rm -rf ~"));
        assert!(!valid_session("../../etc/passwd"));
        assert!(!valid_session("ab"));
        assert!(is_remote("srv:abc") && !is_remote("abc"));
    }

    #[test]
    fn messages_come_back_with_the_servers_offset() {
        let line = r#"{"type":"user","message":{"role":"user","content":"hola"}}"#;
        let mut out = b"0\n".to_vec();
        out.extend_from_slice(line.as_bytes());
        out.push(b'\n');
        let m = from_messages_output(&out, 0).unwrap();
        assert_eq!(m.offset, line.len() as u64 + 1);
        assert_eq!(m.messages.len(), 1);
        assert_eq!(m.messages[0].text, "hola");
        assert!(from_messages_output(b"-1\n", 0).is_err());
        assert!(from_messages_output(b"", 0).is_err());
    }

    #[test]
    fn a_read_from_the_middle_of_a_long_transcript_skips_the_broken_first_line() {
        let mut out = b"500\nrest of a broken record\n".to_vec();
        out.extend_from_slice(br#"{"type":"user","message":{"role":"user","content":"ok"}}"#);
        out.push(b'\n');
        let m = from_messages_output(&out, 0).unwrap();
        assert_eq!(m.messages.len(), 1);
    }
}
