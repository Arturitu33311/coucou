// The small facts the island whispers about: the battery's level, whether it is plugged in or
// charging, and whether there is internet. They come from the system's own services (UPower
// and NetworkManager, on the system bus), which announce every change, so nothing is polled: the
// threads below sleep in a read until the system says something. A change is passed to the page
// only when what it shows would differ.
//
// Nothing here talks to the network or reads what is on it: NetworkManager's own verdict
// ("connectivity") is all there is.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter};
use zbus::blocking::{Connection, MessageIterator};

use crate::island::WINDOW_LABEL;
use crate::log;

static STARTED: AtomicBool = AtomicBool::new(false);
static CURRENT: Mutex<Option<Sys>> = Mutex::new(None);

const UPOWER: &str = "org.freedesktop.UPower";
const UPOWER_PATH: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
const UPOWER_DEVICE: &str = "org.freedesktop.UPower.Device";
const NM: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Sys {
    /// 0–100, or None on a computer without a battery.
    pub battery: Option<u8>,
    /// On mains power (charging, full, or waiting to charge).
    pub plugged: bool,
    /// Actually taking charge right now.
    pub charging: bool,
    pub online: bool,
}

impl Default for Sys {
    /// Until the system has answered: no claim that anything is wrong.
    fn default() -> Self {
        Sys { battery: None, plugged: false, charging: false, online: true }
    }
}

/// UPower's device state: 1 charging, 2 discharging, 3 empty, 4 fully charged, 5 pending charge,
/// 6 pending discharge. On mains power is anything that is not running from the battery.
pub fn plugged_from(state: u32) -> bool {
    matches!(state, 1 | 4 | 5)
}

pub fn charging_from(state: u32) -> bool {
    matches!(state, 1 | 5)
}

/// NetworkManager's connectivity: 0 unknown, 1 none, 2 captive portal, 3 limited, 4 full. When it
/// does not check (unknown), its overall state decides: 70 is "connected to the internet".
pub fn online_from(connectivity: u32, state: u32) -> bool {
    connectivity == 4 || (connectivity == 0 && state >= 70)
}

fn proxy<'a>(conn: &'a Connection, dest: &'a str, path: &'a str, iface: &'a str) -> Option<zbus::blocking::Proxy<'a>> {
    zbus::blocking::proxy::Builder::new(conn)
        .destination(dest)
        .ok()?
        .path(path)
        .ok()?
        .interface(iface)
        .ok()?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .ok()
}

fn read(conn: &Connection) -> Sys {
    let mut s = Sys::default();
    if let Some(up) = proxy(conn, UPOWER, UPOWER_PATH, UPOWER_DEVICE) {
        if up.get_property::<bool>("IsPresent").unwrap_or(false) {
            s.battery = up.get_property::<f64>("Percentage").ok().map(|p| p.round().clamp(0.0, 100.0) as u8);
            let state = up.get_property::<u32>("State").unwrap_or(0);
            s.plugged = plugged_from(state);
            s.charging = charging_from(state);
        }
    }
    if let Some(nm) = proxy(conn, NM, NM_PATH, NM) {
        let connectivity = nm.get_property::<u32>("Connectivity");
        let state = nm.get_property::<u32>("State");
        // No NetworkManager at all: nothing to say, so no claim of being offline.
        if let (Ok(c), Ok(st)) = (connectivity, state) {
            s.online = online_from(c, st);
        }
    }
    s
}

/// What is known now (the page asks once at start; after that it is told).
pub fn current() -> Sys {
    CURRENT.lock().unwrap().clone().unwrap_or_default()
}

fn refresh(app: &AppHandle, conn: &Connection) {
    let now = read(conn);
    let mut cur = CURRENT.lock().unwrap();
    if cur.as_ref() != Some(&now) {
        *cur = Some(now.clone());
        drop(cur);
        let _ = app.emit_to(WINDOW_LABEL, "sysstate", now);
    }
}

/// Starts listening (once). Without a system bus, or without these services, it stays silent.
pub fn start(app: &AppHandle) {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    let conn = match Connection::system() {
        Ok(c) => c,
        Err(e) => {
            log::line(format!("battery and network details: no system bus ({e})"));
            return;
        }
    };
    *CURRENT.lock().unwrap() = Some(read(&conn));
    // (interface, member, path) of what announces a change.
    let watched = [
        ("org.freedesktop.DBus.Properties", "PropertiesChanged", UPOWER_PATH),
        ("org.freedesktop.DBus.Properties", "PropertiesChanged", NM_PATH),
        (NM, "StateChanged", NM_PATH),
    ];
    for (iface, member, path) in watched {
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface(iface)
            .and_then(|b| b.member(member))
            .and_then(|b| b.path(path))
            .map(|b| b.build());
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
    fn mains_power_is_anything_but_running_on_the_battery() {
        assert!(plugged_from(1) && plugged_from(4) && plugged_from(5));
        assert!(!plugged_from(2) && !plugged_from(3) && !plugged_from(6) && !plugged_from(0));
        assert!(charging_from(1) && charging_from(5));
        assert!(!charging_from(4), "full is plugged in but not charging");
        assert!(!charging_from(2));
    }

    #[test]
    fn online_follows_the_connectivity_check_and_falls_back_to_the_state() {
        assert!(online_from(4, 70));
        assert!(!online_from(1, 70), "connected to a network that reaches nowhere");
        assert!(!online_from(2, 60), "a captive portal is not the internet yet");
        assert!(!online_from(3, 60));
        assert!(online_from(0, 70), "no check configured: the state decides");
        assert!(!online_from(0, 20));
    }

    #[test]
    fn until_the_system_answers_nothing_is_claimed() {
        let s = Sys::default();
        assert!(s.online && s.battery.is_none() && !s.plugged);
    }

    /// Reads the REAL system services (read-only): `cargo test --lib real_sysstate_probe -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_sysstate_probe() {
        let conn = Connection::system().expect("system bus");
        println!("{:?}", read(&conn));
    }
}
