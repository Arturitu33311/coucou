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
    /// False for a video about as long as the song, shown with offset 0 until `refine` measures it.
    #[serde(default = "yes")]
    pub measured: bool,
}

fn yes() -> bool {
    true
}

type Waiters = HashMap<u64, oneshot::Sender<Result<Value, String>>>;

struct Running {
    stdin: ChildStdin,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static WAITERS: Mutex<Option<Waiters>> = Mutex::new(None);
static PROC: tokio::sync::Mutex<Option<Running>> = tokio::sync::Mutex::const_new(None);

/// One answer line: the reply to request `id` (the whole object), or its error. The greeting (id 0) answers nobody.
pub fn parse_answer(line: &str) -> Option<(u64, Result<Value, String>)> {
    let v: Value = serde_json::from_str(line).ok()?;
    let id = v.get("id")?.as_u64().filter(|&id| id != 0)?;
    if let Some(e) = v.get("error") {
        return Some((id, Err(e.as_str().unwrap_or("error").to_string())));
    }
    Some((id, Ok(v)))
}

/// The video in a `find` answer.
pub fn video_of(answer: &Value) -> Result<Option<Video>, String> {
    match answer.get("video") {
        None | Some(Value::Null) => Ok(None),
        Some(v) => serde_json::from_value(v.clone()).map(Some).map_err(|e| e.to_string()),
    }
}

/// `fresh`: the cached stream URL just failed, resolve a new one.
pub fn request_line(id: u64, title: &str, artists: &[String], length_s: f64, fresh: bool) -> String {
    format!(
        "{}\n",
        serde_json::json!({ "id": id, "title": title, "artists": artists, "length": length_s.round() as i64,
                            "fresh": fresh })
    )
}

pub fn refine_line(id: u64, title: &str, artists: &[String], length_s: f64, video_id: &str) -> String {
    format!(
        "{}\n",
        serde_json::json!({ "id": id, "op": "refine", "title": title, "artists": artists,
                            "length": length_s.round() as i64, "video": video_id })
    )
}

fn deliver(id: u64, answer: Result<Value, String>) {
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

/// Sends one request (built by `line` with its id) and waits for its answer.
async fn ask(line: impl FnOnce(u64) -> String) -> Result<Value, String> {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let (tx, rx) = oneshot::channel();
    WAITERS.lock().map_err(|_| "poisoned")?.get_or_insert_with(HashMap::new).insert(id, tx);
    {
        let mut proc = PROC.lock().await;
        if proc.is_none() {
            match spawn() {
                Ok(p) => *proc = Some(p),
                Err(e) => {
                    WAITERS.lock().ok().map(|mut w| w.get_or_insert_with(HashMap::new).remove(&id));
                    return Err(e);
                }
            }
        }
        if let Err(e) = proc.as_mut().unwrap().stdin.write_all(line(id).as_bytes()).await {
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

/// The official music video of `title` by `artists` that runs on the song's timeline (`length_s`: the song's
/// length, 0 = unknown), or None when there is none.
pub async fn find(title: &str, artists: &[String], length_s: f64, fresh: bool) -> Result<Option<Video>, String> {
    video_of(&ask(|id| request_line(id, title, artists, length_s, fresh)).await?)
}

/// The measured offset of a video found with `measured: false`; None when it does not line up.
pub async fn refine(title: &str, artists: &[String], length_s: f64, video_id: &str) -> Result<Option<f64>, String> {
    Ok(ask(|id| refine_line(id, title, artists, length_s, video_id)).await?.get("offset").and_then(Value::as_f64))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_answers() {
        let (id, a) = parse_answer(r#"{"id": 3, "video": {"id": "abc", "url": "https://x", "offset": 22.6}}"#).unwrap();
        assert_eq!(id, 3);
        assert_eq!(
            video_of(&a.unwrap()),
            Ok(Some(Video { id: "abc".into(), url: "https://x".into(), offset: 22.6, measured: true }))
        );
        let (_, a) = parse_answer(r#"{"id": 6, "video": {"id": "h", "url": "u", "offset": 0.0, "measured": false}}"#).unwrap();
        assert!(!video_of(&a.unwrap()).unwrap().unwrap().measured);
        let (_, a) = parse_answer(r#"{"id": 4, "video": null}"#).unwrap();
        assert_eq!(video_of(&a.unwrap()), Ok(None));
        assert_eq!(parse_answer(r#"{"id": 5, "error": "boom"}"#), Some((5, Err("boom".into()))));
        assert_eq!(parse_answer(r#"{"id": 0, "ready": true}"#), None, "the greeting answers nobody");
        assert_eq!(parse_answer("not json"), None);
        let (_, a) = parse_answer(r#"{"id": 8, "offset": 2.74}"#).unwrap();
        assert_eq!(a.unwrap()["offset"].as_f64(), Some(2.74));
    }

    #[test]
    fn refine_names_the_video() {
        let v: Value = serde_json::from_str(&refine_line(8, "Houdini", &["Dua Lipa".into()], 186.0, "suAR")).unwrap();
        assert_eq!(v["op"], "refine");
        assert_eq!(v["video"], "suAR");
    }

    #[test]
    fn request_is_one_json_line() {
        let l = request_line(9, "Houdini", &["Dua Lipa".into()], 186.4, true);
        assert!(l.ends_with('\n') && !l[..l.len() - 1].contains('\n'));
        let v: Value = serde_json::from_str(&l).unwrap();
        assert_eq!(v["id"], 9);
        assert_eq!(v["length"], 186);
        assert_eq!(v["artists"][0], "Dua Lipa");
        assert_eq!(v["fresh"], true);
    }
}
