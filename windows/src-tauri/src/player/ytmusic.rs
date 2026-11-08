// Glass player: YT Music search / artist / album through a local Python helper (ytmusicapi, no login).
use super::Item;
use std::path::Path;
use std::time::Duration;

const HELPER: &str = include_str!("ytmusic.py");
const TIMEOUT: Duration = Duration::from_secs(8);
const NOT_READY: &str = "YT Music no está preparado: corre scripts/setup-player.sh";

/// The helper prints a JSON list of items, or `{"error": "..."}`.
pub fn parse_helper_output(stdout: &str) -> Result<Vec<Item>, String> {
    let v: serde_json::Value = serde_json::from_str(stdout.trim()).map_err(|e| format!("respuesta no válida de YT Music: {e}"))?;
    if let Some(msg) = v.get("error") {
        return Err(msg.as_str().unwrap_or("error de YT Music").to_string());
    }
    serde_json::from_value(v).map_err(|e| format!("respuesta no válida de YT Music: {e}"))
}

async fn run_helper_with(interpreter: &Path, op: &str, arg: &str, timeout: Duration) -> Result<Vec<Item>, String> {
    use tokio::io::AsyncWriteExt;
    if !interpreter.exists() {
        return Err(NOT_READY.into());
    }
    let mut child = tokio::process::Command::new(interpreter)
        .args(["-I", "-", op, arg])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("no se pudo lanzar YT Music: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("sin stdin")?;
    let run = async {
        // A helper that exits early closes the pipe; that is reported through its output, not here.
        let _ = stdin.write_all(HELPER.as_bytes()).await;
        drop(stdin);
        child.wait_with_output().await
    };
    // On timeout the future (and the child it owns) is dropped, and kill_on_drop ends the process.
    let out = tokio::time::timeout(timeout, run)
        .await
        .map_err(|_| "YT Music tardó demasiado".to_string())?
        .map_err(|e| format!("YT Music falló: {e}"))?;
    parse_helper_output(&String::from_utf8_lossy(&out.stdout))
}

async fn run_helper(op: &str, arg: &str) -> Result<Vec<Item>, String> {
    let py = crate::settings::local_dir().join("ytm-venv/bin/python3");
    run_helper_with(&py, op, arg, TIMEOUT).await
}

pub async fn search(q: &str) -> Result<Vec<Item>, String> {
    run_helper("search", q).await
}

pub async fn artist(id: &str) -> Result<Vec<Item>, String> {
    run_helper("artist", id).await
}

pub async fn album(id: &str) -> Result<Vec<Item>, String> {
    run_helper("album", id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::{Kind, Source};

    #[test]
    fn parse_ok() {
        let s = r#"[{"source":"ytmusic","kind":"track","id":"abc","title":"T","subtitle":"A, B","artUrl":"http://x/y.jpg"}]"#;
        let v = parse_helper_output(s).unwrap();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].source, Source::Ytmusic);
        assert_eq!(v[0].kind, Kind::Track);
        assert_eq!(v[0].subtitle, "A, B");
        assert_eq!(v[0].art_url, "http://x/y.jpg");
    }

    #[test]
    fn parse_error_object() {
        assert_eq!(parse_helper_output(r#"{"error":"boom"}"#).unwrap_err(), "boom");
    }

    #[test]
    fn parse_garbage_is_error() {
        assert!(parse_helper_output("not json").is_err());
        assert!(parse_helper_output("").is_err());
    }

    #[tokio::test]
    async fn helper_timeout_is_an_error_not_a_hang() {
        let dir = std::env::temp_dir().join(format!("ytm-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let fake = dir.join("fake-python");
        std::fs::write(&fake, "#!/bin/sh\nsleep 5\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let t = std::time::Instant::now();
        let r = run_helper_with(&fake, "search", "x", Duration::from_millis(300)).await;
        assert!(r.is_err());
        assert!(t.elapsed() < Duration::from_secs(3));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn missing_venv_is_a_clear_error() {
        let r = run_helper_with(Path::new("/nonexistent/python3"), "search", "x", Duration::from_secs(1)).await;
        assert!(r.unwrap_err().contains("setup-player.sh"));
    }
}
