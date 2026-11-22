// The desktop player's video resolver (videod.py), kept running: yt-dlp and ytmusicapi load once, and the
// picks, offsets and stream URLs it found stay cached. Started on first use, again if it died.
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use serde::Deserialize;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::ChildStdin;
use tokio::sync::oneshot;

const SCRIPT: &str = include_str!("videod.py");
/// A first answer can take two yt-dlp runs and an audio alignment.
const ANSWER_TIMEOUT: Duration = Duration::from_secs(90);

/// The song's music video, ready for mpv.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Video {
    pub id: String,
    /// The ≤720p picture stream: mpv plays it directly, no yt-dlp.
    pub url: String,
    /// Where the song starts in the video, seconds (an intro of the video's own).
    #[serde(default)]
    pub offset: f64,
}

type Waiters = HashMap<u64, oneshot::Sender<Result<Option<Video>, String>>>;

struct Running {
    stdin: ChildStdin,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static WAITERS: Mutex<Option<Waiters>> = Mutex::new(None);
static PROC: tokio::sync::Mutex<Option<Running>> = tokio::sync::Mutex::const_new(None);

/// One answer line: the reply to request `id`.
pub fn parse_answer(line: &str) -> Option<(u64, Result<Option<Video>, String>)> {
    let v: Value = serde_json::from_str(line).ok()?;
    let id = v.get("id")?.as_u64()?;
    if let Some(e) = v.get("error") {
        return Some((id, Err(e.as_str().unwrap_or("error").to_string())));
    }
    let video = v.get("video")?;
    if video.is_null() {
        return Some((id, Ok(None)));
    }
    Some((id, serde_json::from_value(video.clone()).map(Some).map_err(|e| e.to_string())))
}

pub fn request_line(id: u64, title: &str, artists: &[String], length_s: f64) -> String {
    format!(
        "{}\n",
        serde_json::json!({ "id": id, "title": title, "artists": artists, "length": length_s.round() as i64 })
    )
}

fn deliver(id: u64, answer: Result<Option<Video>, String>) {
    if let Some(tx) = WAITERS.lock().ok().and_then(|mut w| w.get_or_insert_with(HashMap::new).remove(&id)) {
        let _ = tx.send(answer);
    }
}

fn fail_all(why: &str) {
    let drained: Vec<_> = WAITERS.lock().map(|mut w| w.get_or_insert_with(HashMap::new).drain().collect()).unwrap_or_default();
    for (_, tx) in drained {
        let _ = tx.send(Err(why.to_string()));
    }
}

fn spawn() -> Result<Running, String> {
    let py = crate::settings::local_dir().join("ytm-venv/bin/python3");
    if !py.exists() {
        return Err(crate::i18n::t("YT Music isn't set up: run scripts/setup-player.sh"));
    }
    let mut child = tokio::process::Command::new(py)
        .args(["-I", "-c", SCRIPT])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("video resolver: {e}"))?;
    let stdin = child.stdin.take().ok_or("video resolver: no stdin")?;
    let stdout = child.stdout.take().ok_or("video resolver: no stdout")?;
    tauri::async_runtime::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some((id, answer)) = parse_answer(&line) {
                deliver(id, answer);
            }
        }
        let _ = child.wait().await;
        crate::log::line("desktop video: resolver stopped");
        PROC.lock().await.take();
        fail_all("video resolver stopped");
    });
    Ok(Running { stdin })
}

/// The official music video of `title` by `artists` that runs on the song's timeline (`length_s`: the song's
/// length, 0 = unknown), or None when there is none.
pub async fn find(title: &str, artists: &[String], length_s: f64) -> Result<Option<Video>, String> {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let (tx, rx) = oneshot::channel();
    WAITERS.lock().map_err(|_| "poisoned")?.get_or_insert_with(HashMap::new).insert(id, tx);
    {
        let mut proc = PROC.lock().await;
        if proc.is_none() {
            *proc = Some(spawn()?);
        }
        let line = request_line(id, title, artists, length_s);
        if let Err(e) = proc.as_mut().unwrap().stdin.write_all(line.as_bytes()).await {
            proc.take();
            deliver(id, Err(format!("video resolver: {e}")));
        }
    }
    match tokio::time::timeout(ANSWER_TIMEOUT, rx).await {
        Ok(Ok(answer)) => answer,
        Ok(Err(_)) => Err("video resolver: no answer".into()),
        Err(_) => {
            WAITERS.lock().ok().map(|mut w| w.get_or_insert_with(HashMap::new).remove(&id));
            Err("video resolver: too slow".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_answers() {
        assert_eq!(
            parse_answer(r#"{"id": 3, "video": {"id": "abc", "url": "https://x", "offset": 22.6}}"#),
            Some((3, Ok(Some(Video { id: "abc".into(), url: "https://x".into(), offset: 22.6 }))))
        );
        assert_eq!(parse_answer(r#"{"id": 4, "video": null}"#), Some((4, Ok(None))));
        assert_eq!(parse_answer(r#"{"id": 5, "error": "boom"}"#), Some((5, Err("boom".into()))));
        assert_eq!(parse_answer(r#"{"id": 0, "ready": true}"#), None, "the greeting answers nobody");
        assert_eq!(parse_answer("not json"), None);
    }

    #[test]
    fn request_is_one_json_line() {
        let l = request_line(9, "Houdini", &["Dua Lipa".into()], 186.4);
        assert!(l.ends_with('\n') && !l[..l.len() - 1].contains('\n'));
        let v: Value = serde_json::from_str(&l).unwrap();
        assert_eq!(v["id"], 9);
        assert_eq!(v["length"], 186);
        assert_eq!(v["artists"][0], "Dua Lipa");
    }
}
