// The Hub's workspaces: a named set of apps, folders and links that open together with one
// click ("Study": Zotero, the course folder, the school's site). They are kept in Coucou's own
// folder (workspaces.json). Nothing here runs a shell: an app is started through its own
// .desktop file, a folder or a link through `gio open`, each with the arguments as separate
// words, and only what is installed or what was saved by the user can be started at all.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::settings;

pub const MAX_WORKSPACES: usize = 20;
pub const MAX_ITEMS: usize = 12;
const MAX_TEXT: usize = 500;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Item {
    /// "app" (target: the .desktop id), "folder" (a path) or "url" (http or https).
    pub kind: String,
    pub target: String,
    /// What it is called in the list.
    pub label: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub items: Vec<Item>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct App {
    /// The .desktop file name, e.g. "org.gnome.Nautilus.desktop".
    pub id: String,
    pub name: String,
    #[serde(skip)]
    pub path: PathBuf,
}

static LOCK: Mutex<()> = Mutex::new(());

fn path() -> PathBuf {
    settings::local_dir().join("workspaces.json")
}

pub fn load() -> Vec<Workspace> {
    std::fs::read_to_string(path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// Checks and tidies what the page sends before it is kept (or ever acted on).
pub fn clean(list: Vec<Workspace>) -> Result<Vec<Workspace>, String> {
    if list.len() > MAX_WORKSPACES {
        return Err(format!("{MAX_WORKSPACES} workspaces at most"));
    }
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for mut w in list {
        w.name = w.name.trim().chars().take(60).collect();
        if w.name.is_empty() {
            return Err("A workspace needs a name".into());
        }
        if w.id.is_empty() || w.id.len() > 40 || !w.id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') || !seen.insert(w.id.clone()) {
            return Err("A workspace has a bad id".into());
        }
        if w.items.len() > MAX_ITEMS {
            return Err(format!("{MAX_ITEMS} items at most in a workspace"));
        }
        for i in &mut w.items {
            i.target = i.target.trim().to_string();
            i.label = i.label.trim().chars().take(80).collect();
            if i.target.is_empty() || i.target.len() > MAX_TEXT || i.target.contains('\0') || i.target.contains('\n') {
                return Err("An item has no usable target".into());
            }
            match i.kind.as_str() {
                "app" => {
                    let stem = i.target.strip_suffix(".desktop").unwrap_or("");
                    if stem.is_empty() || !stem.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')) {
                        return Err(format!("“{}” is not an app", i.target));
                    }
                }
                "url" => {
                    if !(i.target.starts_with("https://") || i.target.starts_with("http://")) {
                        return Err("A link must start with http:// or https://".into());
                    }
                }
                "folder" => {}
                _ => return Err("Unknown kind of item".into()),
            }
            if i.label.is_empty() {
                i.label = i.target.clone();
            }
        }
        out.push(w);
    }
    Ok(out)
}

pub fn save(list: Vec<Workspace>) -> Result<Vec<Workspace>, String> {
    let list = clean(list)?;
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let p = path();
    if let Some(dir) = p.parent() {
        crate::platform::ensure_private_dir(dir).map_err(|e| e.to_string())?;
    }
    save_to(&p, &list)?;
    Ok(list)
}

fn save_to(p: &Path, list: &[Workspace]) -> Result<(), String> {
    let text = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, p).map_err(|e| e.to_string())
}

// ── The apps that are installed ─────────────────────────────────────────────────

/// (Name, shown?) of a .desktop file, or None if it is not an application.
fn parse_desktop(text: &str) -> Option<(String, bool)> {
    let mut in_entry = false;
    let (mut name, mut kind, mut hidden) = (None, None, false);
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            match k.trim() {
                "Name" => name = Some(v.trim().to_string()),
                "Type" => kind = Some(v.trim().to_string()),
                "NoDisplay" | "Hidden" if v.trim() == "true" => hidden = true,
                _ => {}
            }
        }
    }
    if kind.as_deref() != Some("Application") {
        return None;
    }
    Some((name?, !hidden))
}

fn data_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let data_home = std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| home.join(".local/share"));
    let mut dirs = vec![data_home, home.join(".local/share/flatpak/exports/share")];
    match std::env::var("XDG_DATA_DIRS") {
        Ok(v) if !v.is_empty() => dirs.extend(v.split(':').filter(|s| s.starts_with('/')).map(PathBuf::from)),
        _ => dirs.extend(["/usr/local/share", "/usr/share"].map(PathBuf::from)),
    }
    dirs.extend(["/var/lib/flatpak/exports/share", "/var/lib/snapd/desktop"].map(PathBuf::from));
    dirs
}

