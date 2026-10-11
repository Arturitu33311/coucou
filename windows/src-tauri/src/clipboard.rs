// The Hub's clipboard history. A clipboard is where passwords pass through, so this is built to be
// careful rather than complete:
//
//   * off until the user turns it on (Settings → Hub → Clipboard history);
//   * text only, 4 000 characters at most, 40 items; what a password manager marks as secret
//     (`x-kde-passwordManagerHint`, which KeePassXC, Bitwarden and others set) is never kept;
//   * the history lives in memory only: it is gone when Coucou quits. Only what the user pins is
//     written to disk (a file only they can read), because pinning is a deliberate "keep this";
//   * it is never sent anywhere, never shared with Jinx and never logged.
//
// Capture is event-driven: GTK says when another program takes ownership of the clipboard
// (`owner-change`); nothing polls. The text is asked for asynchronously, so a slow or hung
// program holding the clipboard can never freeze the island.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::island::WINDOW_LABEL;

/// Longest text kept (characters).
pub const MAX_CHARS: usize = 4000;
/// Most unpinned items kept.
pub const MAX_ITEMS: usize = 40;
/// Most pinned items kept.
pub const MAX_PINS: usize = 20;

static ENABLED: AtomicBool = AtomicBool::new(false);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static HISTORY: Mutex<History> = Mutex::new(History::new());

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: u64,
    pub text: String,
    /// Unix seconds.
    pub at: u64,
    pub pinned: bool,
}

/// The list, newest first, pinned ones kept at the top by the page.
#[derive(Default)]
pub struct History {
    items: VecDeque<Entry>,
}

impl History {
    pub const fn new() -> Self {
        History { items: VecDeque::new() }
    }

    /// Takes a copied text in. Returns false when it is not kept (empty, too long).
    pub fn push(&mut self, text: &str, at: u64, id: u64) -> bool {
        if text.trim().is_empty() || text.chars().count() > MAX_CHARS {
            return false;
        }
        // The same text again moves to the top (and keeps being pinned if it was).
        let pinned = match self.items.iter().position(|e| e.text == text) {
            Some(i) => self.items.remove(i).is_some_and(|e| e.pinned),
            None => false,
        };
        self.items.push_front(Entry { id, text: text.to_string(), at, pinned });
        // Unpinned ones beyond the limit fall off the end.
        let mut unpinned = 0;
        self.items.retain(|e| {
            if e.pinned {
                return true;
            }
            unpinned += 1;
            unpinned <= MAX_ITEMS
        });
        true
    }

    pub fn set_pinned(&mut self, id: u64, pinned: bool) -> bool {
        if pinned && self.items.iter().filter(|e| e.pinned && e.id != id).count() >= MAX_PINS {
            return false;
        }
        match self.items.iter_mut().find(|e| e.id == id) {
            Some(e) => {
                e.pinned = pinned;
                true
            }
            None => false,
        }
    }

    pub fn delete(&mut self, id: u64) {
        self.items.retain(|e| e.id != id);
    }

    /// Forgets everything that is not pinned.
    pub fn clear_unpinned(&mut self) {
        self.items.retain(|e| e.pinned);
    }

    pub fn get(&self, id: u64) -> Option<&Entry> {
        self.items.iter().find(|e| e.id == id)
    }

    pub fn list(&self) -> Vec<Entry> {
        self.items.iter().cloned().collect()
    }

    pub fn pinned(&self) -> Vec<Entry> {
        self.items.iter().filter(|e| e.pinned).cloned().collect()
    }

    /// Puts back what was pinned in an earlier run (below anything newer).
    pub fn restore(&mut self, pins: Vec<Entry>) {
        for mut e in pins.into_iter().take(MAX_PINS) {
            if e.text.trim().is_empty() || e.text.chars().count() > MAX_CHARS || self.items.iter().any(|x| x.text == e.text) {
                continue;
            }
            e.pinned = true;
            self.items.push_back(e);
        }
    }
}

/// Does a list of the clipboard's targets say a password manager put this there?
pub fn is_secret(targets: &[String]) -> bool {
    targets.iter().any(|t| {
        let t = t.to_lowercase();
        t == "x-kde-passwordmanagerhint" || t.contains("passwordmanagerhint")
    })
}

