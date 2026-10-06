// The dictation indicator: when Handy (the speech-to-text app) starts recording, the island says
// so, and says when it stops. Handy has no way to tell another program what it is doing, but
// recording shows in the sound server as a program capturing the microphone, so the island
// listens to the server's own announcements (`pactl subscribe`) and looks at who is capturing
// only when something about a capture changes. Nothing polls: while nobody starts or stops a
// capture the process is asleep in a read and costs nothing.
//
// Only the fact "Handy is capturing" is sent to the page; no audio is ever opened or read here.

use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::island::WINDOW_LABEL;
use crate::log;

static STARTED: AtomicBool = AtomicBool::new(false);

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Dictation {
    pub recording: bool,
}

/// Is a block of `pactl list source-outputs` a capture by Handy? (Its monitor streams, which only
/// listen to what is playing, do not count.)
fn is_handy(block: &str) -> bool {
    let b = block.to_lowercase();
    let handy = b.contains("application.process.binary = \"handy\"") || b.contains("application.name = \"handy\"");
    let monitor = b.contains("node.passive = \"true\"") || b.contains("stream.monitor = \"true\"");
    handy && !monitor
}

/// Does the listing show Handy recording?
pub fn handy_recording(listing: &str) -> bool {
    listing.split("Source Output #").skip(1).any(is_handy)
}

/// Whether a line of `pactl subscribe` is about a capture starting, stopping or changing.
fn about_a_capture(line: &str) -> bool {
    line.contains("source-output")
}

fn recording_now() -> bool {
    Command::new("pactl")
        .args(["list", "source-outputs"])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .is_some_and(|o| handy_recording(&String::from_utf8_lossy(&o.stdout)))
}

/// Starts watching (once). Without `pactl` there is nothing to watch and it quietly gives up.
pub fn start(app: &AppHandle) {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || watch(app));
}

fn watch(app: AppHandle) {
    let mut last = false;
    loop {
        let child = Command::new("pactl")
            .arg("subscribe")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(_) => {
                log::line("dictation indicator: pactl is not installed");
                STARTED.store(false, Ordering::SeqCst);
                return;
            }
        };
        if let Some(out) = child.stdout.take() {
            // A capture that was already on when the island started.
            last = report(&app, last, recording_now());
            for line in BufReader::new(out).lines().map_while(Result::ok) {
                if about_a_capture(&line) {
                    last = report(&app, last, recording_now());
                }
            }
        }
        let _ = child.wait();
        // The sound server went away (a restart, a logout): look again in a little while.
        last = report(&app, last, false);
        std::thread::sleep(std::time::Duration::from_secs(5));
    }
}

/// Tells the page when the state changed; returns the state now.
fn report(app: &AppHandle, was: bool, now: bool) -> bool {
    if now != was {
        let _ = app.emit_to(WINDOW_LABEL, "dictation", Dictation { recording: now });
    }
    now
}

#[cfg(test)]
mod tests {
    use super::*;

    const HANDY: &str = "Source Output #77\n\tDriver: PipeWire\n\tProperties:\n\t\tapplication.name = \"Handy\"\n\t\tapplication.process.binary = \"handy\"\n\t\tnode.name = \"alsa_capture.handy\"\n";
    const CAVA: &str = "Source Output #78\n\tProperties:\n\t\tapplication.name = \"cava\"\n\t\tapplication.process.binary = \"cava\"\n";
    const HANDY_MONITOR: &str = "Source Output #79\n\tProperties:\n\t\tapplication.name = \"Handy\"\n\t\tstream.monitor = \"true\"\n";
    const OTHER: &str = "Source Output #80\n\tProperties:\n\t\tapplication.name = \"Handyman Planner\"\n\t\tapplication.process.binary = \"handyman\"\n";

    #[test]
    fn only_handy_capturing_counts() {
        assert!(handy_recording(HANDY));
        assert!(handy_recording(&format!("{CAVA}{HANDY}")), "among others");
        assert!(!handy_recording(CAVA));
        assert!(!handy_recording(HANDY_MONITOR), "a monitor only listens to what plays");
        assert!(!handy_recording(OTHER), "another program with handy in its name");
        assert!(!handy_recording(""));
    }

    #[test]
    fn only_capture_announcements_are_looked_at() {
        assert!(about_a_capture("Event 'new' on source-output #77"));
        assert!(about_a_capture("Event 'remove' on source-output #77"));
        assert!(!about_a_capture("Event 'change' on sink #3"));
        assert!(!about_a_capture("Event 'change' on server"));
    }
}