/// The shown applications of these data directories, the first of a name winning (so the
/// user's own copy of a launcher beats the system's), sorted by name.
fn apps_in(dirs: &[PathBuf]) -> Vec<App> {
    let mut seen = HashSet::new();
    let mut apps = Vec::new();
    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(dir.join("applications")) else { continue };
        for e in rd.flatten() {
            let id = e.file_name().to_string_lossy().to_string();
            if !id.ends_with(".desktop") || !seen.insert(id.clone()) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(e.path()) else { continue };
            if let Some((name, shown)) = parse_desktop(&text) {
                if shown && !name.is_empty() {
                    apps.push(App { id, name, path: e.path() });
                }
            }
        }
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps
}

pub fn list_apps() -> Vec<App> {
    apps_in(&data_dirs())
}

// ── Starting things ─────────────────────────────────────────────────────────────

/// What a program Coucou starts must not inherit: the settings its own launcher puts in the
/// environment for the island (X11 mode, the WebKit workaround, the folder with the library
/// the system does not have). Returns (name, new value) — None means "remove it".
fn env_changes<I: Iterator<Item = (String, String)>>(vars: I) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    for (k, v) in vars {
        if k.starts_with("COUCOU_") || k == "WEBKIT_DISABLE_COMPOSITING_MODE" || k == "GDK_BACKEND" {
            out.push((k, None));
        } else if k == "LD_LIBRARY_PATH" && v.contains("coucou-app") {
            let rest: Vec<&str> = v.split(':').filter(|p| !p.is_empty() && !p.contains("coucou-app")).collect();
            out.push((k, if rest.is_empty() { None } else { Some(rest.join(":")) }));
        }
    }
    out
}

fn spawn_detached(program: &str, args: &[&str]) -> Result<(), String> {
    use std::process::{Command, Stdio};
    let mut cmd = Command::new(program);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    for (k, v) in env_changes(std::env::vars()) {
        match v {
            Some(v) => cmd.env(k, v),
            None => cmd.env_remove(k),
        };
    }
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::process::CommandExt;
        // Its own session: closing Coucou does not close what it opened.
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }
    let mut child = cmd.spawn().map_err(|e| format!("{program}: {e}"))?;
    // Reaped in the background, so a finished launcher is not left as a zombie.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

fn expand_home(p: &str) -> PathBuf {
    match p.strip_prefix("~/") {
        Some(rest) => std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default().join(rest),
        None if p == "~" => std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default(),
        None => PathBuf::from(p),
    }
}

