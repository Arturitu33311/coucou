// The silent virtual sink the audio relay uses, and the capture of what goes into it. While the relay is on
// (or held), the computer's default output is a null sink called `coucou_audiolink`: every program plays
// into it, nothing comes out of the speakers, and `parec` reads its monitor to send the sound to the phone.
//
// The one thing that must never go wrong is leaving the computer without sound after a crash. So before the
// sink exists, the name of the real default output is written to a marker file
// (`<local dir>/audiolink-sink.json`); `sink_restore` puts everything back and deletes it; and
// `restore_if_stale` does the same at the next start if Coucou died in between. A small watchdog process
// (`coucou-hook --audio-watchdog`, see `spawn_watchdog`) covers the case in between: it holds a pipe open to
// Coucou, and when Coucou disappears, however it disappears, the pipe closes and it restores the output.
//
// Only `pactl` and `parec` are run, always with fixed arguments and no shell. Only the monitor of our own sink
// is recorded, never a microphone.

use std::io::{self, Read};
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::Mutex;

use crate::settings;

pub const SINK: &str = "coucou_audiolink";
/// 20 ms of 48 kHz stereo s16le.
pub const CHUNK: usize = 3840;

static WATCHDOG: Mutex<Option<Child>> = Mutex::new(None);

// ── Pure parts ─────────────────────────────────────────────────────────────────

/// The name `pactl get-default-sink` printed, or None when it is empty or not a plain sink name.
pub fn parse_default_sink(output: &str) -> Option<String> {
    let name = output.trim();
    plain_sink_name(name).then(|| name.to_string())
}

