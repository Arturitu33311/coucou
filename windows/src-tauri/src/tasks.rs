// The Hub's tasks and reminders: a short list kept in Coucou's own folder (tasks.json, written
// through a temporary file), and the small parser that turns "llamar a mamá a las 17:30" or
// "stretch in 20 min" into a title and a time. The reminders themselves are timed by the page
// (one timeout, like the Pomodoro); this only remembers what is owed and what was announced.
//
// Each device works alone and the newest change wins when they meet (`merge`), the same rules as
// the Android TaskLogic.kt; windows/scripts/phone/merge-vectors.json holds the cases both must pass.
// A deletion is a tombstone (`deleted`) that travels to the other device and is forgotten later.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::settings;

/// A to-do list, not a project manager.
pub const MAX_TASKS: usize = 500;
pub const MAX_TITLE: usize = 200;
/// How long a deletion is remembered, so a device that was away for a while still learns of it.
pub const TOMBSTONE_SECS: i64 = 30 * 86_400;
/// A change stamped further ahead than this (a clock that is wrong) is taken as made now.
pub const SKEW_SECS: i64 = 300;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub done: bool,
    /// Unix seconds the reminder is due (None: no reminder).
    #[serde(default)]
    pub remind_at: Option<i64>,
    /// The reminder was announced: it is not announced again after a restart.
    #[serde(default)]
    pub notified: bool,
    #[serde(default)]
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
    /// When someone last changed it (unix seconds). 0: never changed by a person (taken in from Jinx).
    #[serde(default)]
    pub updated_at: i64,
    /// Deleted: kept as a tombstone so the deletion travels to the other device (the screens never see it).
    #[serde(default)]
    pub deleted: bool,
    /// Bookkeeping of the Android three-way sync with Jinx; carried, not used here.
    #[serde(default)]
    pub jinx_status: Option<String>,
    #[serde(default)]
    pub jinx_synced: i64,
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
    /// When her store last changed the row (unix seconds), if her gateway says; without it a conflict
    /// is not told by time.
    pub updated_at: Option<i64>,
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

/// A file from before the stamps: what was made here counts from when it was made or finished; what was
/// taken in from Jinx ("j-…") stays at 0, so any real change beats it. Same as Android's decode.
fn legacy_stamp(t: &mut Task) {
    if t.updated_at == 0 && !t.id.starts_with("j-") {
        t.updated_at = t.completed_at.unwrap_or(t.created).max(t.created);
    }
}

/// Everything on disk, tombstones included (what the sync and the merge work on).
pub fn load_all() -> Vec<Task> {
    let mut list: Vec<Task> = std::fs::read_to_string(path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    list.iter_mut().for_each(legacy_stamp);
    list
}

/// What the screens see: the list without the tombstones.
pub fn visible(list: &[Task]) -> Vec<Task> {
    list.iter().filter(|t| !t.deleted).cloned().collect()
}

/// Forgets deletions older than `TOMBSTONE_SECS`. One that still has her pendiente linked is kept until the
/// link is gone (see `reconcile`), so her copy is dismissed before the deletion is forgotten.
pub fn purge_tombstones(list: &mut Vec<Task>, now: i64) {
    list.retain(|t| !(t.deleted && t.jinx_id.is_none() && t.updated_at < now - TOMBSTONE_SECS));
}

/// What the screens see of the stored list.
pub fn load() -> Vec<Task> {
    visible(&load_all())
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

/// Reads the whole list (tombstones included), lets `f` change it, and saves it; one change at a time.
/// Returns what the screens see of it.
pub fn update(f: impl FnOnce(&mut Vec<Task>) -> Result<(), String>) -> Result<Vec<Task>, String> {
    run_update(f, true)
}

/// `notify`: tell a phone on the link at once (a change made here); not for what it just told us.
fn run_update(f: impl FnOnce(&mut Vec<Task>) -> Result<(), String>, notify: bool) -> Result<Vec<Task>, String> {
    let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let mut list = load_all();
    f(&mut list)?;
    purge_tombstones(&mut list, now());
    save(&list)?;
    drop(guard);
    if notify {
        crate::remote::tasks_changed();
    }
    Ok(visible(&list))
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
    list.push(Task {
        id: new_id(now),
        title,
        done: false,
        remind_at,
        notified: false,
        created: now,
        completed_at: None,
        jinx: false,
        jinx_id: None,
        origin: None,
        updated_at: now,
        deleted: false,
        jinx_status: None,
        jinx_synced: 0,
    });
    Ok(())
}

/// A task that is there for the person: a tombstone is gone.
fn find<'a>(list: &'a mut [Task], id: &str) -> Result<&'a mut Task, String> {
    list.iter_mut().find(|t| t.id == id && !t.deleted).ok_or_else(|| "That task is gone".to_string())
}

