// The Hub's Quick tab: the switches you reach for — do not disturb, microphone and speaker
// mute, volume, night light, lock — and what is worth a glance: the Bluetooth devices with
// their batteries, whether something is recording the microphone, the keyboard layout.
//
// Everything goes through the tools of the desktop itself (gsettings, wpctl, pactl,
// bluetoothctl, loginctl); nothing here is a long-running process and nothing assumes a default:
// each switch shows the value it has now. No power-off, restart or log-out lives here.

use std::process::{Command, Stdio};

use serde::Serialize;

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BtDevice {
    pub name: String,
    pub battery: Option<u32>,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Quick {
    /// GNOME's "do not disturb" is the notification banners being off.
    pub dnd: Option<bool>,
    pub mic_muted: Option<bool>,
    /// Something other than Coucou's own visualizer is recording the microphone.
    pub mic_in_use: bool,
    pub sink_muted: Option<bool>,
    pub volume: Option<u32>,
    pub night_light: Option<bool>,
    pub bluetooth: Vec<BtDevice>,
    pub layout: Option<String>,
}

fn run(program: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(program).args(args).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).to_string())
}

fn gsettings_bool(schema: &str, key: &str) -> Option<bool> {
    parse_bool(&run("gsettings", &["get", schema, key])?)
}

// ── Parsers (pure) ────────────────────────────────────────────────────────────

pub fn parse_bool(s: &str) -> Option<bool> {
    match s.trim() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

/// (volume percent, muted) from `wpctl get-volume`: "Volume: 0.40" or "Volume: 0.40 [MUTED]".
pub fn parse_volume(s: &str) -> Option<(u32, bool)> {
    let rest = s.trim().strip_prefix("Volume:")?;
    let level: f64 = rest.split_whitespace().next()?.parse().ok()?;
    Some(((level * 100.0).round().clamp(0.0, 150.0) as u32, rest.contains("[MUTED]")))
}

/// The percentage in `bluetoothctl info`: "Battery Percentage: 0x58 (88)".
pub fn parse_bt_battery(info: &str) -> Option<u32> {
    let line = info.lines().find(|l| l.trim_start().starts_with("Battery Percentage"))?;
    let inside = line.split('(').nth(1)?.split(')').next()?;
    inside.trim().parse().ok()
}

/// "Device AA:BB:CC:DD:EE:FF Name Of It" → (mac, name).
pub fn parse_bt_device(line: &str) -> Option<(String, String)> {
    let rest = line.trim().strip_prefix("Device ")?;
    let (mac, name) = rest.split_once(' ')?;
    (mac.len() == 17 && mac.bytes().all(|b| b.is_ascii_hexdigit() || b == b':')).then(|| (mac.to_string(), name.trim().to_string()))
}

/// The current keyboard layout from the `mru-sources` / `sources` value: "[('xkb', 'us')]" → "us".
pub fn parse_layout(s: &str) -> Option<String> {
    let first = s.split('(').nth(1)?;
    let mut quoted = first.split('\'').skip(1).step_by(2);
    let _kind = quoted.next()?;
    let name = quoted.next()?;
    (!name.is_empty()).then(|| name.to_string())
}

/// Does `pactl list source-outputs` show a program recording that is not Coucou's own
/// visualizer (which declares itself as pavucontrol so the desktop shows no indicator)?
pub fn mic_in_use(listing: &str) -> bool {
    listing
        .split("Source Output #")
        .skip(1)
        .any(|block| {
            let lower = block.to_lowercase();
            // Coucou's own visualizer, and Handy (the user's own dictation, which the island shows itself).
            let ours = block.contains("org.PulseAudio.pavucontrol")
                || block.contains("application.process.binary = \"cava\"")
                || block.contains("application.name = \"cava\"")
                || lower.contains("application.process.binary = \"handy\"")
                || lower.contains("application.name = \"handy\"");
            let monitor = block.contains("node.passive = \"true\"") || block.contains("stream.monitor = \"true\"");
            !ours && !monitor
        })
}

// ── Reading and setting ───────────────────────────────────────────────────────

pub fn read() -> Quick {
    let mut q = Quick::default();
    q.dnd = gsettings_bool("org.gnome.desktop.notifications", "show-banners").map(|banners| !banners);
    q.night_light = gsettings_bool("org.gnome.settings-daemon.plugins.color", "night-light-enabled");
    if let Some((vol, muted)) = run("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"]).and_then(|s| parse_volume(&s)) {
        q.volume = Some(vol);
        q.sink_muted = Some(muted);
    }
    q.mic_muted = run("wpctl", &["get-volume", "@DEFAULT_AUDIO_SOURCE@"]).and_then(|s| parse_volume(&s)).map(|(_, m)| m);
    q.mic_in_use = run("pactl", &["list", "source-outputs"]).is_some_and(|s| mic_in_use(&s));
    q.layout = run("gsettings", &["get", "org.gnome.desktop.input-sources", "mru-sources"])
        .and_then(|s| parse_layout(&s))
        .or_else(|| run("gsettings", &["get", "org.gnome.desktop.input-sources", "sources"]).and_then(|s| parse_layout(&s)));
    if let Some(list) = run("bluetoothctl", &["devices", "Connected"]) {
        q.bluetooth = list
            .lines()
            .filter_map(parse_bt_device)
            .take(4)
            .map(|(mac, name)| BtDevice { battery: run("bluetoothctl", &["info", &mac]).and_then(|i| parse_bt_battery(&i)), name })
            .collect();
    }
    q
}

