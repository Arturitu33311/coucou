// The Hub's tasks and reminders: a short list kept in Coucou's own folder (tasks.json, written
// through a temporary file), and the small parser that turns "llamar a mamá a las 17:30" or
// "stretch in 20 min" into a title and a time. The reminders themselves are timed by the page
// (one timeout, like the Pomodoro); this only remembers what is owed and what was announced.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::settings;

/// A to-do list, not a project manager.
pub const MAX_TASKS: usize = 500;
pub const MAX_TITLE: usize = 200;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub title: String,
    pub done: bool,
    /// Unix seconds the reminder is due (None: no reminder).
    #[serde(default)]
    pub remind_at: Option<i64>,
    /// The reminder was announced: it is not announced again after a restart.
    #[serde(default)]
    pub notified: bool,
    pub created: i64,
    #[serde(default)]
    pub completed_at: Option<i64>,
}

static LOCK: Mutex<()> = Mutex::new(());
static COUNTER: AtomicU64 = AtomicU64::new(0);

fn path() -> PathBuf {
    settings::local_dir().join("tasks.json")
}

pub fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn new_id(now: i64) -> String {
    format!("{:x}-{}", now, COUNTER.fetch_add(1, Ordering::Relaxed))
}

pub fn load() -> Vec<Task> {
    std::fs::read_to_string(path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save(list: &[Task]) -> Result<(), String> {
    let p = path();
    if let Some(dir) = p.parent() {
        crate::platform::ensure_private_dir(dir).map_err(|e| e.to_string())?;
    }
    save_to(&p, list)
}

fn save_to(p: &std::path::Path, list: &[Task]) -> Result<(), String> {
    let text = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, p).map_err(|e| e.to_string())
}

/// Reads the list, lets `f` change it, and saves it; one change at a time.
pub fn update(f: impl FnOnce(&mut Vec<Task>) -> Result<(), String>) -> Result<Vec<Task>, String> {
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut list = load();
    f(&mut list)?;
    save(&list)?;
    Ok(list)
}

// ── Changes (pure, so they can be tested) ─────────────────────────────────────────

pub fn add(list: &mut Vec<Task>, title: &str, remind_at: Option<i64>, now: i64) -> Result<(), String> {
    let title: String = title.trim().chars().take(MAX_TITLE).collect();
    if title.is_empty() {
        return Err("A task needs a title".into());
    }
    if list.len() >= MAX_TASKS {
        return Err("Too many tasks: finish or clear some first".into());
    }
    list.push(Task { id: new_id(now), title, done: false, remind_at, notified: false, created: now, completed_at: None });
    Ok(())
}

fn find<'a>(list: &'a mut [Task], id: &str) -> Result<&'a mut Task, String> {
    list.iter_mut().find(|t| t.id == id).ok_or_else(|| "That task is gone".to_string())
}

pub fn set_done(list: &mut [Task], id: &str, done: bool, now: i64) -> Result<(), String> {
    let t = find(list, id)?;
    t.done = done;
    t.completed_at = done.then_some(now);
    Ok(())
}

/// Moves the reminder to `until` and makes it announce again.
pub fn snooze(list: &mut [Task], id: &str, until: i64) -> Result<(), String> {
    let t = find(list, id)?;
    t.remind_at = Some(until);
    t.notified = false;
    t.done = false;
    t.completed_at = None;
    Ok(())
}

pub fn mark_notified(list: &mut [Task], id: &str) -> Result<(), String> {
    find(list, id)?.notified = true;
    Ok(())
}

pub fn delete(list: &mut Vec<Task>, id: &str) -> Result<(), String> {
    let before = list.len();
    list.retain(|t| t.id != id);
    if list.len() == before {
        return Err("That task is gone".into());
    }
    Ok(())
}

pub fn clear_done(list: &mut Vec<Task>) {
    list.retain(|t| !t.done);
}

// ── "when" in plain words ─────────────────────────────────────────────────────────

/// Splits "14" / "10min" into its number and the letters after it.
fn number_and_unit(tok: &str) -> Option<(i64, &str)> {
    let digits = tok.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits == 0 || digits > 4 {
        return None;
    }
    let (n, rest) = tok.split_at(digits);
    Some((n.parse().ok()?, rest))
}

fn unit_seconds(unit: &str) -> Option<i64> {
    match unit {
        "m" | "min" | "mins" | "minuto" | "minutos" | "minute" | "minutes" => Some(60),
        "h" | "hr" | "hrs" | "hora" | "horas" | "hour" | "hours" => Some(3600),
        "d" | "dia" | "día" | "dias" | "días" | "day" | "days" => Some(86_400),
        _ => None,
    }
}