/// Opens every item of workspace `id`. Returns what could not be opened (empty: all went).
pub fn launch(id: &str) -> Result<Vec<String>, String> {
    let ws = load().into_iter().find(|w| w.id == id).ok_or("That workspace is gone")?;
    // Re-checked even though the page validated it: the file may have been edited by hand.
    let ws = clean(vec![ws])?.remove(0);
    let apps = list_apps();
    let mut failed = Vec::new();
    for item in &ws.items {
        let result = match item.kind.as_str() {
            "app" => match apps.iter().find(|a| a.id == item.target) {
                Some(a) => spawn_detached("gio", &["launch", &a.path.to_string_lossy()]),
                None => Err("not installed".into()),
            },
            "folder" => {
                let p = expand_home(&item.target);
                if p.is_dir() {
                    spawn_detached("gio", &["open", &p.to_string_lossy()])
                } else {
                    Err("folder not found".into())
                }
            }
            "url" => spawn_detached("gio", &["open", &item.target]),
            _ => Err("unknown kind".into()),
        };
        if let Err(e) = result {
            failed.push(format!("{}: {e}", item.label));
        }
    }
    Ok(failed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(id: &str, items: Vec<Item>) -> Workspace {
        Workspace { id: id.into(), name: "Study".into(), items }
    }
    fn item(kind: &str, target: &str) -> Item {
        Item { kind: kind.into(), target: target.into(), label: String::new() }
    }

    #[test]
    fn a_desktop_file_gives_its_name_unless_it_is_hidden_or_not_an_app() {
        let app = "[Desktop Entry]\nType=Application\nName=Files\nName[es]=Archivos\nExec=nautilus\n[Desktop Action new]\nName=New window\n";
        assert_eq!(parse_desktop(app), Some(("Files".into(), true)));
        assert_eq!(parse_desktop("[Desktop Entry]\nType=Application\nName=Helper\nNoDisplay=true\n"), Some(("Helper".into(), false)));
        assert_eq!(parse_desktop("[Desktop Entry]\nType=Link\nName=Site\n"), None);
        assert_eq!(parse_desktop("[Desktop Action x]\nName=Only an action\n"), None);
        assert_eq!(parse_desktop("[Desktop Entry]\nType=Application\n"), None, "no name");
    }

    #[test]
    fn installed_apps_come_from_every_folder_the_first_copy_wins_and_hidden_ones_are_left_out() {
        let root = std::env::temp_dir().join(format!("coucou-apps-{}", std::process::id()));
        let (a, b) = (root.join("user"), root.join("system"));
        for d in [&a, &b] {
            std::fs::create_dir_all(d.join("applications")).unwrap();
        }
        std::fs::write(a.join("applications/zed.desktop"), "[Desktop Entry]\nType=Application\nName=Zed (mine)\n").unwrap();
        std::fs::write(b.join("applications/zed.desktop"), "[Desktop Entry]\nType=Application\nName=Zed (system)\n").unwrap();
        std::fs::write(b.join("applications/alpha.desktop"), "[Desktop Entry]\nType=Application\nName=alpha\n").unwrap();
        std::fs::write(b.join("applications/ghost.desktop"), "[Desktop Entry]\nType=Application\nName=Ghost\nHidden=true\n").unwrap();
        std::fs::write(b.join("applications/readme.txt"), "not an app").unwrap();
        let apps = apps_in(&[a, b, root.join("missing")]);
        let names: Vec<_> = apps.iter().map(|x| x.name.as_str()).collect();
        assert_eq!(names, ["alpha", "Zed (mine)"]);
        assert_eq!(apps[1].id, "zed.desktop");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn what_the_page_sends_is_checked_before_it_is_kept() {
        assert!(clean(vec![ws("a1", vec![item("app", "org.gnome.Nautilus.desktop"), item("folder", "~/Documents"), item("url", "https://moodle.example/x")])]).is_ok());
        assert!(clean(vec![ws("a1", vec![item("url", "javascript:alert(1)")])]).is_err(), "only web links");
        assert!(clean(vec![ws("a1", vec![item("url", "file:///etc/passwd")])]).is_err());
        assert!(clean(vec![ws("a1", vec![item("app", "x; rm -rf ~.desktop")])]).is_err(), "an app id is a plain name");
        assert!(clean(vec![ws("a1", vec![item("app", "firefox")])]).is_err(), "and ends in .desktop");
        assert!(clean(vec![ws("a1", vec![item("shell", "ls")])]).is_err());
        assert!(clean(vec![ws("a1", vec![item("folder", "a\nb")])]).is_err());
        assert!(clean(vec![ws("a1", (0..=MAX_ITEMS).map(|_| item("folder", "/tmp")).collect())]).is_err());
        assert!(clean(vec![ws("a/../b", vec![])]).is_err(), "an id is plain");
        assert!(clean(vec![ws("a1", vec![]), ws("a1", vec![])]).is_err(), "ids are unique");
        let mut blank = ws("a1", vec![]);
        blank.name = "  ".into();
        assert!(clean(vec![blank]).is_err());
        let tidy = clean(vec![ws("a1", vec![item("folder", " /tmp ")])]).unwrap();
        assert_eq!((tidy[0].items[0].target.as_str(), tidy[0].items[0].label.as_str()), ("/tmp", "/tmp"), "trimmed, and a label is never empty");
    }

    #[test]
    fn what_coucous_launcher_sets_is_not_passed_on() {
        let vars = [
            ("HOME", "/home/a"),
            ("COUCOU_X11_MODE", "notification"),
            ("GDK_BACKEND", "x11"),
            ("WEBKIT_DISABLE_COMPOSITING_MODE", "1"),
            ("LD_LIBRARY_PATH", "/home/a/.local/share/coucou-app/lib:/opt/other"),
        ]
        .map(|(k, v)| (k.to_string(), v.to_string()));
        let ch = env_changes(vars.into_iter());
        assert!(ch.contains(&("COUCOU_X11_MODE".into(), None)));
        assert!(ch.contains(&("GDK_BACKEND".into(), None)));
        assert!(ch.contains(&("WEBKIT_DISABLE_COMPOSITING_MODE".into(), None)));
        assert!(ch.contains(&("LD_LIBRARY_PATH".into(), Some("/opt/other".into()))), "the user's own entries stay");
        assert!(!ch.iter().any(|(k, _)| k == "HOME"));
        let only = env_changes([("LD_LIBRARY_PATH".to_string(), "/home/a/.local/share/coucou-app/lib".to_string())].into_iter());
        assert_eq!(only, [("LD_LIBRARY_PATH".to_string(), None)], "nothing left: removed, not emptied");
    }

    #[test]
    fn a_saved_list_survives_and_leaves_no_temporary_file() {
        let dir = std::env::temp_dir().join(format!("coucou-ws-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("workspaces.json");
        let l = vec![ws("a1", vec![item("folder", "/tmp")])];
        save_to(&file, &l).unwrap();
        assert_eq!(serde_json::from_str::<Vec<Workspace>>(&std::fs::read_to_string(&file).unwrap()).unwrap(), l);
        assert!(!dir.join("workspaces.json.tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
