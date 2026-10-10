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
    /// Shared with Jinx: it is (or will be) one of her pendientes too.
    #[serde(default)]
    pub jinx: bool,
    /// Her id for it, once she has it. Its state and ours follow each other (see `reconcile`).
    #[serde(default)]
    pub jinx_id: Option<String>,
    /// Who made it when it was not made here: her source ("hook", "teams", "portal"…). The list shows
    /// a mark for these; they work like any other task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
}

/// One row of Jinx's pendientes, as far as the Hub needs it.
#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct JinxRow {
    pub id: String,
    pub titulo: String,
    pub due: Option<String>,
    pub categoria: Option<String>,
    pub source: Option<String>,
    pub status: String,
}

impl JinxRow {
    pub fn open(&self) -> bool {
        self.status == "pendiente"
    }
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
    list.push(Task { id: new_id(now), title, done: false, remind_at, notified: false, created: now, completed_at: None, jinx: false, jinx_id: None, origin: None });
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

/// Shares a task with Jinx, or stops sharing it (her copy is then dismissed by the next sync).
pub fn set_jinx(list: &mut [Task], id: &str, on: bool) -> Result<(), String> {
    find(list, id)?.jinx = on;
    Ok(())
}

// ── Following Jinx's pendientes ─────────────────────────────────────────────────

/// What a sync has to do on her side after the local list has been brought up to date.
#[derive(Debug, Default, PartialEq)]
pub struct Plan {
    /// Local tasks (ids) to hand to her: shared, not yet hers.
    pub to_add: Vec<String>,
    /// (her id, "hecho" | "descartado") to apply there.
    pub to_resolve: Vec<(String, &'static str)>,
    /// The local list changed (and must be saved).
    pub changed: bool,
}

/// Brings the local tasks in step with her list. She is the other half of a shared task:
/// finished there, it is finished here; finished here, it is closed there; no longer shared,
/// it is dismissed there. What cannot be done now (she is unreachable) is simply asked again
/// at the next sync, because the difference is still there.
pub fn reconcile(list: &mut [Task], jinx: &[JinxRow], now: i64) -> Plan {
    let mut plan = Plan::default();
    for t in list.iter_mut() {
        let Some(jid) = t.jinx_id.clone() else {
            if t.jinx && !t.done {
                plan.to_add.push(t.id.clone());
            }
            continue;
        };
        let Some(row) = jinx.iter().find(|r| r.id == jid) else {
            // Not in her list any more: the link is dead.
            t.jinx_id = None;
            plan.changed = true;
            continue;
        };
        if !t.jinx {
            if row.open() {
                plan.to_resolve.push((jid, "descartado"));
            }
            t.jinx_id = None;
            plan.changed = true;
        } else if !row.open() && !t.done {
            t.done = true;
            t.completed_at = Some(now);
            plan.changed = true;
        } else if row.open() && t.done {
            plan.to_resolve.push((jid, "hecho"));
        }
    }
    plan
}

/// Her open pendientes that are not one of our shared tasks (the ones to show beside ours).
pub fn unlinked_open(list: &[Task], jinx: &[JinxRow]) -> Vec<JinxRow> {
    jinx.iter().filter(|r| r.open() && !list.iter().any(|t| t.jinx_id.as_deref() == Some(r.id.as_str()))).cloned().collect()
}

/// The day number (from 1970-01-01) of "2026-10-06", the inverse of `civil_date`.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// When a pendiente with only a day gets its reminder: 09:00 that day, in this computer's time zone.
pub fn due_reminder(due: &str, offset: &dyn Fn(i64) -> i64) -> Option<i64> {
    let b = due.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return None;
    }
    let (y, m, d) = (due[0..4].parse::<i64>().ok()?, due[5..7].parse::<i64>().ok()?, due[8..10].parse::<i64>().ok()?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let nine = days_from_civil(y, m, d) * 86_400 + 9 * 3600;
    Some(nine - offset(nine))
}

/// Her open pendientes that are not any task of ours become tasks of ours — the same kind, with the same
/// actions — marked with who made them. One with a day gets a reminder at 09:00 that day, unless that time
/// has passed (it then shows as late, without ringing for something old). Returns how many were taken in.
pub fn import_unlinked(list: &mut Vec<Task>, jinx: &[JinxRow], now: i64, offset: &dyn Fn(i64) -> i64) -> usize {
    let mut taken = 0;
    for r in jinx.iter().filter(|r| r.open()) {
        if list.len() >= MAX_TASKS || list.iter().any(|t| t.jinx_id.as_deref() == Some(r.id.as_str())) {
            continue;
        }
        let title: String = r.titulo.trim().chars().take(MAX_TITLE).collect();
        if title.is_empty() {
            continue;
        }
        let remind_at = r.due.as_deref().and_then(|d| due_reminder(d, offset));
        list.push(Task {
            id: format!("j-{}", r.id),
            title,
            done: false,
            remind_at,
            // Only a reminder still to come may ring.
            notified: remind_at.is_some_and(|t| t <= now),
            created: now,
            completed_at: None,
            jinx: true,
            jinx_id: Some(r.id.clone()),
            origin: r.source.clone().filter(|s| s != "coucou"),
        });
        taken += 1;
    }
    taken
}

/// "2026-10-06" for a day number counted from 1970-01-01 (proleptic Gregorian).
pub fn civil_date(days: i64) -> String {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    format!("{:04}-{:02}-{:02}", if m <= 2 { y + 1 } else { y }, m, d)
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

/// A day picked on the calendar (epoch seconds of its local midnight) plus a time: the time
/// of day the sentence carried ("viernes a las 17:30" → Friday 17:30), or `default_hour`
/// when it carried none. Ported from TaskParser.withDay (Android): two passes so a
/// daylight-saving jump inside the day still lands on the right wall time.
pub fn with_day(day_start: i64, parsed_when: Option<i64>, default_hour: i64, offset: &dyn Fn(i64) -> i64) -> i64 {
    let at_min = parsed_when
        .map(|w| (w + offset(w)).rem_euclid(86_400) / 60)
        .unwrap_or(default_hour * 60);
    let t = day_start + at_min * 60 - offset(day_start + 12 * 3600);
    day_start + at_min * 60 - offset(t)
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

    fn row(id: &str, status: &str) -> JinxRow {
        JinxRow { id: id.into(), titulo: "x".into(), due: None, categoria: None, source: Some("coucou".into()), status: status.into() }
    }
    fn shared(title: &str, jinx_id: Option<&str>) -> Task {
        let mut l = Vec::new();
        add(&mut l, title, None, NOW).unwrap();
        let mut t = l.remove(0);
        t.jinx = true;
        t.jinx_id = jinx_id.map(String::from);
        t
    }

    #[test]
    fn a_shared_task_not_yet_hers_is_handed_over_while_it_is_open() {
        let mut l = vec![shared("a", None)];
        assert_eq!(reconcile(&mut l, &[], NOW).to_add, vec![l[0].id.clone()]);
        l[0].done = true;
        assert!(reconcile(&mut l, &[], NOW).to_add.is_empty(), "a finished one is not worth handing over");
        let mut private = vec![shared("b", None)];
        private[0].jinx = false;
        assert_eq!(reconcile(&mut private, &[], NOW), Plan::default());
    }

    #[test]
    fn finishing_follows_in_both_directions() {
        // She closed it: it is done here.
        let mut l = vec![shared("a", Some("j1"))];
        let p = reconcile(&mut l, &[row("j1", "hecho")], NOW);
        assert!(l[0].done && l[0].completed_at == Some(NOW) && p.changed && p.to_resolve.is_empty());
        // We closed it, she still has it open: asked again until it is there.
        let mut l = vec![shared("a", Some("j1"))];
        l[0].done = true;
        let p = reconcile(&mut l, &[row("j1", "pendiente")], NOW);
        assert_eq!(p.to_resolve, vec![("j1".to_string(), "hecho")]);
        assert!(!p.changed);
        // Both open, or both closed: nothing to do.
        let mut l = vec![shared("a", Some("j1"))];
        assert_eq!(reconcile(&mut l, &[row("j1", "pendiente")], NOW), Plan::default());
        let mut l = vec![shared("a", Some("j1"))];
        l[0].done = true;
        assert_eq!(reconcile(&mut l, &[row("j1", "hecho")], NOW), Plan::default());
    }

    #[test]
    fn unsharing_dismisses_her_copy_and_a_vanished_one_breaks_the_link() {
        let mut l = vec![shared("a", Some("j1"))];
        l[0].jinx = false;
        let p = reconcile(&mut l, &[row("j1", "pendiente")], NOW);
        assert_eq!(p.to_resolve, vec![("j1".to_string(), "descartado")]);
        assert_eq!(l[0].jinx_id, None);
        let mut l = vec![shared("a", Some("gone"))];
        assert!(reconcile(&mut l, &[row("j1", "pendiente")], NOW).changed);
        assert_eq!(l[0].jinx_id, None);
    }

    #[test]
    fn only_her_open_items_that_are_not_ours_are_listed() {
        let l = vec![shared("a", Some("j1"))];
        let rows = [row("j1", "pendiente"), row("j2", "pendiente"), row("j3", "hecho")];
        assert_eq!(unlinked_open(&l, &rows).iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["j2"]);
    }

    #[test]
    fn a_day_number_is_a_calendar_date() {
        assert_eq!(civil_date(0), "1970-01-01");
        assert_eq!(civil_date(100), "1970-04-11");
        assert_eq!(civil_date(11_016), "2000-02-29");
        assert_eq!(civil_date(20_732), "2026-10-06");
        assert_eq!(civil_date(-1), "1969-12-31");
    }

    #[test]
    fn a_day_becomes_nine_in_the_morning_in_this_zone() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 2, 29), 11_016);
        assert_eq!(days_from_civil(2026, 10, 6), 20_732);
        assert_eq!(days_from_civil(1969, 12, 31), -1);
        assert_eq!(due_reminder("2026-10-06", &|_| 0), Some(20_732 * DAY + 9 * 3600));
        assert_eq!(due_reminder("2026-10-06", &|_| -6 * 3600), Some(20_732 * DAY + 15 * 3600), "09:00 at UTC-6 is 15:00 UTC");
        for bad in ["mañana", "2026-13-01", "2026-10-00", "2026/10/06", "", "2026-10-061"] {
            assert_eq!(due_reminder(bad, &|_| 0), None, "{bad}");
        }
    }

