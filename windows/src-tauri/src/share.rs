// What Jinx can see of the Hub: small files pushed to the server over ssh, into a folder of
// their own (~/.hermes/state/coucou/). Notes, the calendar, the timer and the machine's state
// land there as plain files Jinx can read; nothing of Hermes' own configuration is touched.
//
// Pushes are coalesced: only the latest content of each file is ever sent, one ssh at a time,
// in the background. The result of the last push is kept so the Hub can say whether Jinx is
// in step.

use std::collections::HashMap;
use std::io::Write;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::Serialize;

/// The folder on the server (a path that starts at the home directory there).
const REMOTE_DIR: &str = ".hermes/state/coucou";

static PENDING: Mutex<Option<HashMap<String, (String, String)>>> = Mutex::new(None); // name -> (host, content)
static RUNNING: AtomicBool = AtomicBool::new(false);
static STATUS: Mutex<Status> = Mutex::new(Status { last_ok: None, last_error: None });

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// Unix seconds of the last push that arrived.
    pub last_ok: Option<i64>,
    pub last_error: Option<String>,
}

pub fn status() -> Status {
    STATUS.lock().unwrap().clone()
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// File names are plain: lowercase letters, digits, `_` and `.` (no folders).
pub fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 40
        && !name.starts_with('.')
        && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'.')
}

/// The remote shell command that writes stdin to `name` atomically.
pub fn remote_command(name: &str) -> String {
    format!(
        "mkdir -p ~/{REMOTE_DIR} && cat > ~/{REMOTE_DIR}/.{name}.tmp && mv ~/{REMOTE_DIR}/.{name}.tmp ~/{REMOTE_DIR}/{name}"
    )
}

fn push_now(host: &str, name: &str, content: &str) -> Result<(), String> {
    let control = crate::sysmon::control_path();
    let mut child = Command::new("ssh")
        .args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=6", "-o", "ControlMaster=auto", "-o", "ControlPersist=120"])
        .args(["-o", &format!("ControlPath={control}")])
        .arg(host)
        .arg(remote_command(name))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "ssh is not installed".to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(content.as_bytes()).map_err(|e| e.to_string())?;
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        return Ok(());
    }
    let why = String::from_utf8_lossy(&out.stderr);
    let why = why.lines().next().unwrap_or("").trim().chars().take(100).collect::<String>();
    Err(if why.is_empty() { format!("Cannot reach {host}") } else { why })
}

/// Sends `content` as `name` to `host` in the background; a newer call for the same file
/// replaces one still waiting.
pub fn schedule(host: &str, name: &str, content: String) {
    if !crate::sysmon::safe_word(host) || !valid_name(name) {
        return;
    }
    PENDING
        .lock()
        .unwrap()
        .get_or_insert_with(HashMap::new)
        .insert(name.to_string(), (host.to_string(), content));
    if RUNNING.swap(true, Ordering::SeqCst) {
        return; // the running loop will take it
    }
    std::thread::spawn(|| {
        loop {
            let next = {
                let mut guard = PENDING.lock().unwrap();
                let map = guard.get_or_insert_with(HashMap::new);
                let key = map.keys().next().cloned();
                key.and_then(|k| map.remove(&k).map(|v| (k, v)))
            };
            let Some((name, (host, content))) = next else {
                RUNNING.store(false, Ordering::SeqCst);
                // Something may have been queued between the empty look and the flag going down.
                if PENDING.lock().unwrap().as_ref().is_some_and(|m| !m.is_empty()) && !RUNNING.swap(true, Ordering::SeqCst) {
                    continue;
                }
                return;
            };
            let result = push_now(&host, &name, &content);
            let mut s = STATUS.lock().unwrap();
            match result {
                Ok(()) => {
                    s.last_ok = Some(now());
                    s.last_error = None;
                }
                Err(e) => s.last_error = Some(e),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_plain_file_names_are_pushed() {
        assert!(valid_name("notes.md") && valid_name("context.json") && valid_name("calendar.json"));
        assert!(!valid_name("../notes.md") && !valid_name(".hidden") && !valid_name("a b") && !valid_name("") && !valid_name("Notes.md"));
    }

    #[test]
    fn the_remote_command_writes_through_a_temporary_file_in_its_own_folder() {
        let c = remote_command("notes.md");
        assert!(c.starts_with("mkdir -p ~/.hermes/state/coucou && cat > ~/.hermes/state/coucou/.notes.md.tmp"));
        assert!(c.ends_with("mv ~/.hermes/state/coucou/.notes.md.tmp ~/.hermes/state/coucou/notes.md"));
    }
}
