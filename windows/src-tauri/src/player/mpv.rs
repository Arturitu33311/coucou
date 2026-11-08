// Drives one background mpv over its JSON IPC socket (YT Music audio for the glass player).
// mpv shows up on MPRIS through the system `mpv-mpris` plugin, so music.rs sees it unchanged.

use std::io;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

const REPLY_TIMEOUT: Duration = Duration::from_millis(1500);
const BOOT_POLL: Duration = Duration::from_millis(100);
const BOOT_MAX: Duration = Duration::from_secs(3);

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

/// Sends one command on an open connection and checks mpv's reply.
async fn send(stream: UnixStream, args: &[&str]) -> Result<(), String> {
    let (r, mut w) = stream.into_split();
    w.write_all(command_json(args).as_bytes())
        .await
        .map_err(|e| format!("mpv write: {e}"))?;
    let mut lines = BufReader::new(r).lines();
    // mpv may interleave events ({"event":...}) before the reply to our command.
    let reply = tokio::time::timeout(REPLY_TIMEOUT, async {
        while let Ok(Some(line)) = lines.next_line().await {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                if v.get("error").is_some() {
                    return Some(v);
                }
            }
        }
        None
    })
    .await;
    match reply {
        Ok(Some(v)) => match v["error"].as_str() {
            Some("success") => Ok(()),
            other => Err(format!("mpv: {}", other.unwrap_or("unknown error"))),
        },
        Ok(None) => Err("mpv closed the socket without replying".into()),
        Err(_) => Err("mpv did not reply in time".into()),
    }
}

async fn play_at(
    sock: &Path,
    launcher: impl Fn() -> io::Result<()>,
    video_id: &str,
) -> Result<(), String> {
    let stream = match UnixStream::connect(sock).await {
        Ok(s) => s,
        Err(_) => {
            let _ = std::fs::remove_file(sock);
            launcher().map_err(|e| format!("could not start mpv: {e}"))?;
            let mut waited = Duration::ZERO;
            loop {
                tokio::time::sleep(BOOT_POLL).await;
                waited += BOOT_POLL;
                if let Ok(s) = UnixStream::connect(sock).await {
                    break s;
                }
                if waited >= BOOT_MAX {
                    return Err("mpv did not open its socket in time".into());
                }
            }
        }
    };
    let url = format!("https://music.youtube.com/watch?v={video_id}");
    send(stream, &["loadfile", &url, "replace"]).await
}

async fn pause_at(sock: &Path) -> Result<(), String> {
    match UnixStream::connect(sock).await {
        Ok(s) => send(s, &["set_property", "pause", "yes"]).await,
        Err(_) => Ok(()),
    }
}

pub async fn play_ytmusic(video_id: &str) -> Result<(), String> {
    let sock = socket_path();
    let s2 = sock.clone();
    play_at(&sock, move || launch_mpv(&s2), video_id).await
}

pub async fn pause() -> Result<(), String> {
    pause_at(&socket_path()).await
}

fn launch_mpv(sock: &Path) -> io::Result<()> {
    let ytdlp = std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".local/bin/yt-dlp"))
        .unwrap_or_else(|| PathBuf::from("yt-dlp"));
    let mut cmd = Command::new("mpv");
    cmd.arg("--no-video")
        .arg("--idle=yes")
        .arg(format!("--input-ipc-server={}", sock.display()))
        .arg("--ytdl-format=bestaudio")
        .arg(format!("--script-opts=ytdl_hook-ytdl_path={}", ytdlp.display()))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
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
    use tokio::net::UnixListener;

    fn tmp_sock(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("coucou-mpv-test-{}-{}", name, std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let p = d.join("s.sock");
        let _ = std::fs::remove_file(&p);
        p
    }

    fn fake_server(sock: &Path) -> Arc<Mutex<Vec<String>>> {
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
                        out.lock().unwrap().push(line);
                        let _ = w.write_all(b"{\"error\":\"success\",\"data\":null}\n").await;
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

    #[tokio::test]
    async fn play_sends_loadfile_to_socket() {
        let sock = tmp_sock("play");
        let lines = fake_server(&sock);
        play_at(&sock, || panic!("must not launch: mpv is alive"), "abc123").await.unwrap();
        let got = lines.lock().unwrap().clone();
        assert_eq!(
            got,
            vec!["{\"command\":[\"loadfile\",\"https://music.youtube.com/watch?v=abc123\",\"replace\"]}"]
        );
    }

    #[tokio::test]
    async fn pause_sends_set_property() {
        let sock = tmp_sock("pause");
        let lines = fake_server(&sock);
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
            *srv2.lock().unwrap() = Some(fake_server(&s2));
            Ok(())
        };
        play_at(&sock, launcher, "zzz").await.unwrap();
        let lines = srv.lock().unwrap().clone().unwrap();
        assert_eq!(lines.lock().unwrap().len(), 1);
    }
}