pub fn set_done(list: &mut [Task], id: &str, done: bool, now: i64) -> Result<(), String> {
    let t = find(list, id)?;
    t.done = done;
    t.completed_at = done.then_some(now);
    t.updated_at = now;
    Ok(())
}

/// Moves the reminder to `until` and makes it announce again.
pub fn snooze(list: &mut [Task], id: &str, until: i64) -> Result<(), String> {
    snooze_at(list, id, until, now())
}

pub fn snooze_at(list: &mut [Task], id: &str, until: i64, now: i64) -> Result<(), String> {
    let t = find(list, id)?;
    t.remind_at = Some(until);
    t.notified = false;
    t.done = false;
    t.completed_at = None;
    t.updated_at = now;
    Ok(())
}

pub fn mark_notified(list: &mut [Task], id: &str) -> Result<(), String> {
    find(list, id)?.notified = true;
    Ok(())
}

/// A deletion is a change like any other: stamped, and kept as a tombstone so the other device learns of it.
pub fn delete(list: &mut [Task], id: &str) -> Result<(), String> {
    delete_at(list, id, now())
}

pub fn delete_at(list: &mut [Task], id: &str, now: i64) -> Result<(), String> {
    let t = find(list, id)?;
    t.deleted = true;
    t.updated_at = now;
    Ok(())
}

/// Shares a task with Jinx, or stops sharing it (her copy is then dismissed by the next sync).
pub fn set_jinx(list: &mut [Task], id: &str, on: bool) -> Result<(), String> {
    set_jinx_at(list, id, on, now())
}

pub fn set_jinx_at(list: &mut [Task], id: &str, on: bool, now: i64) -> Result<(), String> {
    let t = find(list, id)?;
    t.jinx = on;
    t.updated_at = now;
    Ok(())
}

// ── Following Jinx's pendientes ─────────────────────────────────────────────────

/// What a sync has to do on her side after the local list has been brought up to date.
#[derive(Debug, Default, PartialEq)]
pub struct Plan {
    /// Local tasks (ids) to hand to her: shared, not yet hers.
    pub to_add: Vec<String>,
    /// (her id, "hecho" | "descartado" | "reabrir") to apply there.
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
            if t.jinx && !t.done && !t.deleted {
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
        if t.deleted {
            // A deletion dismisses her copy (asked again until it is done); once she has none open the
            // link is let go, so the tombstone can be forgotten.
            if row.open() {
                plan.to_resolve.push((jid, "descartado"));
            } else {
                t.jinx_id = None;
                plan.changed = true;
            }
        } else if !t.jinx {
            if row.open() {
                plan.to_resolve.push((jid, "descartado"));
            }
            t.jinx_id = None;
            plan.changed = true;
        } else if row.open() == !t.done {
            // Agreement: remember it, so the next change on either side is told from this one.
            if t.jinx_status.as_deref() != Some(row.status.as_str()) || t.jinx_synced != t.updated_at {
                t.jinx_status = Some(row.status.clone());
                t.jinx_synced = t.updated_at;
                plan.changed = true;
            }
        } else {
            // They differ. Each side keeps working alone, so both may have changed: what each change
            // is told from is the status she had when we last agreed and the stamp then on both sides.
            // Only one changed: it is copied to the other. Both: the later stamp wins; her row has a
            // stamp only when her gateway reports one, and without it she wins. (Same rules as the
            // phone's TaskLogic.reconcile.)
            let known = t.jinx_status.is_some();
            let she_changed = !known || t.jinx_status.as_deref() != Some(row.status.as_str());
            let we_changed = !known || t.updated_at > t.jinx_synced;
            let take_hers = if !known {
                // An old link with no memory: she is followed when she closed it, and told when we did.
                !row.open()
            } else if she_changed && !we_changed {
                true
            } else if !she_changed {
                false
            } else {
                row.updated_at.map(|u| u > t.updated_at).unwrap_or(true)
            };
            if take_hers {
                // Her change is a change made now: the other device must take it over its older state.
                let stamp = row.updated_at.unwrap_or_else(|| now.max(t.updated_at));
                if row.open() {
                    t.done = false;
                    t.completed_at = None;
                } else {
                    t.done = true;
                    t.completed_at = Some(now);
                }
                t.updated_at = stamp;
                t.jinx_status = Some(row.status.clone());
                t.jinx_synced = stamp;
                plan.changed = true;
            } else {
                plan.to_resolve.push((jid, if t.done { "hecho" } else { "reabrir" }));
            }
        }
    }
    plan
}

