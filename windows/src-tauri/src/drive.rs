// Uploading a shelf file somewhere that is not this computer, directly (no model involved):
//
//   * your school's OneDrive — through Jinx's own upload function (`escuela_subir` of her
//     teams-tareas plugin), called on the server over ssh. The file goes to a staging folder
//     there first; the Microsoft sign-in is hers (its token lives in her vault and never reaches
//     Coucou), and so are the folder rules (Materias/<MATERIA>/Apuntes, "Por clasificar"…).
//   * your personal Google Drive — with `rclone`, which keeps its own login (a one-time browser
//     sign-in that Coucou starts for you). Only files rclone itself creates are reachable.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};

/// OneDrive's simple upload, which Jinx's function uses, stops here.
pub const MAX_SCHOOL_BYTES: u64 = 250 * 1024 * 1024;
const DEFAULT_SCHOOL_FOLDER: &str = "Por clasificar";
const DEFAULT_DRIVE_FOLDER: &str = "Coucou";

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Uploaded {
    /// Where it landed (OneDrive: the folder/name; Drive: remote:folder/name).
    pub location: String,
    pub url: Option<String>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DriveState {
    pub installed: bool,
    pub connected: bool,
}

// ── Names (pure) ──────────────────────────────────────────────────────────────

/// A folder path for the cloud: no empty or `..` segments, no control characters, short.
pub fn clean_folder(raw: &str, fallback: &str) -> String {
    let parts: Vec<String> = raw
        .replace('\\', "/")
        .split('/')
        .map(|p| p.chars().filter(|c| !c.is_control()).collect::<String>().trim().to_string())
        .filter(|p| !p.is_empty() && p != "." && p != "..")
        .collect();
    let joined = parts.join("/");
    let joined: String = joined.chars().take(120).collect();
    if joined.is_empty() { fallback.to_string() } else { joined }
}

/// The name a file has in the staging folder: plain characters only, so it can sit on a command line.
pub fn staging_name(name: &str, stamp: i64) -> String {
    let plain: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
        .take(80)
        .collect();
    format!("{stamp}-{plain}")
}

/// `s` as one single-quoted shell word.
pub fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// A word that may name an rclone remote.
fn remote_ok(s: &str) -> bool {
    !s.is_empty() && s.len() <= 40 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

// ── The school's OneDrive, through Jinx's function ────────────────────────────

const UPLOADER: &str = r#"import importlib.util, json, os, sys
from pathlib import Path
a = json.loads(sys.argv[1])
home = Path(os.environ.get("HERMES_HOME", str(Path.home() / ".hermes")))
src = home / "state" / "coucou" / "outbox" / Path(a["file"]).name
spec = importlib.util.spec_from_file_location("tt", home / "plugins" / "teams-tareas" / "__init__.py")
tt = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tt)
print(tt.escuela_subir({"ruta_archivo": str(src), "subcarpeta": a["folder"], "nombre": a["name"]}))
try:
    src.unlink()
except OSError:
    pass
"#;

/// What the uploader printed (its last line is Jinx's JSON answer) as an upload or an error.
/// The error is `reauth` when the Microsoft sign-in has expired.
pub fn parse_school(stdout: &str) -> Result<Uploaded, String> {
    let line = stdout.lines().rev().find(|l| l.trim_start().starts_with('{')).ok_or("Jinx's uploader said nothing")?;
    let v: Value = serde_json::from_str(line).map_err(|_| "Unreadable answer from Jinx's uploader".to_string())?;
    if v["ok"].as_bool() == Some(true) {
        return Ok(Uploaded { location: v["onedrive"].as_str().unwrap_or("OneDrive").to_string(), url: v["url"].as_str().map(str::to_string) });
    }
    let error = v["error"].as_str().unwrap_or("error");
    let detail: String = v["detalle"].as_str().unwrap_or("").chars().take(120).collect();
    Err(match error {
        "reauth" => "reauth".to_string(),
        "archivo_muy_grande" => "That file is over OneDrive's 250 MB limit".to_string(),
        other => format!("{other}{}{detail}", if detail.is_empty() { "" } else { ": " }),
    })
}

fn ssh(host: &str) -> Command {
    let mut c = Command::new("ssh");
    c.args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=8", "-o", "ControlMaster=auto", "-o", "ControlPersist=120"])
        .args(["-o", &format!("ControlPath={}", crate::sysmon::control_path())])
        .arg(host);
    c
}

/// Uploads `path` (named `name`) to the school's OneDrive, in `folder`.
pub fn school_upload(host: &str, path: &Path, name: &str, folder: &str) -> Result<Uploaded, String> {
    if !crate::sysmon::safe_word(host) {
        return Err("The server's name is not valid".into());
    }
    let meta = std::fs::metadata(path).map_err(|_| "That file is gone".to_string())?;
    if meta.len() > MAX_SCHOOL_BYTES {
        return Err("That file is over OneDrive's 250 MB limit".into());
    }
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0);
    let staged = staging_name(name, stamp);
    let folder = clean_folder(folder, DEFAULT_SCHOOL_FOLDER);

    // 1. the file to the server's staging folder (a folder of Coucou's own)
    let mut push = ssh(host)
        .arg(format!("mkdir -p ~/.hermes/state/coucou/outbox && cat > ~/.hermes/state/coucou/outbox/{staged}"))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "ssh is not installed".to_string())?;
    {
        let mut stdin = push.stdin.take().ok_or("no pipe to ssh")?;
        let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        std::io::copy(&mut file, &mut stdin).map_err(|e| format!("Sending to the server failed: {e}"))?;
    }
    let out = push.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr).lines().next().unwrap_or("").trim().to_string();
        return Err(if why.is_empty() { format!("Cannot reach {host}") } else { why });
    }

    // 2. Jinx's own function puts it in OneDrive (and removes the staged copy)
    let args = json!({ "file": staged, "folder": folder, "name": name }).to_string();
    let mut run = ssh(host)
        .arg(format!("python3 - {}", sh_quote(&args)))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "ssh is not installed".to_string())?;
    if let Some(mut stdin) = run.stdin.take() {
        let _ = stdin.write_all(UPLOADER.as_bytes());
    }
    let out = run.wait_with_output().map_err(|e| e.to_string())?;
    let result = parse_school(&String::from_utf8_lossy(&out.stdout));
    if result.is_err() {
        // Whatever went wrong, the staged copy does not stay on the server.
        let _ = ssh(host).arg(format!("rm -f ~/.hermes/state/coucou/outbox/{staged}")).stdin(Stdio::null()).output();
    }
    result
}

