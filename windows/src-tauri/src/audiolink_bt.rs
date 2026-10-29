// Moving the Bluetooth headphones to or from this computer by hand (the audio relay's manual escape hatch).
// Only `bluetoothctl` is run, always with fixed arguments and a device address that passed `is_mac`, no shell.
// The computer can connect and disconnect the headphones itself; the phone cannot (Android asks the person),
// so the phone's side of a swap is a notification (see AudioLinkService on the phone).

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// `AA:BB:CC:DD:EE:FF`.
pub fn is_mac(s: &str) -> bool {
    let parts: Vec<&str> = s.split(':').collect();
    parts.len() == 6 && parts.iter().all(|p| p.len() == 2 && p.chars().all(|c| c.is_ascii_hexdigit()))
}

/// `(address, name)` of each `Device <mac> <name>` line of `bluetoothctl devices`.
pub fn parse_devices(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|l| {
            let rest = l.trim().strip_prefix("Device ")?;
            let (mac, name) = rest.split_once(' ').unwrap_or((rest, ""));
            is_mac(mac).then(|| (mac.to_string(), name.trim().to_string()))
        })
        .collect()
}

/// What `bluetoothctl info <mac>` says that matters here.
#[derive(Debug, PartialEq, Eq)]
pub struct Info {
    pub icon: String,
    pub paired: bool,
    pub connected: bool,
}

pub fn parse_info(text: &str) -> Info {
    let field = |key: &str| {
        text.lines().find_map(|l| l.trim().strip_prefix(key).and_then(|v| v.strip_prefix(':')).map(|v| v.trim().to_string()))
    };
    Info {
        icon: field("Icon").unwrap_or_default(),
        paired: field("Paired").as_deref() == Some("yes"),
        connected: field("Connected").as_deref() == Some("yes"),
    }
}

/// True for the icons BlueZ gives headphones and headsets.
pub fn is_headphones(info: &Info) -> bool {
    matches!(info.icon.as_str(), "audio-headphones" | "audio-headset")
}

// ── bluetoothctl ───────────────────────────────────────────────────────────────

fn bluetoothctl(args: &[&str], limit: Duration) -> Result<String, String> {
    let mut child = Command::new("bluetoothctl")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("bluetoothctl: {e}"))?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() < limit => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("bluetoothctl took too long".into());
            }
        }
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// The first paired device that is headphones or a headset, if Bluetooth is on and there is one.
pub fn find_headphones_mac() -> Option<String> {
    let listed = bluetoothctl(&["devices"], Duration::from_secs(5)).ok()?;
    parse_devices(&listed).into_iter().find_map(|(mac, _)| {
        let info = parse_info(&bluetoothctl(&["info", &mac], Duration::from_secs(5)).ok()?);
        (info.paired && is_headphones(&info)).then_some(mac)
    })
}

pub fn bluez_connect(mac: &str) -> Result<(), String> {
    if !is_mac(mac) {
        return Err("not a Bluetooth address".into());
    }
    let out = bluetoothctl(&["connect", mac], Duration::from_secs(20))?;
    if out.contains("Connection successful") {
        Ok(())
    } else {
        Err("the headphones did not connect".into())
    }
}

pub fn bluez_disconnect(mac: &str) -> Result<(), String> {
    if !is_mac(mac) {
        return Err("not a Bluetooth address".into());
    }
    let out = bluetoothctl(&["disconnect", mac], Duration::from_secs(20))?;
    if out.contains("Successful disconnected") {
        Ok(())
    } else {
        Err("the headphones did not disconnect".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INFO_HEADSET: &str = "Device AC:BF:71:A1:21:71 (public)\n\
        \tName: Bose QC Earbuds II\n\
        \tAlias: Bose QC Earbuds II\n\
        \tClass: 0x00240418\n\
        \tIcon: audio-headset\n\
        \tPaired: yes\n\
        \tBonded: yes\n\
        \tTrusted: yes\n\
        \tConnected: no\n";

    const INFO_MOUSE: &str = "Device 11:22:33:44:55:66 (random)\n\
        \tName: MX Master\n\
        \tIcon: input-mouse\n\
        \tPaired: yes\n\
        \tConnected: yes\n";

    #[test]
    fn is_mac_accepts_only_an_address() {
        assert!(is_mac("AC:BF:71:A1:21:71"));
        assert!(is_mac("ac:bf:71:a1:21:71"));
        assert!(!is_mac("AC:BF:71:A1:21"));
        assert!(!is_mac("AC:BF:71:A1:21:7G"));
        assert!(!is_mac("AC-BF-71-A1-21-71"));
        assert!(!is_mac("AC:BF:71:A1:21:71; rm -rf ~"));
        assert!(!is_mac(""));
    }

    #[test]
    fn parse_devices_reads_the_device_lines_and_ignores_the_rest() {
        let text = "[NEW] Controller 00:11:22:33:44:55 laptop [default]\n\
                    Device AC:BF:71:A1:21:71 Bose QC Earbuds II\n\
                    Device 11:22:33:44:55:66 MX Master\n\
                    Device nonsense not a mac\n";
        assert_eq!(
            parse_devices(text),
            vec![
                ("AC:BF:71:A1:21:71".to_string(), "Bose QC Earbuds II".to_string()),
                ("11:22:33:44:55:66".to_string(), "MX Master".to_string()),
            ]
        );
        assert_eq!(parse_devices("No default controller available\n"), vec![]);
    }

    #[test]
    fn parse_info_reads_icon_paired_and_connected() {
        assert_eq!(parse_info(INFO_HEADSET), Info { icon: "audio-headset".into(), paired: true, connected: false });
        assert_eq!(parse_info(INFO_MOUSE), Info { icon: "input-mouse".into(), paired: true, connected: true });
    }

    #[test]
    fn only_headphones_and_headsets_are_headphones() {
        assert!(is_headphones(&parse_info(INFO_HEADSET)));
        assert!(is_headphones(&Info { icon: "audio-headphones".into(), paired: true, connected: false }));
        assert!(!is_headphones(&parse_info(INFO_MOUSE)));
        assert!(!is_headphones(&Info { icon: "audio-card".into(), paired: true, connected: true }));
    }
}
