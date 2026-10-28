//! `coucou-hook --audio-watchdog` holds still while Coucou's pipe stays open and puts the computer's sound
//! back the moment it closes (Coucou quit, was killed, crashed). It runs a fake `pactl` here, so the real
//! audio system is never touched.

#![cfg(target_os = "linux")]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const FAKE_PACTL: &str = r#"#!/bin/sh
echo "$@" >> "$FAKE_LOG"
case "$1 $2 $3" in
  "list short sinks") printf '3\talsa_output.test\tPipeWire\n57\tcoucou_audiolink\tPipeWire\n' ;;
  "list short sink-inputs") printf '91\t57\t1\tx\n92\t3\t2\tx\n' ;;
esac
"#;

struct Scene {
    dir: PathBuf,
}

impl Scene {
    fn new(tag: &str, marker: Option<&str>) -> Scene {
        let dir = std::env::temp_dir().join(format!("coucou-watchdog-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        std::fs::create_dir_all(dir.join("data/coucou")).unwrap();
        let pactl = dir.join("bin/pactl");
        std::fs::write(&pactl, FAKE_PACTL).unwrap();
        std::fs::set_permissions(&pactl, std::fs::Permissions::from_mode(0o755)).unwrap();
        if let Some(m) = marker {
            std::fs::write(dir.join("data/coucou/audiolink-sink.json"), m).unwrap();
        }
        Scene { dir }
    }

    fn marker(&self) -> PathBuf {
        self.dir.join("data/coucou/audiolink-sink.json")
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.dir.join("log")).unwrap_or_default()
    }

    fn spawn(&self) -> std::process::Child {
        Command::new(env!("CARGO_BIN_EXE_coucou-hook"))
            .arg("--audio-watchdog")
            .env("PATH", format!("{}:/usr/bin:/bin", self.dir.join("bin").display()))
            .env("XDG_DATA_HOME", self.dir.join("data"))
            .env("FAKE_LOG", self.dir.join("log"))
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    }
}

impl Drop for Scene {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn exits_within(child: &mut std::process::Child, limit: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < limit {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = child.kill();
    false
}

#[test]
fn watchdog_restores_on_eof() {
    let scene = Scene::new("eof", Some(r#"{"previous":"alsa_output.test","module":77}"#));
    let mut child = scene.spawn();
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(scene.log(), "", "nothing may run while Coucou's pipe is open");
    assert!(child.try_wait().unwrap().is_none(), "the watchdog must keep waiting");

    drop(child.stdin.take()); // Coucou is gone
    assert!(exits_within(&mut child, Duration::from_secs(5)), "the watchdog must finish after the pipe closes");

    let log = scene.log();
    assert!(log.contains("set-default-sink alsa_output.test"), "{log}");
    assert!(log.contains("move-sink-input 91 alsa_output.test"), "{log}");
    assert!(!log.contains("move-sink-input 92"), "a stream that was elsewhere must stay there: {log}");
    assert!(log.contains("unload-module 77"), "{log}");
    assert!(!Path::new(&scene.marker()).exists(), "the marker must be deleted");
}

#[test]
fn watchdog_with_nothing_to_undo_runs_nothing() {
    let scene = Scene::new("none", None);
    let mut child = scene.spawn();
    drop(child.stdin.take());
    assert!(exits_within(&mut child, Duration::from_secs(5)));
    assert_eq!(scene.log(), "");
}

#[test]
fn watchdog_ignores_a_marker_it_does_not_understand() {
    let scene = Scene::new("junk", Some(r#"{"previous":"rm -rf ~","module":1}"#));
    let mut child = scene.spawn();
    drop(child.stdin.take());
    assert!(exits_within(&mut child, Duration::from_secs(5)));
    assert_eq!(scene.log(), "", "a name with spaces is never handed to pactl");
}