/// Starts Jinx's device-code sign-in for the school account on the server, handing each line it
/// prints (the page to open, the code to enter) to `on_line`. Returns once it has finished.
pub fn school_login(host: &str, mut on_line: impl FnMut(String)) -> Result<(), String> {
    if !crate::sysmon::safe_word(host) {
        return Err("The server's name is not valid".into());
    }
    let mut child = ssh(host)
        .arg("python3 -u ~/.hermes/scripts/tareas_teams_auth.py")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "ssh is not installed".to_string())?;
    let started = Instant::now();
    let (tx, rx) = std::sync::mpsc::channel::<String>();
    for pipe in [child.stdout.take().map(|p| Box::new(p) as Box<dyn std::io::Read + Send>), child.stderr.take().map(|p| Box::new(p) as Box<dyn std::io::Read + Send>)].into_iter().flatten() {
        let tx = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
    }
    drop(tx);
    // The sign-in waits for you up to a few minutes; this gives it ten.
    while started.elapsed() < Duration::from_secs(600) {
        match rx.recv_timeout(Duration::from_millis(500)) {
            Ok(line) => on_line(line),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                if matches!(child.try_wait(), Ok(Some(_))) && rx.try_recv().is_err() {
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
    let _ = child.kill();
    match child.wait() {
        Ok(s) if s.success() => Ok(()),
        _ => Err("The sign-in did not finish".into()),
    }
}

// ── Google Drive, through rclone ──────────────────────────────────────────────

fn rclone_with(config: Option<&Path>) -> Command {
    let mut c = Command::new("rclone");
    if let Some(cfg) = config {
        c.env("RCLONE_CONFIG", cfg);
    }
    c
}

pub fn drive_state(remote: &str, config: Option<&Path>) -> DriveState {
    let installed = rclone_with(config).arg("version").stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false);
    if !installed || !remote_ok(remote) {
        return DriveState { installed, connected: false };
    }
    let out = rclone_with(config).arg("listremotes").stderr(Stdio::null()).output();
    let connected = out.map(|o| String::from_utf8_lossy(&o.stdout).lines().any(|l| l.trim() == format!("{remote}:"))).unwrap_or(false);
    DriveState { installed, connected }
}

/// Creates the rclone remote for Google Drive: rclone opens the browser for the one-time
/// sign-in and waits for it. The scope reaches only the files rclone creates.
pub fn drive_connect(remote: &str, config: Option<&Path>) -> Result<(), String> {
    if !remote_ok(remote) {
        return Err("Not a valid remote name".into());
    }
    let mut child = rclone_with(config)
        .args(["config", "create", remote, "drive", "scope", "drive.file"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "rclone is not installed".to_string())?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(s)) if s.success() => return Ok(()),
            Ok(Some(_)) => return Err("The Google sign-in did not complete".into()),
            Ok(None) if started.elapsed() > Duration::from_secs(300) => {
                let _ = child.kill();
                return Err("The Google sign-in timed out (5 minutes)".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(400)),
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// Copies `path` to `remote:folder/name`.
pub fn drive_upload(remote: &str, folder: &str, path: &Path, name: &str, config: Option<&Path>) -> Result<Uploaded, String> {
    if !remote_ok(remote) {
        return Err("Not a valid remote name".into());
    }
    let folder = clean_folder(folder, DEFAULT_DRIVE_FOLDER);
    let dest = format!("{remote}:{folder}/{name}");
    let out = rclone_with(config)
        .arg("copyto")
        .arg(path)
        .arg(&dest)
        .stdin(Stdio::null())
        .output()
        .map_err(|_| "rclone is not installed".to_string())?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr);
        let why = why.lines().rev().find(|l| l.contains("ERROR") || l.contains("Failed")).unwrap_or("").trim().chars().take(140).collect::<String>();
        return Err(if why.is_empty() { "rclone could not upload it".into() } else { why });
    }
    Ok(Uploaded { location: format!("Google Drive: {folder}/{name}"), url: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_are_plain_paths_and_fall_back_when_empty() {
        assert_eq!(clean_folder("Materias/BIOLOGIA I/Actividades", "x"), "Materias/BIOLOGIA I/Actividades");
        assert_eq!(clean_folder("  ../../etc//passwd ", "x"), "etc/passwd");
        assert_eq!(clean_folder("a\\b\u{7}c/ ./d", "x"), "a/bc/d");
        assert_eq!(clean_folder("", "Por clasificar"), "Por clasificar");
        assert_eq!(clean_folder("///..///", "Coucou"), "Coucou");
        assert_eq!(clean_folder(&"x/".repeat(100), "f").len(), 120);
    }

    #[test]
    fn staged_names_are_plain_and_unique_by_time() {
        assert_eq!(staging_name("captura de pantalla (1).png", 17), "17-captura_de_pantalla__1_.png");
        assert_eq!(staging_name("a'b;c$d", 5), "5-a_b_c_d");
        assert_ne!(staging_name("x", 1), staging_name("x", 2));
    }

    #[test]
    fn shell_quoting_keeps_a_json_argument_in_one_word() {
        assert_eq!(sh_quote("{\"a\":\"it's\"}"), "'{\"a\":\"it'\\''s\"}'");
    }

    #[test]
    fn jinxs_answer_becomes_an_upload_or_a_named_error() {
        let ok = "noise\n{\"ok\": true, \"onedrive\": \"Por clasificar/a.pdf\", \"bytes\": 3, \"url\": \"https://x/y\"}\n";
        assert_eq!(parse_school(ok), Ok(Uploaded { location: "Por clasificar/a.pdf".into(), url: Some("https://x/y".into()) }));
        let reauth = "{\"ok\": false, \"error\": \"reauth\", \"detalle\": \"invalid_grant\"}";
        assert_eq!(parse_school(reauth), Err("reauth".into()));
        assert!(parse_school("{\"ok\": false, \"error\": \"archivo_muy_grande\"}").unwrap_err().contains("250 MB"));
        assert!(parse_school("{\"ok\": false, \"error\": \"api\", \"detalle\": \"boom\"}").unwrap_err().ends_with("api: boom"));
        assert!(parse_school("Traceback (most recent call last)").is_err());
        assert!(parse_school("").is_err());
    }

    /// Runs the real chain against the REAL server by hand: with a valid Microsoft sign-in this
    /// really uploads a tiny file to the school's OneDrive ("Por clasificar/coucou-probe-delete-me.txt").
    /// `COUCOU_PROBE_HOST=server cargo test --lib real_school_probe -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn real_school_probe() {
        let host = std::env::var("COUCOU_PROBE_HOST").expect("COUCOU_PROBE_HOST");
        let file = std::env::temp_dir().join("coucou-probe-delete-me.txt");
        std::fs::write(&file, "probe").unwrap();
        println!("result: {:?}", school_upload(&host, &file, "coucou-probe-delete-me.txt", "Por clasificar"));
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn drive_uploads_through_rclone_to_a_named_folder() {
        // A throw-away rclone config whose remote is a plain local folder stands in for Drive.
        let dir = std::env::temp_dir().join(format!("coucou-drive-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("cloud")).unwrap();
        let cfg = dir.join("rclone.conf");
        std::fs::write(&cfg, format!("[testdrive]\ntype = alias\nremote = {}\n", dir.join("cloud").display())).unwrap();
        let file = dir.join("hola.txt");
        std::fs::write(&file, "hola").unwrap();
        if !drive_state("testdrive", Some(&cfg)).installed {
            return; // no rclone on this machine: nothing to check
        }
        assert!(drive_state("testdrive", Some(&cfg)).connected);
        assert!(!drive_state("nope", Some(&cfg)).connected);
        let up = drive_upload("testdrive", "Coucou/sub", &file, "hola.txt", Some(&cfg)).unwrap();
        assert_eq!(up.location, "Google Drive: Coucou/sub/hola.txt");
        assert_eq!(std::fs::read_to_string(dir.join("cloud/Coucou/sub/hola.txt")).unwrap(), "hola");
        assert!(drive_upload("bad remote!", "x", &file, "f", Some(&cfg)).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