/// May what the clipboard holds be kept? Only when its kinds of data could be read, it says it is
/// text, and no password manager marked it secret — an unreadable or empty list is "no".
pub fn may_keep(targets: &[String]) -> bool {
    if targets.is_empty() || is_secret(targets) {
        return false;
    }
    targets.iter().any(|t| {
        let t = t.to_lowercase();
        t == "utf8_string" || t == "string" || t == "text" || t.starts_with("text/plain")
    })
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn pins_path() -> std::path::PathBuf {
    crate::settings::local_dir().join("clipboard-pins.json")
}

fn save_pins() {
    let pins = HISTORY.lock().unwrap().pinned();
    let p = pins_path();
    if pins.is_empty() {
        let _ = std::fs::remove_file(&p);
        return;
    }
    if let Some(dir) = p.parent() {
        let _ = crate::platform::ensure_private_dir(dir);
    }
    if let Ok(json) = serde_json::to_string(&pins) {
        let tmp = p.with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
            }
            let _ = std::fs::rename(&tmp, &p);
        }
    }
}

fn load_pins() -> Vec<Entry> {
    std::fs::read_to_string(pins_path())
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<Entry>>(&s).ok())
        .unwrap_or_default()
}

// ── Commands' side ────────────────────────────────────────────────────────────

pub fn list() -> Vec<Entry> {
    HISTORY.lock().unwrap().list()
}

pub fn pin(id: u64, pinned: bool) -> Result<Vec<Entry>, String> {
    if !HISTORY.lock().unwrap().set_pinned(id, pinned) {
        return Err(format!("At most {MAX_PINS} pinned items"));
    }
    save_pins();
    Ok(list())
}

pub fn delete(id: u64) -> Vec<Entry> {
    HISTORY.lock().unwrap().delete(id);
    save_pins();
    list()
}

pub fn clear() -> Vec<Entry> {
    HISTORY.lock().unwrap().clear_unpinned();
    list()
}

pub fn text_of(id: u64) -> Option<String> {
    HISTORY.lock().unwrap().get(id).map(|e| e.text.clone())
}

/// Called when the setting changes (and at start). Turning it off forgets the unpinned history.
pub fn sync_enabled(app: &AppHandle, on: bool) {
    let was = ENABLED.swap(on, Ordering::SeqCst);
    if on && !was {
        HISTORY.lock().unwrap().restore(load_pins());
        #[cfg(target_os = "linux")]
        linux::watch(app);
    } else if !on && was {
        HISTORY.lock().unwrap().clear_unpinned();
        let _ = app.emit_to(WINDOW_LABEL, "clipboard", list());
    }
}