    fn pend(id: &str, source: &str, due: Option<&str>, status: &str) -> JinxRow {
        JinxRow { id: id.into(), titulo: format!("pend {id}"), due: due.map(String::from), categoria: None, source: Some(source.into()), status: status.into() }
    }

    #[test]
    fn what_jinx_made_becomes_ordinary_tasks_with_a_mark() {
        let today = 20_732 * DAY + 10 * 3600; // 2026-10-06 10:00 UTC
        let mut l = Vec::new();
        let rows = [
            pend("a", "hook", Some("2026-10-08"), "pendiente"),   // a day ahead: rings at 09:00 that day
            pend("b", "teams", Some("2026-10-06"), "pendiente"),  // today, 09:00 has passed: late, silent
            pend("c", "portal", None, "pendiente"),               // no day: no reminder
            pend("d", "coucou", None, "pendiente"),               // made on another of my devices: no mark
            pend("e", "hook", None, "hecho"),                     // closed: not taken in
        ];
        assert_eq!(import_unlinked(&mut l, &rows, today, &|_| 0), 4);
        let by = |id: &str| l.iter().find(|t| t.jinx_id.as_deref() == Some(id)).unwrap();
        assert_eq!((by("a").remind_at, by("a").notified, by("a").origin.as_deref()), (Some(20_734 * DAY + 9 * 3600), false, Some("hook")));
        assert_eq!((by("b").remind_at, by("b").notified, by("b").origin.as_deref()), (Some(20_732 * DAY + 9 * 3600), true, Some("teams")));
        assert_eq!((by("c").remind_at, by("c").origin.as_deref()), (None, Some("portal")));
        assert_eq!(by("d").origin, None);
        assert!(l.iter().all(|t| t.jinx && !t.done), "shared with her, open");
        // They behave like any other: the next sync leaves them alone, and it takes nothing in twice.
        assert_eq!(import_unlinked(&mut l, &rows, today, &|_| 0), 0);
        assert_eq!(reconcile(&mut l, &rows, today), Plan::default());
        // She closes one: it is done here; we close one: it is closed there.
        let mut rows2 = rows.to_vec();
        rows2[0].status = "hecho".into();
        reconcile(&mut l, &rows2, today);
        assert!(by_id(&l, "a").done);
        let id = l.iter().find(|t| t.jinx_id.as_deref() == Some("c")).unwrap().id.clone();
        set_done(&mut l, &id, true, today).unwrap();
        assert_eq!(reconcile(&mut l, &rows2, today).to_resolve, vec![("c".to_string(), "hecho")]);
    }

    fn by_id<'a>(l: &'a [Task], jid: &str) -> &'a Task {
        l.iter().find(|t| t.jinx_id.as_deref() == Some(jid)).unwrap()
    }

    #[test]
    fn nothing_is_taken_in_beyond_the_limit() {
        let mut l = Vec::new();
        for _ in 0..MAX_TASKS {
            let _ = add(&mut l, "t", None, NOW);
        }
        assert_eq!(import_unlinked(&mut l, &[pend("z", "hook", None, "pendiente")], NOW, &|_| 0), 0);
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

    #[test]
    fn a_picked_day_wins_over_the_sentence_keeping_its_time() {
        let off = &|_| 0;
        // "viernes a las 17:30" on Friday → Friday 17:30.
        assert_eq!(with_day(104 * DAY, Some(100 * DAY + 17 * 3600 + 1800), 9, off), 104 * DAY + 17 * 3600 + 1800);
        // No time in the sentence → the default hour on the picked day.
        assert_eq!(with_day(104 * DAY, None, 9, off), 104 * DAY + 9 * 3600);
    }
}
