// The batteries of what is plugged or paired to the computer: Bluetooth headphones, a mouse, a
// keyboard, a game controller. UPower (the same system service the laptop's own battery comes
// from, which also exposes what BlueZ reports) announces devices coming and going and their level
// changing, so nothing is polled: the threads below sleep in a read until the system says
// something. The page is told only when the list, as it would be shown, differs.
//
// Only a name, a kind, a percentage and whether it charges are read. Without UPower or a system
// bus it stays silent.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use zbus::blocking::{Connection, MessageIterator};

use crate::island::WINDOW_LABEL;
use crate::log;

static STARTED: AtomicBool = AtomicBool::new(false);
static CURRENT: Mutex<Vec<Peripheral>> = Mutex::new(Vec::new());

const UPOWER: &str = "org.freedesktop.UPower";
const UPOWER_PATH: &str = "/org/freedesktop/UPower";
const DEVICES_NS: &str = "/org/freedesktop/UPower/devices";
const DEVICE: &str = "org.freedesktop.UPower.Device";

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Peripheral {
    /// The device's UPower object name, stable while it stays paired.
    pub id: String,
    pub name: String,
    /// "headphones", "mouse", "keyboard", "gamepad", …
    pub kind: String,
    pub percent: u8,
    pub charging: bool,
}

/// UPower's device type → a short kind, or None for what is not a peripheral worth a line
/// (mains, the computer's own battery and a UPS are handled elsewhere).
pub fn kind_of(device_type: u32, power_supply: bool) -> Option<&'static str> {
    if power_supply {
        return None;
    }
    match device_type {
        1 | 2 | 3 | 4 | 11 | 0 => None, // line power, battery, UPS, monitor, computer, unknown
        5 => Some("mouse"),
        6 => Some("keyboard"),
        7 | 8 | 10 => Some("phone"),
        9 => Some("player"),
        12 => Some("gamepad"),
        13 => Some("pen"),
        14 => Some("touchpad"),
        17 | 19 => Some("headphones"),
        18 | 21 => Some("speaker"),
        22 => Some("remote"),
        26 => Some("wearable"),
        _ => Some("device"),
    }
}

/// A line for one device, or None when UPower has no level for it (a present device that does
/// not report one is not worth a line that would say 0 %).
pub fn describe(
    id: &str,
    device_type: u32,
    power_supply: bool,
    is_present: bool,
    model: &str,
    vendor: &str,
    percentage: f64,
    state: u32,
) -> Option<Peripheral> {
    let kind = kind_of(device_type, power_supply)?;
    if !is_present || !(0.0..=100.0).contains(&percentage) {
        return None;
    }
    // A device that reports exactly 0 and is not discharging has no real level.
    if percentage == 0.0 && state != 2 && state != 3 {
        return None;
    }
    let name = [model.trim(), vendor.trim()]
        .into_iter()
        .find(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| kind.to_string());
    Some(Peripheral { id: id.to_string(), name, kind: kind.to_string(), percent: percentage.round() as u8, charging: matches!(state, 1 | 5) })
}

fn read_all(conn: &Connection) -> Vec<Peripheral> {
    let Ok(reply) = conn.call_method(Some(UPOWER), UPOWER_PATH, Some(UPOWER), "EnumerateDevices", &()) else {
        return Vec::new();
    };
    let Ok(paths) = reply.body().deserialize::<Vec<zbus::zvariant::OwnedObjectPath>>() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for p in paths {
        let path = p.as_str();
        let Some(proxy) = crate::sysstate::device_proxy(conn, path, DEVICE) else { continue };
        let id = path.rsplit('/').next().unwrap_or(path);
        let dev = describe(
            id,
            proxy.get_property::<u32>("Type").unwrap_or(0),
            proxy.get_property::<bool>("PowerSupply").unwrap_or(false),
            proxy.get_property::<bool>("IsPresent").unwrap_or(false),
            &proxy.get_property::<String>("Model").unwrap_or_default(),
            &proxy.get_property::<String>("Vendor").unwrap_or_default(),
            proxy.get_property::<f64>("Percentage").unwrap_or(-1.0),
            proxy.get_property::<u32>("State").unwrap_or(0),
        );
        out.extend(dev);
    }
    out.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    out.truncate(8);
    out
}

