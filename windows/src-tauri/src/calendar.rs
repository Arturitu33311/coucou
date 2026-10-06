// The Hub's Calendar: the next days' events from the calendar GNOME already keeps (Evolution
// Data Server through the Shell's calendar service, with your Google account from Online
// Accounts) — the same calendar Jinx reads — and Jinx's own list of pending tasks.
//
// The service has no "give me the events" call: it emits them as signals once a time range is
// set. Two threads, blocked on those signals and costing nothing while it is quiet, keep a
// cache; the Hub sets the range when its panel opens and reads the cache. GNOME expands the
// recurring events (your classes) for us.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde::Serialize;

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: String,
    pub summary: String,
    /// Unix seconds.
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub title: String,
    pub due: Option<String>,
    pub category: Option<String>,
    pub priority: i64,
}

static CACHE: Mutex<Option<HashMap<String, Event>>> = Mutex::new(None);
static STARTED: AtomicBool = AtomicBool::new(false);
static CONN: OnceLock<zbus::blocking::Connection> = OnceLock::new();

const DEST: &str = "org.gnome.Shell.CalendarServer";
const PATH: &str = "/org/gnome/Shell/CalendarServer";

/// Seconds east of UTC at `ts` here.
fn local_offset(ts: i64) -> i64 {
    let t = ts as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&t, &mut tm) };
    tm.tm_gmtoff as i64
}

/// A whole number of days starting at local midnight is an all-day event.
pub fn is_all_day(start: i64, end: i64, offset: i64) -> bool {
    end > start && (end - start) % 86_400 == 0 && (start + offset).rem_euclid(86_400) == 0
}

type Wire = Vec<(String, String, i64, i64, HashMap<String, zbus::zvariant::OwnedValue>)>;

fn absorb(events: Wire) {
    let mut guard = CACHE.lock().unwrap();
    let cache = guard.get_or_insert_with(HashMap::new);
    for (id, summary, start, end, _) in events {
        let all_day = is_all_day(start, end, local_offset(start));
        cache.insert(id.clone(), Event { id, summary, start, end, all_day });
    }
}

fn forget(ids: Vec<String>) {
    if let Some(cache) = CACHE.lock().unwrap().as_mut() {
        for id in ids {
            cache.remove(&id);
        }
    }
}

/// Connects to the session bus and starts listening (once).
fn ensure_started() -> Result<&'static zbus::blocking::Connection, String> {
    if let Some(c) = CONN.get() {
        return Ok(c);
    }
    let conn = zbus::blocking::Connection::session().map_err(|e| format!("No session bus: {e}"))?;
    if STARTED.swap(true, Ordering::SeqCst) {
        return CONN.get().ok_or_else(|| "The calendar is starting".to_string());
    }
    for member in ["EventsAddedOrUpdated", "EventsRemoved"] {
        let rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface("org.gnome.Shell.CalendarServer")
            .and_then(|b| b.member(member))
            .map(|b| b.build());
        let Ok(rule) = rule else { continue };
        let c = conn.clone();
        std::thread::spawn(move || {
            let Ok(iter) = zbus::blocking::MessageIterator::for_match_rule(rule, &c, Some(256)) else { return };
            for msg in iter.flatten() {
                match msg.header().member().map(|m| m.as_str().to_string()).as_deref() {
                    Some("EventsAddedOrUpdated") => {
                        if let Ok(events) = msg.body().deserialize::<Wire>() {
                            absorb(events);
                        }
                    }
                    Some("EventsRemoved") => {
                        if let Ok(ids) = msg.body().deserialize::<Vec<String>>() {
                            forget(ids);
                        }
                    }
                    _ => {}
                }
            }
        });
    }
    let _ = CONN.set(conn);
    CONN.get().ok_or_else(|| "The calendar did not start".to_string())
}

/// Asks GNOME for the events between two times (it then emits them).
fn request(conn: &zbus::blocking::Connection, since: i64, until: i64, reload: bool) -> Result<(), String> {
    let proxy = zbus::blocking::Proxy::new(conn, DEST, PATH, DEST).map_err(|e| e.to_string())?;
    proxy
        .call_method("SetTimeRange", &(since, until, reload))
        .map(|_| ())
        .map_err(|e| format!("GNOME's calendar is not available ({e})"))
}

/// The cached events that touch `[from, until)`, in order, without duplicates.
pub fn window(from: i64, until: i64) -> Vec<Event> {
    let guard = CACHE.lock().unwrap();
    let mut out: Vec<Event> = guard
        .as_ref()
        .map(|c| c.values().filter(|e| e.end > from && e.start < until).cloned().collect())
        .unwrap_or_default();
    out.sort_by(|a, b| (a.start, &a.summary).cmp(&(b.start, &b.summary)));
    out.dedup_by(|a, b| a.start == b.start && a.end == b.end && a.summary == b.summary);
    out
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The next `days` of events (from an hour ago, so one that is running shows). The first call
/// waits for the events to arrive; later ones read what the signals have kept up to date.
pub fn events(days: u32) -> Result<Vec<Event>, String> {
    // Debug builds only (compiled out of releases): the test harness has no GNOME, so it can
    // hand the Hub a list of events in a JSON file ([{summary,start,end,allDay}, ...]).
    #[cfg(debug_assertions)]
    if let Ok(path) = std::env::var("COUCOU_TEST_CALENDAR") {
        let raw = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let v: Vec<serde_json::Value> = serde_json::from_str(&raw).map_err(|e| e.to_string())?;
        return Ok(v
            .iter()
            .enumerate()
            .map(|(i, e)| Event {
                id: format!("t{i}"),
                summary: e["summary"].as_str().unwrap_or("").into(),
                start: e["start"].as_i64().unwrap_or(0),
                end: e["end"].as_i64().unwrap_or(0),
                all_day: e["allDay"].as_bool().unwrap_or(false),
            })
            .collect());
    }
    let first = CONN.get().is_none();
    let conn = ensure_started()?;
    let (from, until) = (now() - 3600, now() + i64::from(days) * 86_400);
    request(conn, from, until, first)?;
    // Signals come in bursts; take the burst as done once it has been quiet a moment.
    let deadline = Instant::now() + Duration::from_millis(if first { 4000 } else { 1200 });
    let mut seen = window(from, until).len();
    let mut quiet_since = Instant::now();
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
        let n = window(from, until).len();
        if n != seen {
            seen = n;
            quiet_since = Instant::now();
        } else if quiet_since.elapsed() > Duration::from_millis(if first { 900 } else { 300 }) && (seen > 0 || !first) {
            break;
        }
    }
    Ok(window(from, until))
}