/// "17:30", "9:05pm" → minutes since midnight.
fn clock(tok: &str, next: Option<&str>) -> Option<(i64, usize)> {
    let (hm, suffix) = match tok.find(|c: char| c.is_ascii_alphabetic()) {
        Some(i) => (&tok[..i], &tok[i..]),
        None => (tok, ""),
    };
    let (h, m) = match hm.split_once(':') {
        Some((h, m)) => (h.parse::<i64>().ok()?, m.parse::<i64>().ok()?),
        None => (hm.parse::<i64>().ok()?, 0),
    };
    // A suffix may also be the next word ("5 pm").
    let (suffix, used) = if suffix.is_empty() && matches!(next, Some("am") | Some("pm")) { (next.unwrap(), 2) } else { (suffix, 1) };
    let h = match suffix {
        "" => h,
        "am" if (1..=12).contains(&h) => h % 12,
        "pm" if (1..=12).contains(&h) => h % 12 + 12,
        _ => return None,
    };
    ((0..24).contains(&h) && (0..60).contains(&m)).then_some((h * 60 + m, used))
}

/// Takes the time out of `text`: "tomar agua en 20 min" → ("tomar agua", now + 20 min);
/// "reunión mañana a las 9" → ("reunión", tomorrow 09:00). A time of day that has passed
/// today means tomorrow. `offset` gives this computer's time-zone offset (seconds east of UTC).
pub fn parse_when(text: &str, now: i64, offset: &dyn Fn(i64) -> i64) -> (String, Option<i64>) {
    let lower = text.to_lowercase();
    let words: Vec<&str> = text.split_whitespace().collect();
    let low: Vec<&str> = lower.split_whitespace().collect();
    let mut used = vec![false; words.len()];
    let mut relative: Option<i64> = None;
    let mut day_add: Option<i64> = None;
    let mut minutes: Option<i64> = None;

    let mut i = 0;
    while i < low.len() {
        let w = low[i];
        let next = low.get(i + 1).copied();
        // "en 20 min", "in 2 hours", "en 20min"
        if matches!(w, "en" | "in") && relative.is_none() {
            if let Some((n, unit)) = next.and_then(number_and_unit) {
                let (unit, span) = if unit.is_empty() { (low.get(i + 2).copied().unwrap_or(""), 3) } else { (unit, 2) };
                if let Some(s) = unit_seconds(unit) {
                    relative = Some(n * s);
                    for u in used.iter_mut().skip(i).take(span) {
                        *u = true;
                    }
                    i += span;
                    continue;
                }
            }
        }
        if matches!(w, "mañana" | "tomorrow") && day_add.is_none() {
            day_add = Some(1);
            used[i] = true;
        } else if matches!(w, "hoy" | "today") && day_add.is_none() {
            day_add = Some(0);
            used[i] = true;
        } else if minutes.is_none() {
            // "a las 17:30", "a la 1", "at 5pm", or a bare "17:30"
            let (lead, at) = match (w, next) {
                ("a", Some("las")) | ("a", Some("la")) => (2, i + 2),
                ("at", _) | ("a", Some(_)) => (1, i + 1),
                _ => (0, i),
            };
            let candidate = low.get(at).copied().unwrap_or("");
            // "a las 9" is a time by itself; otherwise it needs a colon or am/pm ("9:00", "5pm"),
            // so that "tema 3" or "look at 5 things" stay words.
            let marked = candidate.contains(':')
                || candidate.chars().any(|c| c.is_ascii_alphabetic()) && candidate.starts_with(|c: char| c.is_ascii_digit())
                || (candidate.chars().all(|c| c.is_ascii_digit()) && matches!(low.get(at + 1).copied(), Some("am") | Some("pm")));
            if lead == 2 || marked {
                if let Some((m, n)) = clock(candidate, low.get(at + 1).copied()) {
                    minutes = Some(m);
                    for u in used.iter_mut().skip(i).take(at - i + n) {
                        *u = true;
                    }
                    i = at + n;
                    continue;
                }
            }
        }
        i += 1;
    }

    let when = if let Some(secs) = relative {
        Some(now + secs)
    } else if minutes.is_some() || day_add.is_some() {
        let off = offset(now);
        let today = (now + off).div_euclid(86_400);
        let at = minutes.unwrap_or(9 * 60);
        let ts = |day: i64| day * 86_400 + at * 60 - off;
        let mut t = ts(today + day_add.unwrap_or(0));
        // "a las 8" at 10:00 is tomorrow's 8, unless the day was said.
        if t <= now && day_add.is_none() {
            t = ts(today + 1);
        }
        Some(t)
    } else {
        None
    };

    let title = words.iter().zip(&used).filter(|(_, u)| !**u).map(|(w, _)| *w).collect::<Vec<_>>().join(" ");
    let title = if title.trim().is_empty() { text.trim().to_string() } else { title };
    (title, when)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;
    const NOW: i64 = 100 * DAY + 10 * 3600; // 10:00 on day 100, UTC

    fn p(s: &str) -> (String, Option<i64>) {
        parse_when(s, NOW, &|_| 0)
    }

    #[test]
    fn a_relative_time_is_taken_out_of_the_title() {
        assert_eq!(p("tomar agua en 20 min"), ("tomar agua".into(), Some(NOW + 1200)));
        assert_eq!(p("stretch in 2 hours"), ("stretch".into(), Some(NOW + 7200)));
        assert_eq!(p("llamar en 5min a mamá"), ("llamar a mamá".into(), Some(NOW + 300)));
        assert_eq!(p("revisar en 1 día"), ("revisar".into(), Some(NOW + DAY)));
    }

    #[test]
    fn a_time_of_day_is_today_if_ahead_and_tomorrow_if_passed() {
        assert_eq!(p("llamar a mamá a las 17:30"), ("llamar a mamá".into(), Some(100 * DAY + 17 * 3600 + 1800)));
        assert_eq!(p("standup 9:00"), ("standup".into(), Some(101 * DAY + 9 * 3600)), "09:00 has passed at 10:00");
        assert_eq!(p("call at 5pm"), ("call".into(), Some(100 * DAY + 17 * 3600)));
        assert_eq!(p("call at 5 pm"), ("call".into(), Some(100 * DAY + 17 * 3600)));
        assert_eq!(p("cena a las 21"), ("cena".into(), Some(100 * DAY + 21 * 3600)));
    }

    #[test]
    fn a_day_word_moves_the_day() {
        assert_eq!(p("reunión mañana a las 9"), ("reunión".into(), Some(101 * DAY + 9 * 3600)));
        assert_eq!(p("dentista tomorrow"), ("dentista".into(), Some(101 * DAY + 9 * 3600)), "no time: 09:00");
        assert_eq!(p("gym hoy 8:00"), ("gym".into(), Some(100 * DAY + 8 * 3600)), "today, even if it has passed");
    }

    #[test]
    fn ordinary_words_are_not_a_time() {
        assert_eq!(p("comprar pan"), ("comprar pan".into(), None));
        assert_eq!(p("ir a la tienda"), ("ir a la tienda".into(), None));
        assert_eq!(p("pagar en efectivo"), ("pagar en efectivo".into(), None));
        assert_eq!(p("estudiar tema 3"), ("estudiar tema 3".into(), None));
        assert_eq!(p("a las 25:00 algo"), ("a las 25:00 algo".into(), None), "not a clock");
        assert_eq!(p("en 20 min"), ("en 20 min".into(), Some(NOW + 1200)), "a title is never left empty");
    }

    #[test]
    fn the_time_zone_is_the_local_one() {
        // UTC+2: at 10:00 UTC it is 12:00 there; 17:30 local is 15:30 UTC.
        let (_, t) = parse_when("llamar a las 17:30", NOW, &|_| 2 * 3600);
        assert_eq!(t, Some(100 * DAY + 15 * 3600 + 1800));
    }

    #[test]
    fn tasks_are_added_completed_snoozed_and_deleted() {
        let mut l = Vec::new();
        assert!(add(&mut l, "   ", None, NOW).is_err());
        add(&mut l, "uno", Some(NOW + 60), NOW).unwrap();
        add(&mut l, "dos", None, NOW).unwrap();
        assert_ne!(l[0].id, l[1].id);
        let id = l[0].id.clone();
        mark_notified(&mut l, &id).unwrap();
        assert!(l[0].notified);
        set_done(&mut l, &id, true, NOW + 5).unwrap();
        assert_eq!(l[0].completed_at, Some(NOW + 5));
        snooze(&mut l, &id, NOW + 600).unwrap();
        assert!(!l[0].done && !l[0].notified, "snoozing reopens it and announces again");
        assert_eq!(l[0].remind_at, Some(NOW + 600));
        let second = l[1].id.clone();
        set_done(&mut l, &second, true, NOW).unwrap();
        clear_done(&mut l);
        assert_eq!(l.len(), 1);
        delete(&mut l, &id).unwrap();
        assert!(l.is_empty());
        assert!(delete(&mut l, &id).is_err());
    }

    #[test]
    fn the_list_is_bounded_and_survives_a_save() {
        let mut l = Vec::new();
        add(&mut l, &"x".repeat(MAX_TITLE + 50), None, NOW).unwrap();
        assert_eq!(l[0].title.chars().count(), MAX_TITLE);
        for _ in 0..MAX_TASKS {
            let _ = add(&mut l, "t", None, NOW);
        }
        assert_eq!(l.len(), MAX_TASKS);
        assert!(add(&mut l, "una más", None, NOW).is_err());

        let dir = std::env::temp_dir().join(format!("coucou-tasks-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("tasks.json");
        save_to(&file, &l[..2]).unwrap();
        let back: Vec<Task> = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(back, l[..2]);
        assert!(!dir.join("tasks.json.tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