/// What is known now (the page asks once at start; after that it is told).
pub fn current() -> Vec<Peripheral> {
    CURRENT.lock().unwrap().clone()
}

fn refresh(app: &AppHandle, conn: &Connection) {
    let now = read_all(conn);
    let mut cur = CURRENT.lock().unwrap();
    if *cur != now {
        *cur = now.clone();
        drop(cur);
        let _ = app.emit_to(WINDOW_LABEL, "peripherals", now);
    }
}

/// Starts listening (once).
pub fn start(app: &AppHandle) {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let conn = match Connection::system() {
        Ok(c) => c,
        Err(e) => {
            log::line(format!("device batteries: no system bus ({e})"));
            return;
        }
    };
    *CURRENT.lock().unwrap() = read_all(&conn);
    // A device appearing or going, and any device's properties changing (its level, its state).
    let rules = [
        zbus::MatchRule::builder().msg_type(zbus::message::Type::Signal).interface(UPOWER).and_then(|b| b.path(UPOWER_PATH)).map(|b| b.build()),
        zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface("org.freedesktop.DBus.Properties")
            .and_then(|b| b.member("PropertiesChanged"))
            .and_then(|b| b.path_namespace(DEVICES_NS))
            .map(|b| b.build()),
    ];
    for rule in rules {
        let Ok(rule) = rule else { continue };
        let (c, app) = (conn.clone(), app.clone());
        std::thread::spawn(move || {
            let Ok(iter) = MessageIterator::for_match_rule(rule, &c, Some(64)) else { return };
            for _ in iter.flatten() {
                refresh(&app, &c);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_peripherals_have_a_line() {
        assert_eq!(kind_of(2, true), None, "the computer's own battery");
        assert_eq!(kind_of(1, false), None, "mains");
        assert_eq!(kind_of(3, false), None, "a UPS");
        assert_eq!(kind_of(5, false), Some("mouse"));
        assert_eq!(kind_of(12, false), Some("gamepad"));
        assert_eq!(kind_of(17, false), Some("headphones"));
        assert_eq!(kind_of(19, false), Some("headphones"));
        assert_eq!(kind_of(28, false), Some("device"), "something paired that UPower has no better word for");
        assert_eq!(kind_of(5, true), None, "anything that powers the computer");
    }

    #[test]
    fn a_line_has_a_name_a_level_and_whether_it_charges() {
        let d = describe("headset_dev_AA", 17, false, true, "WH-1000XM4", "Sony", 87.4, 2).unwrap();
        assert_eq!((d.name.as_str(), d.kind.as_str(), d.percent, d.charging), ("WH-1000XM4", "headphones", 87, false));
        let g = describe("gaming_input_x", 12, false, true, "", "Sony", 15.0, 1).unwrap();
        assert_eq!((g.name.as_str(), g.percent, g.charging), ("Sony", 15, true), "the vendor when there is no model");
        let n = describe("mouse_x", 5, false, true, "  ", "", 50.0, 2).unwrap();
        assert_eq!(n.name, "mouse", "the kind when it has no name at all");
    }

    #[test]
    fn a_device_without_a_real_level_has_no_line() {
        assert!(describe("m", 5, false, false, "Mouse", "", 50.0, 2).is_none(), "not present");
        assert!(describe("m", 5, false, true, "Mouse", "", -1.0, 2).is_none(), "no reading");
        assert!(describe("m", 5, false, true, "Mouse", "", 0.0, 4).is_none(), "0 and not discharging: unknown, not empty");
        assert!(describe("m", 5, false, true, "Mouse", "", 0.0, 2).is_some(), "0 while discharging is really empty");
    }

    /// Reads the REAL system service (read-only): `cargo test --lib real_peripherals_probe -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_peripherals_probe() {
        let conn = Connection::system().expect("system bus");
        println!("{:?}", read_all(&conn));
    }
}
