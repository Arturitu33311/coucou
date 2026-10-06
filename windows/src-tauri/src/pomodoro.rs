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
    /// Focus minutes of the last 30 days by hour of the day, by when each period began
    /// (a period that crosses an hour is shared between the two).
    pub hour_minutes: [u32; 24],
    /// The same minutes by weekday, Monday first.
    pub weekday_minutes: [u32; 7],
    /// Share (0–100) of the focus periods of the last 30 days that ran to their end.
    pub completion_pct: Option<u32>,
    /// Focus periods of the last 30 days (finished or not).
    pub periods_30d: u32,
    /// Where the best two hours in a row start ("sharpest 9–11"). Not given until there are
    /// MIN_PERIODS periods to go on: with three sessions it would be an accident, not a habit.
    pub best_window: Option<u32>,
}

/// Periods needed before the best window is called a habit.
pub const MIN_PERIODS: u32 = 10;

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

/// Adds `seconds` of focus that began at `start` to the hour buckets, minute by minute's worth:
/// a period from 09:45 for 50 minutes puts 15 minutes in the 9 o'clock and 35 in the 10.
fn spread(start: i64, seconds: u32, offset: &dyn Fn(i64) -> i64, hours: &mut [u32; 24]) {
    let mut at = start;
    let end = start + seconds as i64;
    while at < end {
        let local = at + offset(at);
        let into_hour = local.rem_euclid(3600);
        let take = (3600 - into_hour).min(end - at);
        hours[(local.rem_euclid(86_400) / 3600) as usize] += (take / 60) as u32;
        at += take;
    }
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
    let mut finished = 0u32;
    for e in entries.iter().filter(|e| e.kind == "focus") {
        let d = day(e.ts);
        let age = today - d;
        if (0..30).contains(&age) {
            s.periods_30d += 1;
            finished += e.completed as u32;
            let start = e.ts - e.seconds as i64;
            spread(start, e.seconds, offset, &mut s.hour_minutes);
            let started_day = (start + offset(start)).div_euclid(86_400);
            s.weekday_minutes[(started_day + 3).rem_euclid(7) as usize] += e.seconds / 60;
        }
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
    if s.periods_30d > 0 {
        s.completion_pct = Some((finished * 100 + s.periods_30d / 2) / s.periods_30d);
    }
    if s.periods_30d >= MIN_PERIODS {
        let pair = |h: usize| s.hour_minutes[h] + s.hour_minutes[h + 1];
        let top = (0..23).map(pair).max().unwrap_or(0);
        if top > 0 {
            s.best_window = (0..23).find(|&h| pair(h) == top).map(|h| h as u32);
        }
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
    fn a_period_is_shared_between_the_hours_it_crosses() {
        let mut h = [0u32; 24];
        // Began 09:45, 50 minutes.
        spread(9 * 3600 + 45 * 60, 50 * 60, &|_| 0, &mut h);
        assert_eq!((h[9], h[10]), (15, 35));
        assert_eq!(h.iter().sum::<u32>(), 50);
        // Across midnight at UTC+2: 22:30 UTC is 00:30 local.
        let mut h = [0u32; 24];
        spread(22 * 3600 + 1800, 60 * 60, &|_| 2 * 3600, &mut h);
        assert_eq!((h[0], h[1]), (30, 30));
    }

    #[test]
    fn the_habits_wait_for_enough_sessions_then_name_the_best_two_hours() {
        let now = 100 * DAY + 20 * 3600;
        // A 25-minute period that began at `hour`:00, `ago` days back (the log holds when it ended).
        let at = |ago: i64, hour: i64, completed: bool| focus(now - ago * DAY - 20 * 3600 + hour * 3600 + 25 * 60, 25, completed);
        // Nine periods: too few to call anything a habit, but the counts are there.
        let few: Vec<Entry> = (1..=9).map(|d| at(d, 9, true)).collect();
        let s = compute(&few, now, &|_| 0);
        assert_eq!(s.periods_30d, 9);
        assert_eq!(s.best_window, None);
        assert_eq!(s.completion_pct, Some(100));
        // Twelve: eight at 09:00 that ran to their end, four at 10:00 that did not.
        let mut many: Vec<Entry> = (1..=8).map(|d| at(d, 9, true)).collect();
        many.extend((1..=4).map(|d| at(d, 10, false)));
        let s = compute(&many, now, &|_| 0);
        assert_eq!(s.periods_30d, 12);
        assert_eq!(s.completion_pct, Some(67), "8 of 12 ran to their end");
        assert_eq!((s.hour_minutes[9], s.hour_minutes[10]), (200, 100));
        assert_eq!(s.best_window, Some(9), "09:00–11:00 holds 300 minutes, more than 08:00–10:00's 200");
        assert_eq!(s.weekday_minutes.iter().sum::<u32>(), 300, "every minute is on some weekday");
        // Day 100 is a Saturday: 1970-01-01 was a Thursday, and Monday is 0.
        let sat = compute(&[focus(100 * DAY + 3600, 25, true)], 100 * DAY + 7200, &|_| 0);
        assert_eq!(sat.weekday_minutes[5], 25);
    }

    #[test]
    fn only_known_kinds_are_logged() {
        assert!(log("coffee", 1, true).is_err());
    }
}