/// Copies an item back to the clipboard.
pub fn copy_back(app: &AppHandle, id: u64) -> Result<(), String> {
    let text = text_of(id).ok_or("That item is gone")?;
    #[cfg(target_os = "linux")]
    {
        linux::set_text(app, text);
        Ok(())
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (app, text);
        Err("Not available here".into())
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;
    use gtk::gdk;
    use gtk::prelude::*;

    static WATCHING: AtomicBool = AtomicBool::new(false);

    /// Hooks the clipboard's owner-change on GTK's main thread (once).
    pub fn watch(app: &AppHandle) {
        if WATCHING.swap(true, Ordering::SeqCst) {
            return;
        }
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            let clipboard = gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD);
            let app = handle.clone();
            // gtk-rs has no typed `owner-change`; the signal is connected by name.
            clipboard.connect_local("owner-change", false, move |values| {
                if !ENABLED.load(Ordering::SeqCst) {
                    return None;
                }
                let cb = values.first()?.get::<gtk::Clipboard>().ok()?;
                let app = app.clone();
                // First what kinds of data it holds (a password manager says so here), then,
                // only if it is not secret, the text. Both asynchronous.
                cb.request_contents(&gdk::Atom::intern("TARGETS"), move |cb, data| {
                    // If what it holds cannot be read, nothing is kept: the filter fails closed.
                    let Some(atoms) = data.targets() else { return };
                    let names: Vec<String> = atoms.iter().map(|a| a.name().to_string()).collect();
                    if !may_keep(&names) {
                        return;
                    }
                    let app = app.clone();
                    cb.request_text(move |_, text| {
                        let Some(text) = text else { return };
                        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
                        if HISTORY.lock().unwrap().push(&text, now_secs(), id) {
                            let _ = app.emit_to(WINDOW_LABEL, "clipboard", list());
                        }
                    });
                });
                None
            });
        });
    }

    pub fn set_text(app: &AppHandle, text: String) {
        let _ = app.run_on_main_thread(move || {
            gtk::Clipboard::get(&gdk::SELECTION_CLIPBOARD).set_text(&text);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h() -> History {
        History::new()
    }

    #[test]
    fn what_is_copied_comes_in_newest_first_and_the_same_text_moves_up() {
        let mut x = h();
        assert!(x.push("one", 1, 1));
        assert!(x.push("two", 2, 2));
        assert!(x.push("one", 3, 3));
        let texts: Vec<_> = x.list().into_iter().map(|e| e.text).collect();
        assert_eq!(texts, vec!["one", "two"]);
        assert_eq!(x.list()[0].at, 3);
    }

    #[test]
    fn empty_and_huge_texts_are_not_kept() {
        let mut x = h();
        assert!(!x.push("   \n ", 1, 1));
        assert!(!x.push(&"a".repeat(MAX_CHARS + 1), 1, 2));
        assert!(x.push(&"a".repeat(MAX_CHARS), 1, 3));
        assert_eq!(x.list().len(), 1);
    }

    #[test]
    fn only_so_many_unpinned_items_are_kept_and_pinned_ones_stay() {
        let mut x = h();
        x.push("keep me", 0, 1000);
        assert!(x.set_pinned(1000, true));
        for i in 0..(MAX_ITEMS + 10) {
            x.push(&format!("item {i}"), i as u64, i as u64);
        }
        let list = x.list();
        assert_eq!(list.iter().filter(|e| !e.pinned).count(), MAX_ITEMS);
        assert!(list.iter().any(|e| e.pinned && e.text == "keep me"), "pinned items never fall off");
    }

    #[test]
    fn pins_are_limited_and_survive_clearing() {
        let mut x = h();
        for i in 0..(MAX_PINS + 1) {
            x.push(&format!("p{i}"), 0, i as u64);
        }
        for i in 0..MAX_PINS {
            assert!(x.set_pinned(i as u64, true));
        }
        assert!(!x.set_pinned(MAX_PINS as u64, true), "one too many");
        x.clear_unpinned();
        assert_eq!(x.list().len(), MAX_PINS);
        x.delete(0);
        assert_eq!(x.list().len(), MAX_PINS - 1);
    }

    #[test]
    fn a_pinned_text_copied_again_stays_pinned() {
        let mut x = h();
        x.push("token", 1, 1);
        x.set_pinned(1, true);
        x.push("token", 2, 2);
        assert_eq!(x.list().len(), 1);
        assert!(x.list()[0].pinned);
    }

    #[test]
    fn restored_pins_go_below_new_items_and_skip_duplicates() {
        let mut x = h();
        x.push("new", 5, 1);
        x.restore(vec![
            Entry { id: 9, text: "old pin".into(), at: 1, pinned: true },
            Entry { id: 8, text: "new".into(), at: 1, pinned: true },
            Entry { id: 7, text: "".into(), at: 1, pinned: true },
        ]);
        let texts: Vec<_> = x.list().into_iter().map(|e| e.text).collect();
        assert_eq!(texts, vec!["new", "old pin"]);
    }

    #[test]
    fn what_a_password_manager_marks_is_never_kept() {
        let secret = vec!["text/plain".to_string(), "x-kde-passwordManagerHint".to_string()];
        assert!(is_secret(&secret));
        assert!(is_secret(&["X-KDE-PasswordManagerHint".to_string()]));
        assert!(!is_secret(&["text/plain".to_string(), "UTF8_STRING".to_string()]));
        assert!(!is_secret(&[]));
    }

    #[test]
    fn the_filter_fails_closed() {
        let t = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert!(may_keep(&t(&["UTF8_STRING", "text/plain;charset=utf-8"])));
        assert!(may_keep(&t(&["TARGETS", "text/plain"])));
        assert!(!may_keep(&[]), "the kinds of data could not be read");
        assert!(!may_keep(&t(&["image/png"])), "not text");
        assert!(!may_keep(&t(&["text/plain", "x-kde-passwordManagerHint"])), "marked secret");
    }
}
