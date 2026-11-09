// Drives one background mpv over its JSON IPC socket (YT Music audio for the glass player).
// mpv shows up on MPRIS through the system `mpv-mpris` plugin, so music.rs sees it unchanged.

use std::io;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

use crate::i18n::{t, tf};

const REPLY_TIMEOUT: Duration = Duration::from_millis(1500);
const BOOT_POLL: Duration = Duration::from_millis(100);
const BOOT_MAX: Duration = Duration::from_secs(3);
/// After `loadfile`, how long to wait for the song to start (or fail) before calling it started.
const SETTLE: Duration = Duration::from_secs(3);

pub fn socket_path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    dir.join("coucou-mpv.sock")
}

pub fn command_json(args: &[&str]) -> String {
    format!("{}\n", serde_json::json!({ "command": args }))
}

/// One line from mpv: the reply to our command, the song starting, the song failing, or anything else.
#[derive(Debug, PartialEq)]
enum Line {
    Reply(Result<(), String>),
    Started,
    Failed,
    Other,
}

fn classify(v: &Value) -> Line {
    if let Some(err) = v.get("error") {
        return Line::Reply(match err.as_str() {
            Some("success") => Ok(()),
            other => Err(other.unwrap_or("unknown error").to_string()),
        });
    }
    match v["event"].as_str() {
        Some("playback-restart") => Line::Started,
        Some("end-file") if v["reason"].as_str() == Some("error") => Line::Failed,
        _ => Line::Other,
    }
}

/// Sends one command on an open connection and checks mpv's reply. With `settle`, once mpv
/// took it, waits up to that long on the same connection for the file to start playing
/// (`playback-restart`) or fail (`end-file` with an error); silence means it is on its way.
async fn send(stream: UnixStream, args: &[&str], settle: Option<Duration>) -> Result<(), String> {
    let (r, mut w) = stream.into_split();
    w.write_all(command_json(args).as_bytes())
        .await
        .map_err(|e| tf("Couldn't talk to mpv: {error}", &[("error", &e.to_string())]))?;
    let mut lines = BufReader::new(r).lines();
    let reply_by = tokio::time::Instant::now() + REPLY_TIMEOUT;
    // Set once mpv accepted the command and a start is waited for.
    let mut started_by: Option<tokio::time::Instant> = None;
    loop {
        let deadline = started_by.unwrap_or(reply_by);
        // mpv interleaves events ({"event":...}) with the reply to our command.
        let line = match tokio::time::timeout_at(deadline, lines.next_line()).await {
            Ok(Ok(Some(line))) => line,
            Err(_) if started_by.is_some() => return Ok(()),
            Err(_) => return Err(t("mpv didn't answer in time")),
            Ok(_) if started_by.is_some() => return Ok(()),
            Ok(_) => return Err(t("mpv closed the connection without answering")),
        };
        let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
        match classify(&v) {
            Line::Reply(Ok(())) if started_by.is_none() => match settle {
                None => return Ok(()),
                Some(s) => started_by = Some(tokio::time::Instant::now() + s),
            },
            Line::Reply(Err(e)) if started_by.is_none() => {
                return Err(tf("mpv refused the command: {error}", &[("error", &e)]))
            }
            Line::Started if started_by.is_some() => return Ok(()),
            Line::Failed if settle.is_some() => return Err(t("YT Music couldn't play this song")),
            _ => {}
        }
    }
}

async fn play_at(
    sock: &Path,
    launcher: impl Fn() -> io::Result<()>,
    video_id: &str,
    settle: Duration,
) -> Result<(), String> {
    let stream = match UnixStream::connect(sock).await {
        Ok(s) => s,
        Err(_) => {
            let _ = std::fs::remove_file(sock);
            launcher().map_err(|e| tf("Couldn't start mpv: {error}", &[("error", &e.to_string())]))?;
            let mut waited = Duration::ZERO;
            loop {
                tokio::time::sleep(BOOT_POLL).await;
                waited += BOOT_POLL;
                if let Ok(s) = UnixStream::connect(sock).await {
                    break s;
                }
                if waited >= BOOT_MAX {
                    return Err(t("mpv didn't start in time"));
                }
            }
        }
    };
    let url = format!("https://music.youtube.com/watch?v={video_id}");
    send(stream, &["loadfile", &url, "replace"], Some(settle)).await
}

async fn pause_at(sock: &Path) -> Result<(), String> {
    match UnixStream::connect(sock).await {
        Ok(s) => send(s, &["set_property", "pause", "yes"], None).await,
        Err(_) => Ok(()),
    }
}

