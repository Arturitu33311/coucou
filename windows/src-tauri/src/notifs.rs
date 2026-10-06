// Notification peek: when another program sends the desktop a notification, the island can show
// it. It reads private content (messages, mail), so it is off until the user turns it on in
// Settings, and what it sees is only forwarded to the island's page — never stored, never logged,
// never sent anywhere.
//
// The desktop's own notification service is untouched: the island watches the calls to it by
// becoming a D-Bus *monitor* (the way `dbus-monitor` does), which sees a copy of each `Notify`
// call. It cannot hide the desktop's own banner; that is the desktop's setting.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::island::WINDOW_LABEL;
use crate::log;

static ENABLED: AtomicBool = AtomicBool::new(false);
static STARTED: AtomicBool = AtomicBool::new(false);

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Peek {
    pub app: String,
    pub summary: String,
    pub body: String,
}

/// Notification bodies may carry a little markup (`<b>`, `<a href>`): the words only.
pub fn strip_markup(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'")
}

/// What to show for a `Notify` call, if anything: not Coucou's own, not empty, kept short.
pub fn peek_from(app: &str, summary: &str, body: &str) -> Option<Peek> {
    if app.eq_ignore_ascii_case("coucou") {
        return None;
    }
    let summary: String = strip_markup(summary).trim().chars().take(80).collect();
    let body: String = strip_markup(body).split_whitespace().collect::<Vec<_>>().join(" ").chars().take(160).collect();
    if summary.is_empty() && body.is_empty() {
        return None;
    }
    Some(Peek { app: app.trim().chars().take(40).collect(), summary, body })
}

/// Turns the peek on or off; the watcher starts the first time it is turned on.
pub fn sync_enabled(app: &AppHandle, on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
    if on && !STARTED.swap(true, Ordering::SeqCst) {
        let app = app.clone();
        std::thread::spawn(move || watch(app));
    }
}

fn watch(app: AppHandle) {
    use zbus::blocking::{Connection, MessageIterator};
    use zbus::zvariant::OwnedValue;

    let conn = match Connection::session() {
        Ok(c) => c,
        Err(e) => {
            log::line(format!("notification peek: no session bus ({e})"));
            STARTED.store(false, Ordering::SeqCst);
            return;
        }
    };
    let rules = vec!["type='method_call',interface='org.freedesktop.Notifications',member='Notify'"];
    let monitoring = conn.call_method(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        Some("org.freedesktop.DBus.Monitoring"),
        "BecomeMonitor",
        &(rules, 0u32),
    );
    if let Err(e) = monitoring {
        log::line(format!("notification peek: the bus refused to let it watch ({e})"));
        STARTED.store(false, Ordering::SeqCst);
        return;
    }
    log::line("notification peek: watching".to_string());
    type Notify = (String, u32, String, String, String, Vec<String>, HashMap<String, OwnedValue>, i32);
    for msg in MessageIterator::from(conn).flatten() {
        if !ENABLED.load(Ordering::Relaxed) {
            continue;
        }
        let is_notify = msg.header().member().is_some_and(|m| m.as_str() == "Notify");
        if !is_notify {
            continue;
        }
        if let Ok((app_name, _, _, summary, body, ..)) = msg.body().deserialize::<Notify>() {
            if let Some(peek) = peek_from(&app_name, &summary, &body) {
                let _ = app.emit_to(WINDOW_LABEL, "notification", peek);
            }
        }
    }
    STARTED.store(false, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_is_reduced_to_words() {
        assert_eq!(strip_markup("<b>Mamá</b>: ¿vienes? &amp; trae <a href=\"x\">pan</a>"), "Mamá: ¿vienes? & trae pan");
        assert_eq!(strip_markup("sin marcas"), "sin marcas");
    }

    #[test]
    fn a_peek_is_short_never_coucous_own_and_never_empty() {
        let p = peek_from("Signal", "Ana", "hola\n\n  ¿qué   tal?").unwrap();
        assert_eq!((p.app.as_str(), p.summary.as_str(), p.body.as_str()), ("Signal", "Ana", "hola ¿qué tal?"));
        assert!(peek_from("Coucou", "x", "y").is_none());
        assert!(peek_from("coucou", "x", "y").is_none());
        assert!(peek_from("App", "  ", "<b></b>").is_none());
        let long = peek_from("App", &"s".repeat(300), &"b".repeat(500)).unwrap();
        assert_eq!((long.summary.len(), long.body.len()), (80, 160));
    }
}