/// A name we are willing to hand back to `pactl`: one word, no control characters, and not our own sink.
fn plain_sink_name(name: &str) -> bool {
    !name.is_empty() && name != SINK && !name.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// `{"previous":"<sink>","module":<id>}`: what has to be undone.
pub fn marker_json(previous: &str, module: u32) -> String {
    serde_json::json!({ "previous": previous, "module": module }).to_string()
}

/// The marker's content, or None when it is not what `marker_json` writes.
pub fn parse_marker(text: &str) -> Option<(String, u32)> {
    let v: serde_json::Value = serde_json::from_str(text).ok()?;
    let previous = v["previous"].as_str().filter(|p| plain_sink_name(p))?;
    let module = u32::try_from(v["module"].as_u64()?).ok()?;
    Some((previous.to_string(), module))
}

/// The index of the sink called `name` in `pactl list short sinks` (tab-separated: index, name, …).
pub fn sink_index(short_sinks: &str, name: &str) -> Option<u32> {
    short_sinks.lines().find_map(|l| {
        let mut f = l.split('\t');
        let index = f.next()?.trim().parse().ok()?;
        (f.next()? == name).then_some(index)
    })
}

/// The sink PipeWire makes for a connected Bluetooth device: `bluez_output.AC_BF_71_A1_21_71.1` (the trailing
/// number varies). `mac` is `AC:BF:71:A1:21:71`, in either case.
pub fn bluez_sink_for(short_sinks: &str, mac: &str) -> Option<String> {
    let prefix = format!("bluez_output.{}.", mac.replace(':', "_").to_uppercase());
    short_sinks.lines().find_map(|l| {
        let name = l.split('\t').nth(1)?;
        name.to_uppercase().starts_with(&prefix.to_uppercase()).then(|| name.to_string())
    })
}

/// Makes the Bluetooth headphones' sink the default output, waiting a few seconds for PipeWire to create it.
/// Restoring the output after the relay writes the *speakers* as the default, so after moving the headphones
/// to this computer the sound would stay on the speakers; this puts it where the person just asked for it.
pub fn default_to_bluez(mac: &str) -> bool {
    for _ in 0..10 {
        let sinks = pactl(&["list", "short", "sinks"]).unwrap_or_default();
        if let Some(name) = bluez_sink_for(&sinks, mac) {
            if plain_sink_name(&name) && pactl(&["set-default-sink", &name]).is_ok() {
                return true;
            }
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    false
}

/// The module that made our sink, in `pactl list short modules` (index, name, arguments), if it is loaded.
pub fn module_with_sink(short_modules: &str) -> Option<u32> {
    short_modules
        .lines()
        .find(|l| l.split_whitespace().any(|w| w == format!("sink_name={SINK}")))
        .and_then(|l| l.split('\t').next()?.trim().parse().ok())
}

// ── The sink ───────────────────────────────────────────────────────────────────

fn pactl(args: &[&str]) -> Result<String, String> {
    let out = Command::new("pactl")
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("pactl: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(format!("pactl {} failed", args.join(" ")))
    }
}

fn marker_path() -> std::path::PathBuf {
    settings::local_dir().join("audiolink-sink.json")
}

fn write_marker(previous: &str, module: u32) -> Result<(), String> {
    let path = marker_path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, marker_json(previous, module)).map_err(|e| e.to_string())
}

/// Moves the computer's output to the silent sink: remembers the real one first, then makes the sink, makes
/// it the default and moves every playing stream onto it. Safe to call when it is already there.
pub fn sink_virtual() -> Result<(), String> {
    restore_if_stale();
    let previous = parse_default_sink(&pactl(&["get-default-sink"])?).ok_or("no usable default output")?;
    write_marker(&previous, 0)?;
    let loaded = pactl(&[
        "load-module",
        "module-null-sink",
        &format!("sink_name={SINK}"),
        "sink_properties=device.description=Coucou-relay",
    ]);
    let module: u32 = match loaded.as_ref().ok().and_then(|t| t.trim().parse().ok()) {
        Some(m) => m,
        None => {
            let _ = std::fs::remove_file(marker_path());
            return Err(loaded.err().unwrap_or_else(|| "pactl load-module gave no module id".into()));
        }
    };
    write_marker(&previous, module)?;
    pactl(&["set-default-sink", SINK])?;
    for input in inputs_on(&pactl(&["list", "short", "sink-inputs"]).unwrap_or_default(), None) {
        let _ = pactl(&["move-sink-input", &input.to_string(), SINK]);
    }
    Ok(())
}

/// Puts the output back where the marker says it was and removes the silent sink. Safe to call twice.
pub fn sink_restore() -> Result<(), String> {
    let Ok(text) = std::fs::read_to_string(marker_path()) else { return Ok(()) };
    let Some((previous, module)) = parse_marker(&text) else {
        let _ = std::fs::remove_file(marker_path());
        return Err("the sink marker was not readable".into());
    };
    let mut first_err = None;
    let mut note = |r: Result<String, String>| {
        if let Err(e) = r {
            first_err.get_or_insert(e);
        }
    };
    let ours = sink_index(&pactl(&["list", "short", "sinks"]).unwrap_or_default(), SINK);
    note(pactl(&["set-default-sink", &previous]));
    if let Some(sink) = ours {
        for input in inputs_on(&pactl(&["list", "short", "sink-inputs"]).unwrap_or_default(), Some(sink)) {
            note(pactl(&["move-sink-input", &input.to_string(), &previous]));
        }
    }
    // The id is 0 when Coucou died between making the sink and writing it down.
    let module = if module > 0 { Some(module) } else { module_with_sink(&pactl(&["list", "short", "modules"]).unwrap_or_default()) };
    if let Some(m) = module {
        note(pactl(&["unload-module", &m.to_string()]));
    }
    let _ = std::fs::remove_file(marker_path());
    first_err.map_or(Ok(()), Err)
}

/// At start: if Coucou died with the relay on, the marker is still there; undo it.
pub fn restore_if_stale() {
    if marker_path().exists() {
        crate::log::line("audiolink: putting the output back after an unclean stop");
        if let Err(e) = sink_restore() {
            crate::log::line(format!("audiolink: restore: {e}"));
        }
    }
}

/// Starts the watchdog (`coucou-hook --audio-watchdog`) with a pipe it waits on. When Coucou ends, however
/// it ends, the pipe closes and the watchdog restores the output by itself.
pub fn spawn_watchdog() {
    let mut slot = WATCHDOG.lock().unwrap();
    if slot.is_some() {
        return;
    }
    match Command::new(settings::hook_exe_path())
        .arg("--audio-watchdog")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => *slot = Some(child),
        Err(e) => crate::log::line(format!("audiolink: no watchdog ({e})")),
    }
}

/// Lets the watchdog go: closing its pipe makes it run, and finding nothing to undo, end.
pub fn stop_watchdog() {
    if let Some(mut child) = WATCHDOG.lock().unwrap().take() {
        drop(child.stdin.take());
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

// ── The capture ────────────────────────────────────────────────────────────────

/// `parec` reading our sink's monitor. Ends (and is reaped) when dropped.
pub struct Capture {
    child: Child,
    out: ChildStdout,
}

pub fn capture_start() -> Result<Capture, String> {
    let mut child = Command::new("parec")
        .args([&format!("--device={SINK}.monitor") as &str, "--format=s16le", "--rate=48000", "--channels=2", "--latency-msec=20"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("parec: {e}"))?;
    let out = child.stdout.take().ok_or("parec gave no output")?;
    Ok(Capture { child, out })
}

impl Capture {
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// One 20 ms chunk. False when the capture has ended.
    pub fn read_chunk(&mut self, buf: &mut [u8; CHUNK]) -> io::Result<bool> {
        match self.out.read_exact(buf) {
            Ok(()) => Ok(true),
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => Ok(false),
            Err(e) => Err(e),
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The sink inputs in `pactl list short sink-inputs` (index, sink index, …) playing on sink `sink`;
/// `None` means every one of them.
pub fn inputs_on(short_inputs: &str, sink: Option<u32>) -> Vec<u32> {
    short_inputs
        .lines()
        .filter_map(|l| {
            let mut f = l.split('\t');
            let input: u32 = f.next()?.trim().parse().ok()?;
            let on: u32 = f.next()?.trim().parse().ok()?;
            sink.map_or(true, |s| s == on).then_some(input)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SINKS: &str = "3\talsa_output.pci-0000_00_1f.3.analog-stereo\tPipeWire\ts32le 2ch 48000Hz\tSUSPENDED\n\
                         57\tcoucou_audiolink\tPipeWire\ts16le 2ch 48000Hz\tIDLE\n";
    const INPUTS: &str = "91\t57\t120\tPipeWire\tfloat32le 2ch 48000Hz\n\
                          92\t3\t121\tPipeWire\tfloat32le 2ch 48000Hz\n\
                          93\t57\t122\tPipeWire\tfloat32le 2ch 48000Hz\n";

    #[test]
    fn parse_default_sink_reads_pactl_get_default_sink() {
        assert_eq!(
            parse_default_sink("alsa_output.pci-0000_00_1f.3.analog-stereo\n").as_deref(),
            Some("alsa_output.pci-0000_00_1f.3.analog-stereo")
        );
        assert_eq!(parse_default_sink("bluez_output.AC_BF_71_A1_21_71.1"), Some("bluez_output.AC_BF_71_A1_21_71.1".into()));
        assert_eq!(parse_default_sink("\n"), None);
        assert_eq!(parse_default_sink("two words\n"), None);
        assert_eq!(parse_default_sink("a\nb\n"), None);
    }

    #[test]
    fn the_default_sink_being_ours_is_not_a_previous_one() {
        assert_eq!(parse_default_sink("coucou_audiolink\n"), None);
    }

    #[test]
    fn marker_round_trips() {
        let json = marker_json("alsa_output.test", 77);
        assert_eq!(parse_marker(&json), Some(("alsa_output.test".to_string(), 77)));
    }

    #[test]
    fn parse_marker_rejects_garbage() {
        assert_eq!(parse_marker(""), None);
        assert_eq!(parse_marker("not json"), None);
        assert_eq!(parse_marker(r#"{"previous":"","module":1}"#), None);
        assert_eq!(parse_marker(r#"{"previous":"has space","module":1}"#), None);
        assert_eq!(parse_marker(r#"{"previous":"x","module":"1"}"#), None);
        assert_eq!(parse_marker(r#"{"previous":"coucou_audiolink","module":1}"#), None);
    }

    #[test]
    fn sink_index_finds_our_sink_by_exact_name() {
        assert_eq!(sink_index(SINKS, SINK), Some(57));
        assert_eq!(sink_index(SINKS, "coucou_audio"), None);
        assert_eq!(sink_index("", SINK), None);
    }

    /// Touches the real audio system for about a second: run by hand with `cargo test live_sink -- --ignored`.
    /// It refuses to run while anything is playing, and always leaves the output where it found it.
    #[test]
    #[ignore]
    fn live_sink_round_trip() {
        if !inputs_on(&pactl(&["list", "short", "sink-inputs"]).unwrap(), None).is_empty() {
            eprintln!("something is playing: not touching the audio");
            return;
        }
        let before = parse_default_sink(&pactl(&["get-default-sink"]).unwrap()).expect("a default output");
        sink_virtual().unwrap();
        let during = pactl(&["get-default-sink"]).unwrap();
        let sinks = pactl(&["list", "short", "sinks"]).unwrap();
        let _ = capture_start().map(|mut c| {
            let mut buf = [0u8; CHUNK];
            assert!(c.read_chunk(&mut buf).unwrap(), "the monitor must give a chunk of silence");
        });
        sink_restore().unwrap();
        let after = parse_default_sink(&pactl(&["get-default-sink"]).unwrap()).unwrap();
        let sinks_after = pactl(&["list", "short", "sinks"]).unwrap();
        assert_eq!(during.trim(), SINK);
        assert!(sink_index(&sinks, SINK).is_some());
        assert_eq!(after, before);
        assert!(sink_index(&sinks_after, SINK).is_none(), "the silent sink must be gone");
        assert!(!marker_path().exists());
    }

    #[test]
    fn bluez_sink_for_finds_the_headphones_sink_by_address() {
        let sinks = "3\talsa_output.pci-0000_00_1f.3.analog-stereo\tPipeWire\ts32le 2ch 48000Hz\tSUSPENDED\n\
                     12\tbluez_output.AC_BF_71_A1_21_71.1\tPipeWire\ts16le 2ch 48000Hz\tIDLE\n\
                     13\tbluez_output.11_22_33_44_55_66.1\tPipeWire\ts16le 2ch 48000Hz\tIDLE\n";
        assert_eq!(bluez_sink_for(sinks, "AC:BF:71:A1:21:71").as_deref(), Some("bluez_output.AC_BF_71_A1_21_71.1"));
        assert_eq!(bluez_sink_for(sinks, "ac:bf:71:a1:21:71").as_deref(), Some("bluez_output.AC_BF_71_A1_21_71.1"));
        assert_eq!(bluez_sink_for(sinks, "99:99:99:99:99:99"), None);
        assert_eq!(bluez_sink_for("", "AC:BF:71:A1:21:71"), None);
    }

    #[test]
    fn module_with_sink_finds_our_null_sink_module() {
        let modules = "10\tmodule-alsa-card\tdevice_id=1\n\
                       44\tmodule-null-sink\tsink_name=coucou_audiolink sink_properties=device.description=Coucou\n\
                       45\tmodule-null-sink\tsink_name=other\n";
        assert_eq!(module_with_sink(modules), Some(44));
        assert_eq!(module_with_sink("45\tmodule-null-sink\tsink_name=other\n"), None);
    }

    #[test]
    fn inputs_on_picks_only_the_streams_on_that_sink() {
        assert_eq!(inputs_on(INPUTS, Some(57)), vec![91, 93]);
        assert_eq!(inputs_on(INPUTS, Some(3)), vec![92]);
        assert_eq!(inputs_on(INPUTS, None), vec![91, 92, 93]);
        assert_eq!(inputs_on("junk line\n", None), Vec::<u32>::new());
    }
}
