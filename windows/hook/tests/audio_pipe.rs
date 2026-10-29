//! `coucou-hook --audio` and `--remote` are what a phone's ssh keys may run. Each only joins its stdin and
//! stdout to one private socket in the user's runtime directory; this proves that, and that the two keys
//! reach different sockets.

#![cfg(target_os = "linux")]

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn private_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("coucou-hook-test-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    dir
}

fn hook(flag: &str, dir: &PathBuf) -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_coucou-hook"))
        .arg(flag)
        .env("XDG_RUNTIME_DIR", dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap()
}

/// A socket that answers every line with the same line, upper-cased.
fn echo_server(dir: &PathBuf, socket: &str) {
    let listener = UnixListener::bind(dir.join(socket)).unwrap();
    std::thread::spawn(move || {
        if let Ok((mut s, _)) = listener.accept() {
            let mut buf = [0u8; 256];
            while let Ok(n) = s.read(&mut buf) {
                if n == 0 {
                    break;
                }
                let up: Vec<u8> = buf[..n].iter().map(|b| b.to_ascii_uppercase()).collect();
                if s.write_all(&up).is_err() {
                    break;
                }
            }
        }
    });
}

fn forwards_both_ways(flag: &str, socket: &str) {
    let dir = private_dir(flag.trim_start_matches('-'));
    echo_server(&dir, socket);
    let mut child = hook(flag, &dir);
    child.stdin.as_mut().unwrap().write_all(b"ping\n").unwrap();
    // Read in a thread so a hook that does not answer fails the test instead of hanging it.
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut line = String::new();
        let _ = BufReader::new(stdout).read_line(&mut line);
        let _ = tx.send(line);
    });
    let got = rx.recv_timeout(std::time::Duration::from_secs(5));
    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(got.as_deref(), Ok("PING\n"));
}

fn says_offline_without_a_socket(flag: &str) {
    let dir = private_dir(&format!("off{}", flag.trim_start_matches('-')));
    let out = hook(flag, &dir).wait_with_output().unwrap();
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), "{\"t\":\"offline\"}\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn audio_pipe_forwards_both_ways() {
    forwards_both_ways("--audio", "coucou-audio.sock");
}

#[test]
fn audio_pipe_offline_when_no_socket() {
    says_offline_without_a_socket("--audio");
}

#[test]
fn remote_pipe_forwards_both_ways() {
    forwards_both_ways("--remote", "coucou-remote.sock");
}

#[test]
fn remote_pipe_offline_when_no_socket() {
    says_offline_without_a_socket("--remote");
}

#[test]
fn each_flag_reaches_only_its_own_socket() {
    let dir = private_dir("cross");
    echo_server(&dir, "coucou-remote.sock"); // only the phone link exists
    let out = hook("--audio", &dir).wait_with_output().unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout), "{\"t\":\"offline\"}\n");
    let _ = std::fs::remove_dir_all(&dir);
}
