// What the server is busy with (downloads, a backup that is late), for the island's pill and the
// Today tab. Off until turned on in Settings, and only with a server set.
//
// One fixed script (activities.py) is sent over the same shared ssh connection as the rest of the
// Hub, on stdin: nothing is installed on the server, and no argument or text of the user's is
// part of the command. It prints one line of JSON, which is checked here before the page sees it:
// every field is bounded, unknown states are dropped. The server's API keys stay on the server.

use std::io::Write;
use std::process::{Command, Stdio};

use serde::Serialize;
use serde_json::Value;

/// The script that runs on the server.
const SCRIPT: &str = include_str!("activities.py");

const MAX_ITEMS: usize = 12;

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Activity {
    pub id: String,
    pub app: String,
    /// "download", "backup" or "other".
    pub kind: String,
    pub title: String,
    /// 0–1, or None when it has no progress to speak of.
    pub progress: Option<f64>,
    /// Seconds left, when known.
    pub eta: Option<u64>,
    /// "active", "queued", "done" or "error".
    pub state: String,
    pub detail: String,
}

fn text(v: &Value, max: usize) -> String {
    v.as_str()
        .unwrap_or("")
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max)
        .collect()
}

/// The script's last line of output, checked and bounded.
pub fn parse(output: &str) -> Vec<Activity> {
    let last = output.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
    let Ok(v) = serde_json::from_str::<Value>(last) else { return Vec::new() };
    let Some(items) = v["activities"].as_array() else { return Vec::new() };
    items
        .iter()
        .filter_map(|i| {
            let state = i["state"].as_str()?;
            if !matches!(state, "active" | "queued" | "done" | "error") {
                return None;
            }
            let kind = match i["kind"].as_str() {
                Some("download") => "download",
                Some("backup") => "backup",
                _ => "other",
            };
            let id = text(&i["id"], 60);
            let title = text(&i["title"], 80);
            if id.is_empty() || title.is_empty() {
                return None;
            }
            Some(Activity {
                id,
                app: text(&i["app"], 30),
                kind: kind.to_string(),
                title,
                progress: i["progress"].as_f64().filter(|p| p.is_finite()).map(|p| p.clamp(0.0, 1.0)),
                eta: i["eta"].as_u64().filter(|s| *s <= 10 * 86_400),
                state: state.to_string(),
                detail: text(&i["detail"], 40),
            })
        })
        .take(MAX_ITEMS)
        .collect()
}

/// One look at the server. Fails quietly in the caller: an offline server is not an error worth a
/// message every half minute.
pub fn fetch(host: &str) -> Result<Vec<Activity>, String> {
    if !crate::sysmon::safe_word(host) {
        return Err("The server's name is not valid".into());
    }
    let mut child = Command::new("ssh")
        .args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=6", "-o", "ServerAliveInterval=5", "-o", "ServerAliveCountMax=2"])
        .args(["-o", "ControlMaster=auto", "-o", "ControlPersist=120"])
        .args(["-o", &format!("ControlPath={}", crate::sysmon::control_path())])
        .arg(host)
        .arg("python3 -")
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
    Ok(parse(&String::from_utf8_lossy(&out.stdout)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_scripts_line_becomes_checked_activities() {
        let line = r#"{"at":1,"activities":[
          {"id":"deluge:ab","app":"Deluge","kind":"download","title":"Some.Show.S01E01","progress":0.42,"eta":300,"state":"active"},
          {"id":"backup:B","app":"Backup","kind":"backup","title":"Backup","progress":null,"eta":null,"state":"error","detail":"2 d ago"}]}"#
            .replace('\n', "");
        let a = parse(&format!("noise\n{line}\n"));
        assert_eq!(a.len(), 2);
        assert_eq!((a[0].state.as_str(), a[0].progress, a[0].eta), ("active", Some(0.42), Some(300)));
        assert_eq!((a[1].kind.as_str(), a[1].detail.as_str(), a[1].progress), ("backup", "2 d ago", None));
    }

    #[test]
    fn everything_is_bounded_and_the_unknown_is_dropped() {
        let long = "x".repeat(500);
        let line = format!(
            r#"{{"activities":[
              {{"id":"a","app":"{long}","kind":"weird","title":"{long}","progress":7.5,"eta":99999999,"state":"active"}},
              {{"id":"b","title":"t","state":"exploding"}},
              {{"id":"","title":"no id","state":"active"}},
              {{"id":"c","title":"ctl\u0007\n\u001b[0m","state":"queued"}}]}}"#
        )
        .replace('\n', "");
        let a = parse(&line);
        assert_eq!(a.len(), 2, "an unknown state and a missing id are dropped");
        assert_eq!(a[0].app.chars().count(), 30);
        assert_eq!(a[0].title.chars().count(), 80);
        assert_eq!(a[0].kind, "other");
        assert_eq!(a[0].progress, Some(1.0), "clamped");
        assert_eq!(a[0].eta, None, "an absurd eta is not believed");
        assert!(!a[1].title.chars().any(|c| c.is_control()));
    }

    #[test]
    fn garbage_gives_nothing() {
        assert!(parse("").is_empty());
        assert!(parse("not json").is_empty());
        assert!(parse(r#"{"activities":"no"}"#).is_empty());
        assert!(parse(r#"{"other":1}"#).is_empty());
    }

    #[test]
    fn at_most_so_many() {
        let items: Vec<String> = (0..30).map(|i| format!(r#"{{"id":"i{i}","title":"t","state":"active"}}"#)).collect();
        assert_eq!(parse(&format!(r#"{{"activities":[{}]}}"#, items.join(","))).len(), MAX_ITEMS);
    }

    #[test]
    fn the_script_is_the_one_that_is_tested() {
        assert!(SCRIPT.contains("def collect"), "include_str! pulled in activities.py");
        assert!(!SCRIPT.contains("'''") || SCRIPT.contains("\"\"\""));
    }
}