pub async fn play_ytmusic(video_id: &str) -> Result<(), String> {
    let sock = socket_path();
    let s2 = sock.clone();
    play_at(&sock, move || launch_mpv(&s2), video_id, SETTLE).await
}

pub async fn pause() -> Result<(), String> {
    pause_at(&socket_path()).await
}

/// `--script-opts` pointing mpv's ytdl hook at the yt-dlp scripts/setup-player.sh installs, when
/// it is there; otherwise none, and mpv finds yt-dlp on the PATH itself.
fn ytdl_opt(home: Option<&Path>) -> Option<String> {
    let ytdlp = home?.join(".local/bin/yt-dlp");
    ytdlp.is_file().then(|| format!("--script-opts=ytdl_hook-ytdl_path={}", ytdlp.display()))
}

/// yt-dlp options for Coucou only (written by scripts/setup-player.sh: IPv4 first, a JavaScript
/// runtime for YouTube), passed through mpv's ytdl hook when the file is there.
fn ytdl_config_opt(home: Option<&Path>) -> Option<String> {
    let conf = home?.join(".config/coucou/yt-dlp.conf");
    conf.is_file().then(|| format!("--ytdl-raw-options=config-locations={}", conf.display()))
}

fn launch_mpv(sock: &Path) -> io::Result<()> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut cmd = Command::new("mpv");
    cmd.arg("--no-video")
        .arg("--idle=yes")
        .arg(format!("--input-ipc-server={}", sock.display()))
        .arg("--ytdl-format=bestaudio");
    if let Some(opt) = ytdl_opt(home.as_deref()) {
        cmd.arg(opt);
    }
    if let Some(opt) = ytdl_config_opt(home.as_deref()) {
        cmd.arg(opt);
    }
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    // Own session: mpv must outlive the island and not get its terminal signals.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let mut child = cmd.spawn()?;
    // Reap it when it exits so it never lingers as a zombie while the island runs.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use std::time::Instant;
    use tokio::net::UnixListener;

    fn tmp_sock(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("coucou-mpv-test-{}-{}", name, std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("s.sock");
        let _ = std::fs::remove_file(&p);
        p
    }

    const STARTED: &[&str] = &[r#"{"event":"start-file","playlist_entry_id":1}"#, r#"{"event":"playback-restart"}"#];

    /// Answers every command with success; after a `loadfile` it also sends `events`, as mpv would.
    fn fake_server(sock: &Path, events: &'static [&'static str]) -> Arc<Mutex<Vec<String>>> {
        let lines = Arc::new(Mutex::new(Vec::new()));
        let l = UnixListener::bind(sock).unwrap();
        let out = lines.clone();
        tokio::spawn(async move {
            loop {
                let Ok((st, _)) = l.accept().await else { break };
                let out = out.clone();
                tokio::spawn(async move {
                    let (r, mut w) = st.into_split();
                    let mut r = BufReader::new(r).lines();
                    while let Ok(Some(line)) = r.next_line().await {
                        let load = line.contains("loadfile");
                        out.lock().unwrap().push(line);
                        let _ = w.write_all(b"{\"error\":\"success\",\"data\":null,\"request_id\":0}\n").await;
                        if load {
                            for e in events {
                                let _ = w.write_all(format!("{e}\n").as_bytes()).await;
                            }
                        }
                    }
                });
            }
        });
        lines
    }

    #[test]
    fn command_json_format() {
        assert_eq!(
            command_json(&["set_property", "pause", "yes"]),
            "{\"command\":[\"set_property\",\"pause\",\"yes\"]}\n"
        );
    }

    #[test]
    fn ytdl_path_only_when_it_is_there() {
        let home = std::env::temp_dir().join(format!("coucou-mpv-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        assert_eq!(ytdl_opt(Some(&home)), None, "no yt-dlp there: mpv looks on the PATH");
        assert_eq!(ytdl_opt(None), None);
        std::fs::create_dir_all(home.join(".local/bin")).unwrap();
        std::fs::write(home.join(".local/bin/yt-dlp"), "").unwrap();
        assert_eq!(
            ytdl_opt(Some(&home)),
            Some(format!("--script-opts=ytdl_hook-ytdl_path={}", home.join(".local/bin/yt-dlp").display()))
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn ytdl_config_only_when_it_is_there() {
        let home = std::env::temp_dir().join(format!("coucou-mpv-conf-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        assert_eq!(ytdl_config_opt(Some(&home)), None, "no config: yt-dlp's defaults");
        assert_eq!(ytdl_config_opt(None), None);
        std::fs::create_dir_all(home.join(".config/coucou")).unwrap();
        std::fs::write(home.join(".config/coucou/yt-dlp.conf"), "--force-ipv4\n").unwrap();
        assert_eq!(
            ytdl_config_opt(Some(&home)),
            Some(format!(
                "--ytdl-raw-options=config-locations={}",
                home.join(".config/coucou/yt-dlp.conf").display()
            ))
        );
        let _ = std::fs::remove_dir_all(&home);
    }

    #[tokio::test]
    async fn play_sends_loadfile_to_socket() {
        let sock = tmp_sock("play");
        let lines = fake_server(&sock, STARTED);
        let at = Instant::now();
        play_at(&sock, || panic!("must not launch: mpv is alive"), "abc123", SETTLE).await.unwrap();
        assert!(at.elapsed() < SETTLE, "playback-restart ends the wait");
        let got = lines.lock().unwrap().clone();
        assert_eq!(
            got,
            vec!["{\"command\":[\"loadfile\",\"https://music.youtube.com/watch?v=abc123\",\"replace\"]}"]
        );
    }

    #[tokio::test]
    async fn play_reports_a_song_that_fails_to_load() {
        let sock = tmp_sock("fail");
        // The old song's end, unrelated events, then the new one failing (yt-dlp could not read it).
        const EVENTS: &[&str] = &[
            r#"{"event":"end-file","reason":"stop","playlist_entry_id":1}"#,
            r#"{"event":"start-file","playlist_entry_id":2}"#,
            r#"{"event":"property-change","id":1,"name":"pause","data":false}"#,
            "not json",
            r#"{"event":"end-file","reason":"error","playlist_entry_id":2,"file_error":"unrecognized file format"}"#,
        ];
        let _lines = fake_server(&sock, EVENTS);
        let r = play_at(&sock, || panic!("must not launch"), "bad", SETTLE).await;
        assert_eq!(r, Err(t("YT Music couldn't play this song")));
    }

    #[tokio::test]
    async fn play_with_no_news_is_ok_after_the_wait() {
        let sock = tmp_sock("quiet");
        let _lines = fake_server(&sock, &[r#"{"event":"start-file","playlist_entry_id":3}"#]);
        let settle = Duration::from_millis(200);
        let at = Instant::now();
        play_at(&sock, || panic!("must not launch"), "slow", settle).await.unwrap();
        assert!(at.elapsed() >= settle);
    }

    #[test]
    fn classifies_replies_and_events() {
        let c = |s: &str| classify(&serde_json::from_str(s).unwrap());
        assert_eq!(c(r#"{"error":"success","data":null}"#), Line::Reply(Ok(())));
        assert_eq!(c(r#"{"error":"invalid parameter"}"#), Line::Reply(Err("invalid parameter".into())));
        assert_eq!(c(r#"{"event":"playback-restart"}"#), Line::Started);
        assert_eq!(c(r#"{"event":"end-file","reason":"error"}"#), Line::Failed);
        assert_eq!(c(r#"{"event":"end-file","reason":"eof"}"#), Line::Other);
        assert_eq!(c(r#"{"event":"idle"}"#), Line::Other);
    }

    #[tokio::test]
    async fn pause_sends_set_property() {
        let sock = tmp_sock("pause");
        let lines = fake_server(&sock, &[]);
        pause_at(&sock).await.unwrap();
        assert_eq!(
            lines.lock().unwrap().clone(),
            vec!["{\"command\":[\"set_property\",\"pause\",\"yes\"]}"]
        );
    }

    #[tokio::test]
    async fn pause_without_mpv_is_ok() {
        let sock = tmp_sock("none");
        assert!(pause_at(&sock).await.is_ok());
    }

    #[tokio::test]
    async fn play_launches_when_dead_then_sends() {
        let sock = tmp_sock("launch");
        let srv: Arc<Mutex<Option<Arc<Mutex<Vec<String>>>>>> = Arc::new(Mutex::new(None));
        let (s2, srv2) = (sock.clone(), srv.clone());
        // The "launcher" brings the fake server up, as a real mpv would.
        let launcher = move || {
            *srv2.lock().unwrap() = Some(fake_server(&s2, STARTED));
            Ok(())
        };
        play_at(&sock, launcher, "zzz", SETTLE).await.unwrap();
        let lines = srv.lock().unwrap().clone().unwrap();
        assert_eq!(lines.lock().unwrap().len(), 1);
    }
}
