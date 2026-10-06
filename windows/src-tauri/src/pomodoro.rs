// Pomodoro sessions: a log of the focus and break periods that were run (one JSON line
// each, in Coucou's own folder), and the statistics the Hub shows from it. The timer
// itself lives in the page; this only remembers what happened.

use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::settings;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Entry {
    /// Unix seconds when the period ended.
    pub ts: i64,
    /// "focus" | "break"
    pub kind: String,
    pub seconds: u32,
    /// Ran to its end (not skipped or reset).
    pub completed: bool,
}

#[derive(Serialize, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub today_sessions: u32,
    pub today_minutes: u32,
    /// Focus minutes of the last 7 days, oldest first, today last.
    pub week_minutes: [u32; 7],
    /// Days in a row (counting back from today, or from yesterday if today has none) with a session.
    pub streak: u32,
    /// The hour of the day with most completed sessions over the last 30 days.
    pub peak_hour: Option<u32>,
    pub total_sessions: u32,
}

fn path() -> PathBuf {
    settings::local_dir().join("pomodoro.jsonl")
}

pub fn log(kind: &str, seconds: u32, completed: bool) -> Result<(), String> {
    if kind != "focus" && kind != "break" {
        return Err("unknown kind".into());
    }
    let entry = Entry { ts: now(), kind: kind.into(), seconds, completed };
    let line = serde_json::to_string(&entry).map_err(|e| e.to_string())?;
    let p = path();
    if let Some(dir) = p.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut f = std::fs::OpenOptions::new().create(true).append(true).open(&p).map_err(|e| e.to_string())?;
    writeln!(f, "{line}").map_err(|e| e.to_string())
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Seconds east of UTC at `ts` in this computer's time zone.
#[cfg(unix)]
pub(crate) fn local_offset(ts: i64) -> i64 {
    let t = ts as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&t, &mut tm) };
    tm.tm_gmtoff as i64
}

#[cfg(not(unix))]
pub(crate) fn local_offset(_ts: i64) -> i64 {
    0
}

pub fn read_entries() -> Vec<Entry> {
    std::fs::read_to_string(path())
        .unwrap_or_default()
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect()
}

pub fn stats() -> Stats {
    compute(&read_entries(), now(), &local_offset)
}

/// The statistics of `entries` as of `now`; `offset` gives the time-zone offset at a time.
pub fn compute(entries: &[Entry], now: i64, offset: &dyn Fn(i64) -> i64) -> Stats {
    let day = |ts: i64| (ts + offset(ts)).div_euclid(86_400);
    let today = day(now);
    let mut s = Stats::default();
    let mut days_with_session = std::collections::BTreeSet::new();
    let mut hours = [0u32; 24];
    for e in entries.iter().filter(|e| e.kind == "focus") {
        let d = day(e.ts);
        let age = today - d;
        if (0..7).contains(&age) {
            s.week_minutes[(6 - age) as usize] += e.seconds / 60;
        }
        if age == 0 {
            s.today_minutes += e.seconds / 60;
        }
        if e.completed {
            s.total_sessions += 1;
            days_with_session.insert(d);
            if age == 0 {
                s.today_sessions += 1;
            }
            if (0..30).contains(&age) {
                hours[((e.ts + offset(e.ts)).rem_euclid(86_400) / 3600) as usize] += 1;
            }
        }
    }
    // Back from today; a day with nothing yet today does not break a streak that ran to yesterday.
    let mut d = if days_with_session.contains(&today) { today } else { today - 1 };
    while days_with_session.contains(&d) {
        s.streak += 1;
        d -= 1;
    }
    let best = hours.iter().copied().max().unwrap_or(0);
    if best > 0 {
        s.peak_hour = hours.iter().position(|&n| n == best).map(|h| h as u32);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;
    fn focus(ts: i64, minutes: u32, completed: bool) -> Entry {
        Entry { ts, kind: "focus".into(), seconds: minutes * 60, completed }
    }

    #[test]
    fn today_the_week_and_the_streak_come_from_the_log() {
        let now = 100 * DAY + 10 * 3600; // 10:00, day 100 (UTC)
        let entries = vec![
            focus(now - 3600, 25, true),          // today 09:00
            focus(now - 7200, 25, true),          // today 08:00
            focus(now - 600, 10, false),          // today, skipped: minutes count, not a session
            focus(now - DAY, 25, true),           // yesterday
            focus(now - 2 * DAY, 50, true),       // two days ago
            focus(now - 4 * DAY, 25, true),       // a gap at three days ago
            Entry { ts: now - 3000, kind: "break".into(), seconds: 300, completed: true },
        ];
        let s = compute(&entries, now, &|_| 0);
        assert_eq!(s.today_sessions, 2);
        assert_eq!(s.today_minutes, 60);
        assert_eq!(s.week_minutes, [0, 0, 25, 0, 50, 25, 60]);
        assert_eq!(s.streak, 3, "today, yesterday and the day before; the gap ends it");
        assert_eq!(s.total_sessions, 5);
        assert_eq!(s.peak_hour, Some(10), "three sessions started at 10:00, more than any other hour");
    }

    #[test]
    fn a_streak_survives_a_day_that_has_not_had_its_session_yet() {
        let now = 50 * DAY + 3600;
        let s = compute(&[focus(now - DAY, 25, true), focus(now - 2 * DAY, 25, true)], now, &|_| 0);
        assert_eq!(s.streak, 2);
        assert_eq!(s.today_sessions, 0);
        assert_eq!(compute(&[], now, &|_| 0), Stats::default());
    }

    #[test]
    fn the_time_zone_moves_the_day_boundary() {
        // 23:30 UTC is already the next day at UTC+2.
        let now = 10 * DAY + 23 * 3600 + 1800;
        let s = compute(&[focus(now, 25, true), focus(now - 3600, 25, true)], now, &|_| 2 * 3600);
        assert_eq!(s.today_sessions, 2);
        assert_eq!(s.peak_hour, Some(0), "00:30 and 01:30 local tie: the earliest hour wins");
    }

    #[test]
    fn only_known_kinds_are_logged() {
        assert!(log("coffee", 1, true).is_err());
    }
}