/// "2026-10-06T19:30:42.27+00:00" (or with Z, or without a zone: UTC) to unix seconds; None if it is
/// not a date. How the gateway's `updated_at` is read (the phone's `TaskLogic.isoToUnix`).
pub fn iso_to_unix(s: &str) -> Option<i64> {
    let b = s.trim().as_bytes();
    let num = |from: usize, len: usize| -> Option<i64> {
        let part = b.get(from..from + len)?;
        part.iter().all(u8::is_ascii_digit).then(|| std::str::from_utf8(part).ok()?.parse().ok())?
    };
    if b.len() < 19 || b[4] != b'-' || b[7] != b'-' || !(b[10] == b'T' || b[10] == b' ') || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let (y, mo, d, h, mi, se) = (num(0, 4)?, num(5, 2)?, num(8, 2)?, num(11, 2)?, num(14, 2)?, num(17, 2)?);
    let mut t = days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + se;
    let mut i = 19;
    if b.get(i) == Some(&b'.') {
        i += 1;
        let digits = b[i..].iter().take_while(|c| c.is_ascii_digit()).count();
        if digits == 0 {
            return None;
        }
        i += digits;
    }
    match b.get(i) {
        None => {}
        Some(b'Z') if i + 1 == b.len() => {}
        Some(&sign @ (b'+' | b'-')) => {
            let rest: Vec<u8> = b[i + 1..].iter().copied().filter(|c| *c != b':').collect();
            if rest.len() != 4 || !rest.iter().all(u8::is_ascii_digit) {
                return None;
            }
            let hh: i64 = std::str::from_utf8(&rest[..2]).ok()?.parse().ok()?;
            let mm: i64 = std::str::from_utf8(&rest[2..]).ok()?.parse().ok()?;
            let off = hh * 3600 + mm * 60;
            t -= if sign == b'+' { off } else { -off };
        }
        Some(_) => return None,
    }
    Some(t)
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
            // Not a person's change: what a person does with it later beats this.
            updated_at: 0,
            deleted: false,
            jinx_status: Some(r.status.clone()),
            jinx_synced: 0,
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

pub fn clear_done(list: &mut [Task]) {
    clear_done_at(list, now());
}

pub fn clear_done_at(list: &mut [Task], now: i64) {
    for t in list.iter_mut().filter(|t| t.done && !t.deleted) {
        t.deleted = true;
        t.updated_at = now;
    }
}

// ── Meeting the other device ────────────────────────────────────────────────────

/// The newest of two versions: the later change; on a tie a fixed order, so both devices choose the same.
fn newer(a: &Task, b: &Task) -> bool {
    if a.updated_at != b.updated_at {
        return a.updated_at > b.updated_at;
    }
    if a.deleted != b.deleted {
        return a.deleted;
    }
    if a.done != b.done {
        return a.done;
    }
    if a.title != b.title {
        // Byte order of the UTF-8, as on the phone.
        return a.title.as_bytes() > b.title.as_bytes();
    }
    // None is the smallest: a reminder beats none, a later one beats an earlier one.
    a.remind_at > b.remind_at
}

/// The same id, or the same pendiente of Jinx's: one made here and taken in from her elsewhere is one task.
fn same_task(local: &Task, remote: &Task) -> bool {
    local.id == remote.id || (local.jinx_id.is_some() && local.jinx_id == remote.jinx_id)
}

/// Merges another device's list into ours: per task the newest change wins, a deletion is a change, and what
/// the other has and we do not is added. Meeting twice, or in either order, gives the same list.
/// Returns the new list and whether it changed.
pub fn merge(local: &[Task], remote: &[Task], now: i64) -> (Vec<Task>, bool) {
    let mut out: Vec<Task> = local.to_vec();
    let mut changed = false;
    for r0 in remote {
        let mut r = r0.clone();
        // A clock that is ahead must not win for ever.
        r.updated_at = r.updated_at.min(now + SKEW_SECS);
        let Some(i) = out.iter().position(|l| same_task(l, &r)) else {
            if out.len() < MAX_TASKS * 2 && !(r.deleted && r.updated_at < now - TOMBSTONE_SECS) {
                // A reminder still to come rings here too; one already past does not ring late.
                r.notified = r.remind_at.is_some_and(|t| t <= now);
                out.push(r);
                changed = true;
            }
            continue;
        };
        if newer(&r, &out[i]) {
            let l = &out[i];
            r.notified = if l.remind_at == r.remind_at { l.notified || r.notified } else { r.notified };
            r.id = l.id.clone();
            r.jinx_id = l.jinx_id.clone().or(r.jinx_id);
            r.jinx = l.jinx || r.jinx;
            r.origin = l.origin.clone().or(r.origin);
            r.jinx_status = l.jinx_status.clone();
            r.jinx_synced = l.jinx_synced;
            out[i] = r;
            changed = true;
        } else if out[i].jinx_id.is_none() && r.jinx_id.is_some() {
            out[i].jinx_id = r.jinx_id;
            changed = true;
        }
    }
    (out, changed)
}

/// A task from another device is data, not trust: its text and its links are bounded before it is merged.
/// None: not worth keeping.
pub fn sanitize_remote(t: &Task) -> Option<Task> {
    // Lengths count UTF-16 units, as Kotlin's do.
    if t.id.is_empty() || t.id.encode_utf16().count() > 64 || t.id.chars().any(|c| c.is_control()) {
        return None;
    }
    let title: String = t.title.chars().filter(|c| !c.is_control()).take(MAX_TITLE).collect();
    let title = title.trim().to_string();
    if title.is_empty() && !t.deleted {
        return None;
    }
    let hex_ok = t.jinx_id.as_deref().map_or(true, |j| !j.is_empty() && j.len() <= 16 && j.bytes().all(|b| b.is_ascii_hexdigit()));
    let origin = t.origin.clone().filter(|o| o.encode_utf16().count() <= 24 && o.chars().all(|c| c.is_alphanumeric() || c == '-'));
    let mut s = t.clone();
    s.title = title;
    if !hex_ok {
        s.jinx_id = None;
    }
    s.origin = origin;
    // Bookkeeping about the other device's own link with her is not ours to take.
    s.jinx_status = None;
    s.jinx_synced = 0;
    Some(s)
}

/// The whole list, tombstones included, in the shape the Android app writes (every key present).
pub fn snapshot_json() -> String {
    encode(&load_all())
}

fn encode(list: &[Task]) -> String {
    let arr: Vec<serde_json::Value> = list
        .iter()
        .map(|t| {
            let mut v = serde_json::to_value(t).unwrap_or(serde_json::Value::Null);
            if let Some(o) = v.as_object_mut() {
                o.entry("origin").or_insert(serde_json::Value::Null);
            }
            v
        })
        .collect();
    serde_json::to_string(&arr).unwrap_or_else(|_| "[]".into())
}

/// Reads a list in the Android shape. An element that is not a task is skipped (it would not survive
/// `sanitize_remote` either); a file from before the stamps gets its stamp as in `load_all`.
fn decode(json: &str) -> Result<Vec<Task>, String> {
    let values: Vec<serde_json::Value> = serde_json::from_str(json).map_err(|e| e.to_string())?;
    Ok(values
        .into_iter()
        .filter_map(|v| serde_json::from_value::<Task>(v).ok())
        .map(|mut t| {
            legacy_stamp(&mut t);
            t
        })
        .collect())
}

/// Takes in the other device's list (the Android JSON shape): each task is bounded, then merged into the
/// stored list under the same lock as every other change. Returns whether anything changed.
pub fn merge_remote_json(json: &str) -> Result<bool, String> {
    let remote: Vec<Task> = decode(json)?.iter().filter_map(sanitize_remote).collect();
    let mut changed = false;
    run_update(
        |l| {
            let (merged, ch) = merge(l, &remote, now());
            if ch {
                *l = merged;
            }
            changed = ch;
            Ok(())
        },
        false,
    )?;
    Ok(changed)
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

    /// The phone's tests read the same fixture: both sides write a task with the same keys and read the other's.
    #[test]
    fn a_task_on_the_wire_has_the_keys_the_phone_writes_and_reads() {
        let doc: serde_json::Value = serde_json::from_str(include_str!("../../scripts/phone/wire-fixtures.json")).unwrap();
        let keys = |v: &serde_json::Value| {
            let mut k: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
            k.sort();
            k
        };
        let theirs = &doc["phone_to_laptop"]["tasks_sync"]["tasks"][0];
        let mut mine = Vec::new();
        add(&mut mine, "leche", None, 1).unwrap();
        let encoded: serde_json::Value = serde_json::from_str(&encode(&mine)).unwrap();
        assert_eq!(keys(&encoded[0]), keys(theirs));
        // And theirs, as the phone writes it, is a task here.
        let read = decode(&serde_json::json!([theirs]).to_string()).unwrap();
        assert_eq!((read[0].id.as_str(), read[0].title.as_str(), read[0].updated_at), ("t-1", "leche", 5));
        let back = decode(&serde_json::json!([doc["laptop_to_phone"]["tasks"]["tasks"][0]]).to_string()).unwrap();
        assert!(back[0].done && back[0].updated_at == 9);
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
        assert_eq!(visible(&l).len(), 1);
        delete(&mut l, &id).unwrap();
        assert!(visible(&l).is_empty());
        assert!(delete(&mut l, &id).is_err());
    }

    fn row(id: &str, status: &str) -> JinxRow {
        JinxRow { id: id.into(), titulo: "x".into(), due: None, categoria: None, source: Some("coucou".into()), status: status.into(), updated_at: None }
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
        // Both open, or both closed: nothing to do on her side; the first meeting is remembered, the
        // next one has nothing to say.
        let mut l = vec![shared("a", Some("j1"))];
        let first = reconcile(&mut l, &[row("j1", "pendiente")], NOW);
        assert!(first.to_add.is_empty() && first.to_resolve.is_empty());
        assert_eq!(l[0].jinx_status.as_deref(), Some("pendiente"));
        assert_eq!(reconcile(&mut l, &[row("j1", "pendiente")], NOW), Plan::default());
        let mut l = vec![shared("a", Some("j1"))];
        l[0].done = true;
        reconcile(&mut l, &[row("j1", "hecho")], NOW);
        assert_eq!(reconcile(&mut l, &[row("j1", "hecho")], NOW), Plan::default());
    }

    /// A shared task that both sides last agreed on as open, at the stamp `synced`.
    fn agreed(updated: i64, synced: i64) -> Task {
        let mut t = shared("a", Some("j1"));
        t.updated_at = updated;
        t.jinx_status = Some("pendiente".into());
        t.jinx_synced = synced;
        t
    }

    #[test]
    fn a_change_on_one_side_only_is_copied_to_the_other() {
        // Only she closed it: we follow her.
        let mut l = vec![agreed(100, 100)];
        let p = reconcile(&mut l, &[row("j1", "hecho")], NOW);
        assert!(l[0].done && p.changed && p.to_resolve.is_empty());
        // Only we closed it (stamped after the agreement): she is told, and we keep it.
        let mut l = vec![agreed(200, 100)];
        l[0].done = true;
        let p = reconcile(&mut l, &[row("j1", "pendiente")], NOW);
        assert!(l[0].done);
        assert_eq!(p.to_resolve, vec![("j1".to_string(), "hecho")]);
        // We reopened a task she had closed: she is told to reopen it.
        let mut l = vec![agreed(200, 100)];
        l[0].jinx_status = Some("hecho".into());
        l[0].done = false;
        let p = reconcile(&mut l, &[row("j1", "hecho")], NOW);
        assert_eq!(p.to_resolve, vec![("j1".to_string(), "reabrir")]);
    }

    #[test]
    fn when_both_changed_the_later_stamp_wins_and_without_a_stamp_she_does() {
        // We closed it at 300; she reopened a closed one at 250 (her gateway says so): ours is later.
        let mut l = vec![agreed(300, 100)];
        l[0].jinx_status = Some("hecho".into());
        l[0].done = true;
        let mut r = row("j1", "pendiente");
        r.updated_at = Some(250);
        let p = reconcile(&mut l, &[r], NOW);
        assert!(l[0].done);
        assert_eq!(p.to_resolve, vec![("j1".to_string(), "hecho")]);
        // Hers is later: we follow her, and take her stamp.
        let mut l = vec![agreed(300, 100)];
        l[0].jinx_status = Some("hecho".into());
        l[0].done = true;
        let mut r = row("j1", "pendiente");
        r.updated_at = Some(400);
        let p = reconcile(&mut l, &[r], NOW);
        assert!(!l[0].done && l[0].updated_at == 400 && l[0].jinx_synced == 400 && p.to_resolve.is_empty());
        // No stamp from her gateway: she wins.
        let mut l = vec![agreed(300, 100)];
        l[0].jinx_status = Some("hecho".into());
        l[0].done = true;
        reconcile(&mut l, &[row("j1", "pendiente")], NOW);
        assert!(!l[0].done);
    }

    #[test]
    fn her_timestamps_are_read_in_every_shape_the_gateway_writes() {
        assert_eq!(iso_to_unix("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(iso_to_unix("2026-10-06T19:30:42+00:00"), Some(1_791_315_042));
        assert_eq!(iso_to_unix("2026-10-06T19:30:42.27+00:00"), Some(1_791_315_042));
        assert_eq!(iso_to_unix("2026-10-06 19:30:42"), Some(1_791_315_042));
        // A zone moves it: 21:30 at +02:00 is 19:30 UTC.
        assert_eq!(iso_to_unix("2026-10-06T21:30:42+02:00"), Some(1_791_315_042));
        assert_eq!(iso_to_unix("2026-10-06T14:30:42-0500"), Some(1_791_315_042));
        for bad in ["", "yesterday", "2026-10-06", "2026-10-06T19:30", "2026-10-06T19:30:42+9"] {
            assert_eq!(iso_to_unix(bad), None, "{bad}");
        }
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
        JinxRow { id: id.into(), titulo: format!("pend {id}"), due: due.map(String::from), categoria: None, source: Some(source.into()), status: status.into(), updated_at: None }
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
        assert!(l.iter().all(|t| t.updated_at == 0), "taken in from her: not a person's change");
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
        assert_eq!(by_id(&l, "c").updated_at, today);
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

    // ── each device works alone; the newest change wins ────────────────────────────

    fn mk(id: &str, title: &str, updated: i64) -> Task {
        Task {
            id: id.into(),
            title: title.into(),
            done: false,
            remind_at: None,
            notified: false,
            created: 1,
            completed_at: None,
            jinx: true,
            jinx_id: None,
            origin: None,
            updated_at: updated,
            deleted: false,
            jinx_status: None,
            jinx_synced: 0,
        }
    }

    #[test]
    fn a_change_is_stamped_and_a_deletion_is_a_tombstone_the_screens_do_not_see() {
        let mut l = Vec::new();
        add(&mut l, "a", None, 100).unwrap();
        assert_eq!(l[0].updated_at, 100);
        let id = l[0].id.clone();
        set_done(&mut l, &id, true, 105).unwrap();
        assert_eq!(l[0].updated_at, 105);
        snooze_at(&mut l, &id, 900, 107).unwrap();
        assert_eq!(l[0].updated_at, 107);
        set_jinx_at(&mut l, &id, false, 108).unwrap();
        assert_eq!(l[0].updated_at, 108);
        delete_at(&mut l, &id, 109).unwrap();
        assert!(l[0].deleted && l[0].updated_at == 109);
        assert!(visible(&l).is_empty());
        assert!(delete_at(&mut l, &id, 110).is_err(), "a tombstone is gone for the person");
        assert!(set_done(&mut l, &id, true, 110).is_err());
        let mut kept = l.clone();
        purge_tombstones(&mut kept, 109 + TOMBSTONE_SECS - 1);
        assert_eq!(kept.len(), 1);
        purge_tombstones(&mut kept, 109 + TOMBSTONE_SECS + 1);
        assert!(kept.is_empty());
        // Clearing the finished ones is a deletion of each.
        let mut l = vec![mk("a", "a", 1), mk("b", "b", 1)];
        l[0].done = true;
        clear_done_at(&mut l, 50);
        assert!(l[0].deleted && l[0].updated_at == 50 && !l[1].deleted && l[1].updated_at == 1);
    }

    #[test]
    fn a_tombstone_with_her_pendiente_linked_is_not_forgotten_before_she_is_told() {
        let mut l = vec![mk("a", "a", 100)];
        l[0].jinx_id = Some("j1".into());
        delete_at(&mut l, "a", 100).unwrap();
        // Old enough to be purged, but her copy is still open and linked: kept.
        let mut kept = l.clone();
        purge_tombstones(&mut kept, 100 + TOMBSTONE_SECS + 1);
        assert_eq!(kept.len(), 1);
        // The sync dismisses her copy (and asks again while it is open)...
        let p = reconcile(&mut l, &[row("j1", "pendiente")], NOW);
        assert_eq!(p.to_resolve, vec![("j1".to_string(), "descartado")]);
        assert!(p.to_add.is_empty());
        assert_eq!(l[0].jinx_id.as_deref(), Some("j1"));
        // ...and once she has none open, the link is let go and the tombstone can go.
        let p = reconcile(&mut l, &[row("j1", "descartado")], NOW);
        assert!(p.to_resolve.is_empty() && p.changed);
        purge_tombstones(&mut l, 100 + TOMBSTONE_SECS + 1);
        assert!(l.is_empty());
        // A tombstone never gets handed to her.
        let mut l = vec![mk("b", "b", 1)];
        l[0].deleted = true;
        assert!(reconcile(&mut l, &[], NOW).to_add.is_empty());
    }

    #[test]
    fn a_file_from_before_the_stamps_loads_and_gets_its_stamp() {
        let old = r#"[{"id":"x","title":"a","done":true,"created":50,"completedAt":80},
                      {"id":"j-9","title":"b","done":false,"created":60},
                      {"id":"y","title":"c","done":false,"created":70,"remindAt":null,"jinx":true}]"#;
        let l = decode(old).unwrap();
        assert_eq!(l.len(), 3);
        assert_eq!(l[0].updated_at, 80);
        assert_eq!(l[1].updated_at, 0, "taken in from Jinx stays at 0");
        assert_eq!(l[2].updated_at, 70);
        assert!(l.iter().all(|t| !t.deleted && t.jinx_status.is_none() && t.jinx_synced == 0));
        // The plain serde path (what load_all uses) takes the file too.
        let raw: Vec<Task> = serde_json::from_str(old).unwrap();
        assert_eq!(raw.len(), 3);
        // And the whole thing survives the Android shape both ways.
        let again = decode(&encode(&l)).unwrap();
        assert_eq!(again, l);
        assert!(encode(&l).contains("\"origin\":null"));
        assert!(encode(&l).contains("\"jinxStatus\":null") && encode(&l).contains("\"updatedAt\":80"));
    }

    #[test]
    fn what_another_device_sends_is_bounded_before_it_is_merged() {
        let mut t = mk("abc", "  hola\u{7}  ", 10);
        t.jinx_id = Some("not hex!".into());
        t.jinx_status = Some("hecho".into());
        t.jinx_synced = 7;
        t.origin = Some("hook".into());
        let s = sanitize_remote(&t).unwrap();
        assert_eq!(s.title, "hola");
        assert_eq!(s.jinx_id, None);
        assert_eq!((s.jinx_status, s.jinx_synced), (None, 0));
        assert_eq!(s.origin.as_deref(), Some("hook"));
        let mut ok = mk("abc", "x", 1);
        ok.jinx_id = Some("00ff00ff".into());
        ok.origin = Some("a b".into());
        let s = sanitize_remote(&ok).unwrap();
        assert_eq!(s.jinx_id.as_deref(), Some("00ff00ff"));
        assert_eq!(s.origin, None, "an origin is a short word");
        assert!(sanitize_remote(&mk("", "x", 1)).is_none());
        assert!(sanitize_remote(&mk(&"x".repeat(65), "x", 1)).is_none());
        assert!(sanitize_remote(&mk("a\nb", "x", 1)).is_none());
        assert!(sanitize_remote(&mk("a", "   ", 1)).is_none());
        assert_eq!(sanitize_remote(&mk("a", &"y".repeat(1000), 1)).unwrap().title.chars().count(), MAX_TITLE);
        let mut tomb = mk("a", "", 1);
        tomb.deleted = true;
        assert!(sanitize_remote(&tomb).is_some(), "a tombstone needs no title");
        let mut long = mk("a", "x", 1);
        long.jinx_id = Some("0".repeat(17));
        assert_eq!(sanitize_remote(&long).unwrap().jinx_id, None);
    }

    #[test]
    fn the_newest_change_wins_whichever_way_round_they_meet() {
        let mut phone = mk("x", "llamar", 100);
        phone.done = true;
        phone.updated_at = 200;
        let mut laptop = mk("x", "llamar", 100);
        laptop.remind_at = Some(500);
        laptop.updated_at = 150;
        let (a, a_changed) = merge(&[phone.clone()], &[laptop.clone()], 1_000);
        let (b, b_changed) = merge(&[laptop], &[phone], 1_000);
        assert!(!a_changed && b_changed);
        assert_eq!(a, b);
        assert!(a[0].done && a[0].updated_at == 200);
        assert!(!merge(&a, &b, 1_000).1, "meeting again changes nothing");
    }

    #[test]
    fn a_wrong_clock_does_not_win_for_ever() {
        let mut future = mk("x", "future", 9_999_999);
        future.done = true;
        let (l, _) = merge(&[mk("x", "a", 1_000)], &[future], 2_000);
        assert_eq!(l[0].updated_at, 2_000 + SKEW_SECS);
        let (l2, _) = merge(&l, &[mk("x", "real", 2_400)], 2_400);
        assert_eq!(l2[0].title, "real");
    }

    #[test]
    fn a_reminder_still_to_come_rings_where_it_is_learnt_and_a_past_one_does_not() {
        let mut soon = mk("s", "pronto", 100);
        soon.remind_at = Some(1_000 + 600);
        soon.notified = true;
        let mut past = mk("p", "ya paso", 100);
        past.remind_at = Some(1_000 - 600);
        let (l, _) = merge(&[], &[soon, past], 1_000);
        assert!(!l.iter().find(|t| t.id == "s").unwrap().notified);
        assert!(l.iter().find(|t| t.id == "p").unwrap().notified);
    }

    #[test]
    fn the_same_pendiente_under_two_ids_is_one_task() {
        let mut here = mk("t-1", "a", 100);
        here.jinx_id = Some("j1".into());
        let mut there = mk("j-j1", "a", 200);
        there.jinx_id = Some("j1".into());
        there.done = true;
        let (l, changed) = merge(&[here], &[there], 1_000);
        assert!(changed);
        assert_eq!(l.len(), 1);
        assert_eq!((l[0].id.as_str(), l[0].done, l[0].updated_at), ("t-1", true, 200), "keeps our id");
    }

    fn project(l: &[Task]) -> Vec<String> {
        let mut v: Vec<String> = l.iter().map(|t| format!("{}|{}|{}|{}|{}", t.id, t.title, t.done, t.deleted, t.updated_at)).collect();
        v.sort();
        v
    }

    #[test]
    fn the_merge_vectors_shared_with_the_phone_hold() {
        let doc: serde_json::Value = serde_json::from_str(include_str!("../../scripts/phone/merge-vectors.json")).unwrap();
        let cases = doc["cases"].as_array().unwrap();
        assert!(!cases.is_empty());
        let read = |v: &serde_json::Value, created: bool| -> Vec<Task> {
            let arr = v.as_array().unwrap();
            let json = serde_json::Value::Array(
                arr.iter()
                    .cloned()
                    .map(|mut o| {
                        if created {
                            o["created"] = 1.into();
                        }
                        o
                    })
                    .collect(),
            )
            .to_string();
            let l = decode(&json).unwrap();
            assert_eq!(l.len(), arr.len(), "every element of a vector is a task");
            l
        };
        for c in cases {
            let name = c["name"].as_str().unwrap();
            let at = c["now"].as_i64().unwrap();
            let local = read(&c["local"], false);
            let remote = read(&c["remote"], false);
            let want = project(&read(&c["expect"], true));
            assert_eq!(want, project(&merge(&local, &remote, at).0), "{name}");
            if c["symmetric"].as_bool().unwrap_or(false) {
                assert_eq!(want, project(&merge(&remote, &local, at).0), "{name} (the other way round)");
            }
        }
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