// ── Jinx's pending tasks ──────────────────────────────────────────────────────

/// The open items of Jinx's `pendientes.json` (a file of hers, read here, never written).
pub fn parse_tasks(json: &str) -> Vec<Task> {
    let Ok(v) = serde_json::from_str::<serde_json::Value>(json) else { return Vec::new() };
    let items = v["items"].as_array().cloned().unwrap_or_default();
    let mut tasks: Vec<Task> = items
        .iter()
        .filter(|i| !matches!(i["status"].as_str(), Some("descartado" | "hecho" | "done" | "completado" | "cancelado")))
        .filter(|i| i["done_at"].is_null())
        .filter_map(|i| {
            Some(Task {
                title: i["titulo"].as_str()?.to_string(),
                due: i["due"].as_str().map(str::to_string),
                category: i["categoria"].as_str().map(str::to_string),
                priority: i["prioridad"].as_i64().unwrap_or(0),
            })
        })
        .collect();
    // Dated ones first (soonest), then by priority.
    tasks.sort_by(|a, b| match (&a.due, &b.due) {
        (Some(x), Some(y)) => x.cmp(y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => b.priority.cmp(&a.priority),
    });
    tasks
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reads the REAL calendar service (read-only): run by hand with
    /// `cargo test --lib real_calendar_probe -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_calendar_probe() {
        let list = events(7).expect("calendar");
        println!("events in the next 7 days: {}", list.len());
        for e in list.iter().take(6) {
            println!("  {} -> {} all_day={} {:.30}", e.start, e.end, e.all_day, e.summary);
        }
    }

    #[test]
    fn whole_days_from_local_midnight_are_all_day_events() {
        let utc_minus_6 = -6 * 3600;
        let midnight = 1_791_784_800; // 2026-10-12 00:00 at UTC-6
        assert!(is_all_day(midnight, midnight + 86_400, utc_minus_6));
        assert!(is_all_day(midnight, midnight + 3 * 86_400, utc_minus_6));
        assert!(!is_all_day(midnight + 3600, midnight + 3600 + 86_400, utc_minus_6), "a 24-hour event that starts at 01:00 is not");
        assert!(!is_all_day(midnight, midnight + 3600, utc_minus_6));
        assert!(!is_all_day(midnight, midnight, utc_minus_6));
    }

    #[test]
    fn the_window_is_ordered_deduplicated_and_only_what_touches_it() {
        let ev = |id: &str, s: &str, a: i64, b: i64| Event { id: id.into(), summary: s.into(), start: a, end: b, all_day: false };
        absorb_for_test(vec![
            ev("1", "Clase: Física I", 1000, 1100),
            ev("2", "Clase: Física I", 1000, 1100), // the same event from a second calendar
            ev("3", "Antes", 100, 200),
            ev("4", "Primero", 500, 600),
            ev("5", "Corriendo", 900, 5000),
        ]);
        let got = window(900, 2000);
        assert_eq!(got.iter().map(|e| e.summary.as_str()).collect::<Vec<_>>(), vec!["Corriendo", "Clase: Física I"]);
        CACHE.lock().unwrap().take();
    }

    fn absorb_for_test(events: Vec<Event>) {
        let mut g = CACHE.lock().unwrap();
        let c = g.get_or_insert_with(HashMap::new);
        for e in events {
            c.insert(e.id.clone(), e);
        }
    }

    #[test]
    fn jinxs_open_tasks_come_dated_first_and_closed_ones_stay_out() {
        let json = r#"{"items":[
          {"titulo":"sin fecha alta","due":null,"categoria":"personal","status":"pendiente","prioridad":2,"done_at":null},
          {"titulo":"entregar tarea","due":"2026-10-08","categoria":"escuela","status":"pendiente","prioridad":0,"done_at":null},
          {"titulo":"descartada","due":null,"status":"descartado","prioridad":0,"done_at":"2026-10-02T04:25:33Z"},
          {"titulo":"pagar luz","due":"2026-10-06","status":"pendiente","prioridad":1,"done_at":null},
          {"titulo":"sin fecha baja","due":null,"status":"pendiente","prioridad":0,"done_at":null}
        ],"updated_at":"x"}"#;
        let t = parse_tasks(json);
        assert_eq!(t.iter().map(|x| x.title.as_str()).collect::<Vec<_>>(), vec!["pagar luz", "entregar tarea", "sin fecha alta", "sin fecha baja"]);
        assert!(parse_tasks("not json").is_empty());
    }
}