/// One switch. `what` is one of a short list; nothing else reaches a command.
pub fn set(what: &str, value: i64) -> Result<(), String> {
    let ok = |r: Option<String>| r.map(|_| ()).ok_or_else(|| format!("Could not change {what}"));
    match what {
        // do not disturb on = banners off
        "dnd" => ok(run("gsettings", &["set", "org.gnome.desktop.notifications", "show-banners", if value != 0 { "false" } else { "true" }])),
        "night" => ok(run("gsettings", &["set", "org.gnome.settings-daemon.plugins.color", "night-light-enabled", if value != 0 { "true" } else { "false" }])),
        "mic" => ok(run("wpctl", &["set-mute", "@DEFAULT_AUDIO_SOURCE@", if value != 0 { "1" } else { "0" }])),
        "sink" => ok(run("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", if value != 0 { "1" } else { "0" }])),
        "volume" => ok(run("wpctl", &["set-volume", "@DEFAULT_AUDIO_SINK@", &format!("{}%", value.clamp(0, 100))])),
        "lock" => ok(run("loginctl", &["lock-session"])),
        _ => Err("unknown switch".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads the REAL desktop (read-only; changes nothing): run by hand with
    /// `cargo test --lib real_quick_probe -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_quick_probe() {
        println!("quick: {:?}", read());
    }

    #[test]
    fn switches_and_levels_are_read_as_they_are() {
        assert_eq!(parse_bool("true\n"), Some(true));
        assert_eq!(parse_bool("false"), Some(false));
        assert_eq!(parse_bool("uint32 3"), None, "a value that is not a boolean is not guessed");
        assert_eq!(parse_volume("Volume: 0.40\n"), Some((40, false)));
        assert_eq!(parse_volume("Volume: 1.00 [MUTED]"), Some((100, true)));
        assert_eq!(parse_volume("nope"), None);
    }

    #[test]
    fn bluetooth_devices_and_batteries_and_the_layout() {
        let (mac, name) = parse_bt_device("Device 00:1A:7D:DA:71:13 WH-1000XM4 Sony").unwrap();
        assert_eq!((mac.as_str(), name.as_str()), ("00:1A:7D:DA:71:13", "WH-1000XM4 Sony"));
        assert!(parse_bt_device("Device nonsense X").is_none());
        assert_eq!(parse_bt_battery("Name: x\n\tBattery Percentage: 0x58 (88)\n"), Some(88));
        assert_eq!(parse_bt_battery("Name: x\n"), None);
        assert_eq!(parse_layout("[('xkb', 'us')]\n").as_deref(), Some("us"));
        assert_eq!(parse_layout("[('xkb', 'latam'), ('xkb', 'us')]").as_deref(), Some("latam"));
        assert_eq!(parse_layout("@a(ss) []"), None);
    }

    #[test]
    fn only_other_programs_recording_count_as_the_microphone_in_use() {
        let cava = "Source Output #5\n\tProperties:\n\t\tapplication.id = \"org.PulseAudio.pavucontrol\"\n\t\tapplication.process.binary = \"cava\"\n";
        let zoom = "Source Output #9\n\tProperties:\n\t\tapplication.name = \"zoom\"\n";
        assert!(!mic_in_use(cava));
        assert!(mic_in_use(zoom));
        assert!(mic_in_use(&format!("{cava}{zoom}")));
        assert!(!mic_in_use(""));
    }

    #[test]
    fn only_known_switches_reach_a_command() {
        assert!(set("reboot", 1).is_err());
        assert!(set("rm -rf", 1).is_err());
    }
}
