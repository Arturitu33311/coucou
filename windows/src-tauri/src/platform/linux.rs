// Linux: XDG directories for files, xdg-open for links and folders, and
// gtk-layer-shell for the island window.
//
// Wayland gives an app no global cursor position and no say over where its
// window goes, so the island works differently from Windows:
//   * it is a layer-shell surface anchored to the top edge, above everything,
//     on compositors that support it (COSMIC, KDE, wlroots — not GNOME);
//   * click-through is the window's input region, set to the island shape, so
//     the compositor itself sends every other click to whatever is underneath;
//   * the cursor comes from the page's own mouse events, which only fire over
//     the island — Mochi's eyes follow the pointer there, not across the screen.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use gtk::glib::translate::ToGlibPtr;
use gtk::prelude::*;
use tauri::{AppHandle, WebviewWindow};

use super::{home_dir, LocalTime};

/// File name of the Claude Code relay.
pub const HOOK_EXE: &str = "coucou-hook";

/// Environment variable holding the home directory.
pub const HOME_VAR: &str = "HOME";

// ── Files ─────────────────────────────────────────────────────────────────────

/// An XDG base directory (`$XDG_CONFIG_HOME` …), or its fallback under the home
/// directory when it is unset or not absolute.
fn xdg(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home_dir().join(fallback))
}

/// ~/.config/coucou — preferences.
pub fn config_dir() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("coucou")
}

/// ~/.local/share/coucou — where coucou-hook, the inbox and the log live. The
/// relay has to sit at a stable path: an AppImage is mounted somewhere new on
/// every launch.
pub fn local_dir() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join("coucou")
}

/// Where a saved image goes, best first: the XDG pictures folder named in
/// ~/.config/user-dirs.dirs, then ~/Pictures, ~/Downloads and the home folder.
/// The caller takes the first one that exists.
pub fn picture_dirs() -> Vec<PathBuf> {
    let home = home_dir();
    let mut dirs = Vec::new();
    let user_dirs = xdg("XDG_CONFIG_HOME", ".config").join("user-dirs.dirs");
    if let Ok(text) = std::fs::read_to_string(user_dirs) {
        if let Some(dir) = xdg_user_dir(&text, "XDG_PICTURES_DIR", &home) {
            dirs.push(dir);
        }
    }
    dirs.push(home.join("Pictures"));
    dirs.push(home.join("Downloads"));
    dirs.push(home);
    dirs
}

/// `XDG_PICTURES_DIR="$HOME/Pictures"` → /home/me/Pictures. Only the two forms
/// xdg-user-dirs writes are understood: "$HOME/…" and an absolute path.
fn xdg_user_dir(text: &str, key: &str, home: &Path) -> Option<PathBuf> {
    let line = text.lines().map(str::trim).find(|l| l.starts_with(key))?;
    let value = line.strip_prefix(key)?.trim_start().strip_prefix('=')?.trim();
    let value = value.strip_prefix('"')?.strip_suffix('"')?;
    let path = match value.strip_prefix("$HOME") {
        Some(rest) => home.join(rest.trim_start_matches('/')),
        None => PathBuf::from(value),
    };
    // "$HOME/" alone means "no pictures folder", by the spec.
    (path.is_absolute() && path != home).then_some(path)
}

/// Environment the webview must inherit, set before any thread or process
/// starts.
///
/// Inside an AppImage, WebKit uses the GStreamer bundled with it, and GStreamer
/// keeps its plugin registry in ~/.cache/gstreamer-1.0 by default — the same
/// file the system's GStreamer uses. The AppImage is mounted somewhere new on
/// every launch, so each launch would rewrite the system's registry with
/// plugin paths that vanish once Coucou quits. Give ours its own file.
pub fn prepare_environment() {
    prefer_x11_on_gnome();
    follow_gnome_text_scaling();
    if std::env::var_os("APPIMAGE").is_none() || std::env::var_os("GST_REGISTRY").is_some() {
        return;
    }
    let cache = xdg("XDG_CACHE_HOME", ".cache").join("coucou");
    if std::fs::create_dir_all(&cache).is_ok() {
        std::env::set_var("GST_REGISTRY", cache.join("gstreamer-registry.bin"));
    }
}

/// GNOME on Wayland has no layer-shell and ignores where a regular window asks
/// to go, so the island lands in the middle of the screen. Through XWayland it
/// can be placed, and a Dock window survives "show desktop" (Super+D).
/// An inherited GDK_BACKEND=wayland is overridden too: editors and terminals
/// pass theirs down to every child. COUCOU_X11=0 keeps the Wayland window.
fn prefer_x11_on_gnome() {
    let env = |k: &str| std::env::var(k).unwrap_or_default();
    if should_prefer_x11(
        &env("XDG_SESSION_TYPE"),
        &env("XDG_CURRENT_DESKTOP"),
        &env("COUCOU_X11"),
        &env("DISPLAY"),
    ) {
        std::env::set_var("GDK_BACKEND", "x11");
    }
}

/// Only with XWayland actually there (`DISPLAY` set): forcing x11 without it
/// would leave GTK with no display at all.
fn should_prefer_x11(session: &str, desktop: &str, opt: &str, display: &str) -> bool {
    session.eq_ignore_ascii_case("wayland")
        && desktop.split(':').any(|d| d.eq_ignore_ascii_case("gnome"))
        && opt != "0"
        && !display.trim().is_empty()
}

fn is_gnome(desktop: &str) -> bool {
    desktop.split(':').any(|d| d.eq_ignore_ascii_case("gnome"))
}

/// GNOME's "Large text" (text-scaling-factor > 1) makes WebKitGTK draw every
/// font larger inside a window sized in fixed pixels, so the island's content
/// was cut off (#122). The island's layout is its own, so it asks GDK to undo
/// that factor for this process only. An explicit GDK_DPI_SCALE always wins.
fn follow_gnome_text_scaling() {
    if std::env::var_os("GDK_DPI_SCALE").is_some()
        || !is_gnome(&std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default())
    {
        return;
    }
    let Ok(out) = Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "text-scaling-factor"])
        .output()
    else {
        return;
    };
    if let Some(scale) = dpi_scale_for(&String::from_utf8_lossy(&out.stdout)) {
        std::env::set_var("GDK_DPI_SCALE", scale);
    }
}

/// `"1.25\n"` → `Some("0.8")`; `None` for 1.0 or anything unreadable.
fn dpi_scale_for(gsettings_output: &str) -> Option<String> {
    let factor: f64 = gsettings_output.trim().parse().ok()?;
    if !(0.5..=3.0).contains(&factor) || (factor - 1.0).abs() < 0.01 {
        return None;
    }
    Some(format!("{:.4}", 1.0 / factor).trim_end_matches('0').trim_end_matches('.').to_string())
}

pub fn local_time() -> LocalTime {
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe {
        let now = libc::time(std::ptr::null_mut());
        libc::localtime_r(&now, &mut tm);
    }
    LocalTime {
        year: (tm.tm_year + 1900) as u32,
        month: (tm.tm_mon + 1) as u32,
        day: tm.tm_mday as u32,
        hour: tm.tm_hour as u32,
        minute: tm.tm_min as u32,
        second: tm.tm_sec as u32,
    }
}

/// Creates `dir` and closes it to other users. The log, the inbox of dropped
/// files and the relay binary live under these directories; with the default
/// umask they would come out 0755 and readable by anyone on the machine.
pub fn ensure_private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
}

/// True when `dir` is a real directory (not a symlink), owned by us, with no
/// access for group or others: what `$XDG_RUNTIME_DIR` promises, checked
/// rather than assumed, since the socket in it decides who can answer a
/// permission request.
fn is_private_dir(dir: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    std::fs::symlink_metadata(dir)
        .map(|m| {
            m.file_type().is_dir() && m.uid() == unsafe { libc::getuid() } && m.mode() & 0o077 == 0
        })
        .unwrap_or(false)
}

/// Where coucou-hook finds us: `$XDG_RUNTIME_DIR/coucou.sock`, or
/// `/run/user/<uid>/coucou.sock` when the variable is missing. A directory
/// that is not ours and private means no relay at all — never a fallback to a
/// shared place like /tmp. Must match `socket_path()` in hook/src/unix.rs
/// exactly.
pub fn relay_socket_path() -> Option<PathBuf> {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", unsafe { libc::getuid() })));
    is_private_dir(&dir).then(|| dir.join("coucou.sock"))
}

// ── Who we are ────────────────────────────────────────────────────────────────

/// The account's full name: the first field of the passwd GECOS entry
/// ("Louis Raille,,,"), or nothing when it is unset.
pub fn user_full_name() -> Option<String> {
    unsafe {
        let mut pwd: libc::passwd = std::mem::zeroed();
        let mut result: *mut libc::passwd = std::ptr::null_mut();
        let mut buf = vec![0 as libc::c_char; 16 * 1024];
        let rc = libc::getpwuid_r(libc::getuid(), &mut pwd, buf.as_mut_ptr(), buf.len(), &mut result);
        if rc != 0 || result.is_null() || pwd.pw_gecos.is_null() {
            return None;
        }
        let gecos = std::ffi::CStr::from_ptr(pwd.pw_gecos).to_string_lossy().into_owned();
        let name = gecos.split(',').next().unwrap_or("").trim().to_string();
        (!name.is_empty()).then_some(name)
    }
}

// ── Processes ─────────────────────────────────────────────────────────────────

/// Nothing to hide: a spawned process only gets a terminal if it asks for one.
pub fn no_console(cmd: &mut Command) -> &mut Command {
    cmd
}

pub fn open_url(url: &str) {
    let _ = Command::new("xdg-open").arg(url).spawn();
}

pub fn reveal_folder(path: &str) {
    let _ = Command::new("xdg-open").arg(path).spawn();
}

/// Our own `which`: the first executable file named `stem` on $PATH.
pub fn find_on_path(stem: &str) -> Option<PathBuf> {
    let dirs = std::env::var_os("PATH")?;
    std::env::split_paths(&dirs)
        .map(|dir| dir.join(stem))
        .find(|p| {
            std::fs::metadata(p)
                .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
                .unwrap_or(false)
        })
}

// ── Session windows ("Open terminal") ─────────────────────────────────────────
//
// Finding and raising a session's window lives in linux_focus.rs. There is no
// Claude desktop app for Linux.

pub fn open_claude_desktop() -> bool {
    false
}

/// Where the Codex CLI may be, best first: $PATH, then the usual per-user
/// install folders, which a desktop launch often leaves out of $PATH (npm's
/// global prefix, Volta, Bun, pnpm, and nvm with its newest Node first).
pub fn codex_candidates() -> Vec<PathBuf> {
    let home = home_dir();
    let mut out: Vec<PathBuf> = find_on_path("codex").into_iter().collect();
    for dir in [".local/bin", ".npm-global/bin", ".volta/bin", ".bun/bin", ".local/share/pnpm"] {
        out.push(home.join(dir).join("codex"));
    }
    out.push(PathBuf::from("/usr/local/bin/codex"));
    out.push(PathBuf::from("/usr/bin/codex"));
    let nvm = home.join(".nvm/versions/node");
    if let Ok(entries) = std::fs::read_dir(&nvm) {
        let mut versions: Vec<String> =
            entries.filter_map(|e| e.ok()?.file_name().into_string().ok()).collect();
        versions.sort_by(|a, b| crate::codex_plan::compare_versions(b, a));
        out.extend(versions.iter().map(|v| nvm.join(v).join("bin/codex")));
    }
    out.retain(|p| {
        std::fs::metadata(p)
            .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
    });
    out
}

// ── Cursor ────────────────────────────────────────────────────────────────────

/// Nothing polls the cursor here: the page reports it over the island, and the
/// input region decides click-through (see the top of this file).
pub const CURSOR_POLL: bool = false;

pub fn cursor_physical() -> Option<(f64, f64)> {
    None
}

pub fn left_button_down() -> bool {
    false
}

/// Whether the pointer can be read anywhere on screen: only on a real X11 session.
pub fn gaze_supported() -> bool {
    if std::env::var("XDG_SESSION_TYPE").map(|v| v == "wayland").unwrap_or(false) {
        return false;
    }
    gtk::gdk::Display::default()
        .map(|d| d.type_().name().contains("X11"))
        .unwrap_or(false)
}

/// The pointer in window-logical coordinates, wherever it is on screen.
/// Must run on the main thread (GDK).
pub fn pointer_in_window(win: &WebviewWindow) -> Option<(f64, f64)> {
    let gw = win.gtk_window().ok()?;
    let origin = gw.window()?.origin();
    let pointer = gtk::gdk::Display::default()?.default_seat()?.pointer()?;
    let (_, px, py) = pointer.position();
    Some(((px - origin.1) as f64, (py - origin.2) as f64))
}

/// How many window buttons the desktop puts on the left and on the right of a title bar
/// (`button-layout`, e.g. `appmenu:minimize,maximize,close`: none, then three).
fn button_sides() -> (usize, usize) {
    // Asked four times a second by the dodge: the setting is read again every few seconds, not every time.
    thread_local! {
        static CACHE: std::cell::Cell<Option<(std::time::Instant, (usize, usize))>> = const { std::cell::Cell::new(None) };
    }
    const FRESH: std::time::Duration = std::time::Duration::from_secs(5);
    if let Some((at, sides)) = CACHE.with(|c| c.get()) {
        if at.elapsed() < FRESH {
            return sides;
        }
    }
    let sides = read_button_sides();
    CACHE.with(|c| c.set(Some((std::time::Instant::now(), sides))));
    sides
}

fn read_button_sides() -> (usize, usize) {
    use gtk::gio;
    use gtk::gio::prelude::SettingsExt;
    let schema = "org.gnome.desktop.wm.preferences";
    let known = gio::SettingsSchemaSource::default().and_then(|s| s.lookup(schema, true)).is_some();
    if !known {
        return (0, 3);
    }
    let layout = gio::Settings::new(schema).string("button-layout");
    let mut sides = layout.splitn(2, ':');
    let count = |s: &str| s.split(',').filter(|b| matches!(*b, "minimize" | "maximize" | "close")).count();
    (count(sides.next().unwrap_or("")), count(sides.next().unwrap_or("")))
}

/// Width and height, in logical px, given to one window button's area and to the title bar.
const BUTTON_W: i32 = 38;
const TITLEBAR_H: i32 = 46;
/// A window whose top edge is lower than this is not up against the top of the screen.
const TOP_EDGE_REACH: i32 = 64;

/// The areas where a window's buttons are, from its frame `(x, y, w, h)` in root
/// coordinates: `(left_buttons, right_buttons)` of them at each end of its top edge.
/// Pure, so it can be tested; empty for a window that is not at the top of the screen.
pub fn button_areas(frame: (i32, i32, i32, i32), left: usize, right: usize) -> Vec<[i32; 4]> {
    let (x, y, w, _) = frame;
    if y > TOP_EDGE_REACH || y < -TITLEBAR_H {
        return Vec::new();
    }
    let mut out = Vec::new();
    if right > 0 {
        let width = BUTTON_W * right as i32;
        out.push([x + w - width, y, width, TITLEBAR_H]);
    }
    if left > 0 {
        out.push([x, y, BUTTON_W * left as i32, TITLEBAR_H]);
    }
    out
}

/// The invisible shadow a GTK window draws around itself, as `(left, right, top, bottom)`:
/// its frame is that much bigger than what is seen, and its buttons sit inside the visible part.
fn shadow_extents(w: &gtk::gdk::Window) -> (i32, i32, i32, i32) {
    let atom = gtk::gdk::Atom::intern("_GTK_FRAME_EXTENTS");
    let cardinal = gtk::gdk::Atom::intern("CARDINAL");
    // `length` is in bytes: four 32-bit values.
    let Some((_, _, data)) = gtk::gdk::property_get(w, &atom, &cardinal, 0, 16, 0) else { return (0, 0, 0, 0) };
    // GDK hands 32-bit properties back as C longs (8 bytes each on 64-bit).
    let values: Vec<i32> = match data.len() {
        32 => data.chunks_exact(8).map(|c| u64::from_ne_bytes(c.try_into().unwrap()) as i32).collect(),
        16 => data.chunks_exact(4).map(|c| u32::from_ne_bytes(c.try_into().unwrap()) as i32).collect(),
        _ => return (0, 0, 0, 0),
    };
    (values[0].max(0), values[1].max(0), values[2].max(0), values[3].max(0))
}

/// What the window manager says about one top-level window: everything the island needs to know to decide
/// whether the buttons of its title bar are really there to be clicked.
#[derive(Clone, Debug, PartialEq)]
pub struct WinInfo {
    /// What is seen of it (its frame without the invisible shadow), in root coordinates.
    pub visible: (i32, i32, i32, i32),
    /// A window with a title bar of its own (`_NET_WM_WINDOW_TYPE` NORMAL or DIALOG, or none set).
    pub normal: bool,
    /// The process behind it (`_NET_WM_PID`).
    pub pid: Option<u32>,
    /// The workspace it is on (`_NET_WM_DESKTOP`; `ALL_DESKTOPS` for every one).
    pub desktop: Option<u32>,
    /// Iconified, full screen, or otherwise not on screen.
    pub hidden: bool,
}

/// `_NET_WM_DESKTOP` of a window that is on every workspace.
pub const ALL_DESKTOPS: u32 = 0xFFFF_FFFF;

/// Is a window with these `_NET_WM_WINDOW_TYPE` names one that has a title bar? No type set is an ordinary window.
pub fn has_titlebar(types: &[String]) -> bool {
    types.is_empty()
        || types
            .iter()
            .any(|t| matches!(t.as_str(), "_NET_WM_WINDOW_TYPE_NORMAL" | "_NET_WM_WINDOW_TYPE_DIALOG"))
}

/// Does `inner` lie entirely inside `outer`?
fn within_rect(inner: [i32; 4], outer: (i32, i32, i32, i32)) -> bool {
    inner[0] >= outer.0
        && inner[1] >= outer.1
        && inner[0] + inner[2] <= outer.0 + outer.2
        && inner[1] + inner[3] <= outer.1 + outer.3
}

/// The button areas, in root coordinates, that can be clicked: those of the windows on this workspace that
/// are up against the top of the screen and not covered by a window above them. `stack` is bottom-most first
/// (as `window_stack` gives it); `own_pid` is Coucou itself, whose own windows (the pill, the Coucou window,
/// Mochi on the desktop) are not someone's title bar. Pure, so it can be tested.
pub fn clickable_button_areas(
    stack: &[WinInfo],
    current_desktop: Option<u32>,
    own_pid: u32,
    left: usize,
    right: usize,
) -> Vec<[i32; 4]> {
    let on_this_desktop = |w: &WinInfo| match (w.desktop, current_desktop) {
        (Some(d), Some(c)) => d == ALL_DESKTOPS || d == c,
        _ => true, // not told: shown rather than lost
    };
    let counts = |w: &WinInfo| w.normal && !w.hidden && w.pid != Some(own_pid) && on_this_desktop(w);
    let mut covering: Vec<(i32, i32, i32, i32)> = Vec::new();
    let mut out = Vec::new();
    // From the top down: what is already collected hides what lies under it.
    for w in stack.iter().rev().filter(|w| counts(w)) {
        for a in button_areas(w.visible, left, right) {
            if !covering.iter().any(|c| within_rect(a, *c)) {
                out.push(a);
            }
        }
        covering.push(w.visible);
    }
    out
}

/// The workspace in use (`_NET_CURRENT_DESKTOP` of the root window), if the window manager says.
fn current_desktop(screen: &gtk::gdk::Screen) -> Option<u32> {
    cardinal(&screen.root_window()?, "_NET_CURRENT_DESKTOP")
}

/// A one-number property of a window (`CARDINAL`).
fn cardinal(w: &gtk::gdk::Window, name: &str) -> Option<u32> {
    let atom = gtk::gdk::Atom::intern(name);
    let cardinal = gtk::gdk::Atom::intern("CARDINAL");
    let (_, _, data) = gtk::gdk::property_get(w, &atom, &cardinal, 0, 8, 0)?;
    match data.len() {
        8 => Some(u64::from_ne_bytes(data[..8].try_into().ok()?) as u32),
        4 => Some(u32::from_ne_bytes(data[..4].try_into().ok()?)),
        _ => None,
    }
}

/// The names in the window's `_NET_WM_WINDOW_TYPE`. GDK hands an `ATOM` property back already as GdkAtoms.
fn window_types(w: &gtk::gdk::Window) -> Vec<String> {
    let atom = gtk::gdk::Atom::intern("_NET_WM_WINDOW_TYPE");
    let kind = gtk::gdk::Atom::intern("ATOM");
    let Some((_, _, data)) = gtk::gdk::property_get(w, &atom, &kind, 0, 64, 0) else { return Vec::new() };
    let word = std::mem::size_of::<usize>();
    let known = [
        "_NET_WM_WINDOW_TYPE_NORMAL",
        "_NET_WM_WINDOW_TYPE_DIALOG",
        "_NET_WM_WINDOW_TYPE_DOCK",
        "_NET_WM_WINDOW_TYPE_DESKTOP",
        "_NET_WM_WINDOW_TYPE_UTILITY",
        "_NET_WM_WINDOW_TYPE_TOOLBAR",
        "_NET_WM_WINDOW_TYPE_MENU",
        "_NET_WM_WINDOW_TYPE_SPLASH",
        "_NET_WM_WINDOW_TYPE_NOTIFICATION",
        "_NET_WM_WINDOW_TYPE_TOOLTIP",
        "_NET_WM_WINDOW_TYPE_POPUP_MENU",
        "_NET_WM_WINDOW_TYPE_DROPDOWN_MENU",
        "_NET_WM_WINDOW_TYPE_COMBO",
        "_NET_WM_WINDOW_TYPE_DND",
    ];
    let named: Vec<(usize, &str)> = known.iter().map(|n| (gtk::gdk::Atom::intern(n).value(), *n)).collect();
    data.chunks_exact(word)
        .map(|c| usize::from_ne_bytes(c.try_into().unwrap_or([0; std::mem::size_of::<usize>()])))
        .map(|v| named.iter().find(|(a, _)| *a == v).map_or_else(|| "other".to_string(), |(_, n)| (*n).to_string()))
        .collect()
}

/// Every top-level window on screen as the window manager describes it, bottom-most first.
fn window_infos(screen: &gtk::gdk::Screen, own: Option<&gtk::gdk::Window>) -> Vec<WinInfo> {
    use gtk::gdk::WindowState;
    screen
        .window_stack()
        .into_iter()
        .filter(|w| own.map_or(true, |o| w != o))
        .map(|w| {
            let f = w.frame_extents();
            let (sl, sr, st, sb) = shadow_extents(&w);
            WinInfo {
                visible: (f.x() + sl, f.y() + st, f.width() - sl - sr, f.height() - st - sb),
                normal: has_titlebar(&window_types(&w)),
                pid: cardinal(&w, "_NET_WM_PID"),
                desktop: cardinal(&w, "_NET_WM_DESKTOP"),
                hidden: !w.is_visible() || w.state().intersects(WindowState::ICONIFIED | WindowState::FULLSCREEN),
            }
        })
        .collect()
}

/// The button areas of every window up against the top of the screen, in the coordinates of
/// a window at `(ox, oy)` of size `(own_w, own_h)`; only those that reach into it.
fn collect_zones(own: Option<&gtk::gdk::Window>, ox: i32, oy: i32, own_w: i32, own_h: i32) -> Vec<[f64; 4]> {
    let Some(screen) = gtk::gdk::Screen::default() else { return Vec::new() };
    let (left, right) = button_sides();
    let stack = window_infos(&screen, own);
    clickable_button_areas(&stack, current_desktop(&screen), std::process::id(), left, right)
        .into_iter()
        .filter_map(|a| {
            let (zx, zy) = (a[0] - ox, a[1] - oy);
            (zx + a[2] > 0 && zx < own_w && zy + a[3] > 0 && zy < own_h)
                .then(|| [f64::from(zx), f64::from(zy), f64::from(a[2]), f64::from(a[3])])
        })
        .collect()
}

/// Where the buttons of the windows up against the top of the screen are, in the island
/// window's own coordinates (only those that reach into it). Must run on the main thread.
pub fn window_button_zones(win: &WebviewWindow) -> Vec<[f64; 4]> {
    // Debug builds only (compiled out of releases): the test harness has no window manager,
    // so it states the areas itself, "x,y,w,h;x,y,w,h" in the island window's coordinates.
    #[cfg(debug_assertions)]
    if let Ok(list) = std::env::var("COUCOU_TEST_ZONES") {
        return list
            .split(';')
            .filter_map(|z| {
                let n: Vec<f64> = z.split(',').filter_map(|v| v.trim().parse().ok()).collect();
                (n.len() == 4).then(|| [n[0], n[1], n[2], n[3]])
            })
            .collect();
    }
    let Ok(gw) = win.gtk_window() else { return Vec::new() };
    let Some(own) = gw.window() else { return Vec::new() };
    let (_, ox, oy) = own.origin();
    collect_zones(Some(&own), ox, oy, own.width(), own.height())
}

// ── Island window ─────────────────────────────────────────────────────────────

/// The few gtk-layer-shell calls we need, straight from the C library.
mod layer {
    use gtk::ffi::GtkWindow;
    use std::os::raw::{c_char, c_int};

    pub const LAYER_TOP: c_int = 2;
    pub const LAYER_OVERLAY: c_int = 3;
    pub const EDGE_LEFT: c_int = 0;
    pub const EDGE_RIGHT: c_int = 1;
    pub const EDGE_TOP: c_int = 2;
    pub const EDGE_BOTTOM: c_int = 3;
    pub const KEYBOARD_NONE: c_int = 0;
    pub const KEYBOARD_ON_DEMAND: c_int = 2;

    #[link(name = "gtk-layer-shell")]
    extern "C" {
        pub fn gtk_layer_is_supported() -> c_int;
        pub fn gtk_layer_init_for_window(window: *mut GtkWindow);
        pub fn gtk_layer_set_namespace(window: *mut GtkWindow, name_space: *const c_char);
        pub fn gtk_layer_set_layer(window: *mut GtkWindow, layer: c_int);
        pub fn gtk_layer_set_anchor(window: *mut GtkWindow, edge: c_int, anchor: c_int);
        pub fn gtk_layer_set_margin(window: *mut GtkWindow, edge: c_int, margin: c_int);
        pub fn gtk_layer_set_monitor(window: *mut GtkWindow, monitor: *mut gtk::gdk::ffi::GdkMonitor);
        pub fn gtk_layer_set_exclusive_zone(window: *mut GtkWindow, zone: c_int);
        pub fn gtk_layer_set_keyboard_mode(window: *mut GtkWindow, mode: c_int);
    }
}

/// COUCOU_LAYER_SHELL=0 is the way out on a compositor where it misbehaves.
fn layer_shell_wanted() -> bool {
    std::env::var("COUCOU_LAYER_SHELL").map(|v| v != "0").unwrap_or(true)
}

/// True once the island window is a layer-shell surface.
static LAYER_SURFACE: AtomicBool = AtomicBool::new(false);

/// The input region last asked for, re-applied whenever the window is mapped:
/// GTK resets it to the whole window on map. Until the page reports the island
/// shape it is empty, so nothing takes the mouse.
type Region = Option<(f64, f64, f64, f64)>;
static INPUT_REGION: Mutex<Region> = Mutex::new(Some((0.0, 0.0, 0.0, 0.0)));

fn gtk_window_ptr(win: &gtk::ApplicationWindow) -> *mut gtk::ffi::GtkWindow {
    let w: &gtk::Window = win.upcast_ref();
    w.to_glib_none().0
}

/// WebKitGTK has no competing drop target to remove.
pub fn unblock_webview_drops(_app: &AppHandle) {}

/// Turns the island into an overlay surface on the top edge that never takes
/// the keyboard. Must run before the window is first shown: a layer surface
/// cannot be made out of a window the compositor already knows.
///
/// Without layer-shell (GNOME, X11, or COUCOU_LAYER_SHELL=0) the window stays
/// an ordinary always-on-top window that refuses focus; where it lands is then
/// up to the window manager.
pub fn make_non_activating(win: &WebviewWindow) {
    let Ok(gw) = win.gtk_window() else { return };
    // The pointer watch reports the pointer leaving on every kind of window:
    // a regular one (GNOME, X11) misses `mouseout` just as a layer surface does.
    ISLAND_GTK.with(|cell| *cell.borrow_mut() = Some(gw.clone()));
    watch_pointer_leave(win);
    let wanted = layer_shell_wanted();
    let supported = unsafe { layer::gtk_layer_is_supported() } != 0;
    if !wanted || !supported || gw.is_realized() {
        let why = if !wanted {
            "COUCOU_LAYER_SHELL=0"
        } else if supported {
            "window already shown"
        } else {
            "compositor has no layer-shell"
        };
        crate::log::line(format!("island is a regular window ({why})"));
        gw.set_accept_focus(false);
        place_without_layer_shell(&gw);
        return;
    }
    // tao gives undecorated Wayland windows an empty titlebar to force
    // client-side decorations. A layer surface has none, and a client-decorated
    // GtkWindow recomputes its own input region (shadow margins included) on
    // every map, over ours.
    gw.set_titlebar(None::<&gtk::Widget>);
    let ptr = gtk_window_ptr(&gw);
    unsafe {
        layer::gtk_layer_init_for_window(ptr);
        layer::gtk_layer_set_namespace(ptr, c"coucou".as_ptr());
        layer::gtk_layer_set_layer(ptr, layer::LAYER_OVERLAY);
        // Top edge only: the compositor centres the surface horizontally.
        layer::gtk_layer_set_anchor(ptr, layer::EDGE_TOP, 1);
        // -1: sit right against the screen edge, over any top panel, the way
        // the Mac island sits in the notch.
        layer::gtk_layer_set_exclusive_zone(ptr, -1);
        layer::gtk_layer_set_keyboard_mode(ptr, layer::KEYBOARD_NONE);
    }
    // WebKitGTK in a freshly mapped layer surface never paints its first frame
    // (seen on COSMIC, and reproduced with a bare GTK window + WebKitGTK, no
    // Tauri involved): the surface stays empty. Unmapping and mapping it once,
    // right after the first map, gets it drawing for good.
    let remapped = std::cell::Cell::new(false);
    gw.connect_map_event(move |w, _| {
        apply_input_region(w, *INPUT_REGION.lock().unwrap());
        if !remapped.replace(true) {
            let w = w.clone();
            gtk::glib::idle_add_local_once(move || {
                w.hide();
                w.show_all();
                apply_input_region(&w, *INPUT_REGION.lock().unwrap());
            });
        }
        gtk::glib::Propagation::Proceed
    });
    LAYER_SURFACE.store(true, Ordering::Relaxed);
    crate::log::line("island is a layer-shell overlay");
}

/// GNOME (X11) has no layer-shell, and its top panel is drawn above every normal
/// window — "always on top" included — while mutter places a normal window where it
/// likes (seen on Zorin 18 / Pop Shell: 36 px down and off-centre). So how the
/// island is mapped is chosen with `COUCOU_X11_MODE`:
///   * unset: upstream's choice — a Dock, kept above everything and left alone by
///     "show desktop" (`COUCOU_DOCK=0`: a utility window instead);
///   * `normal`: an ordinary window (what this fork did before taking upstream's);
///   * `utility`: the same, typed as a utility window — mutter keeps it centred,
///     directly under the panel;
///   * `or`: an override-redirect window. mutter never manages it (no placement,
///     no tiling, no focus handling) and draws it in the group above the Shell's
///     own UI, so it can sit on the very edge of the screen over the panel.
/// The window type the island rests in for a `COUCOU_X11_MODE`.
fn resting_hint(mode: &str) -> Option<gtk::gdk::WindowTypeHint> {
    use gtk::gdk::WindowTypeHint as T;
    Some(match mode {
        "utility" => T::Utility,
        "notification" => T::Notification,
        "dock" => T::Dock,
        "splash" => T::Splashscreen,
        "popup" => T::PopupMenu,
        "tooltip" => T::Tooltip,
        _ => return None,
    })
}

fn place_without_layer_shell(gw: &gtk::ApplicationWindow) {
    // The island is an overlay, not a window to switch to: keep it out of the
    // overview (Super), Alt+Tab and the dock whatever the window type ends up being.
    gw.set_skip_taskbar_hint(true);
    gw.set_skip_pager_hint(true);
    let mode = std::env::var("COUCOU_X11_MODE").unwrap_or_default();
    match mode.as_str() {
        // GNOME's overview (Super) lists utility windows but none of the other types: the
        // island is an overlay, not a window to switch to.
        "utility" | "notification" | "dock" | "splash" | "popup" | "tooltip" => {
            if let Some(hint) = resting_hint(&mode) {
                gw.set_type_hint(hint)
            }
        }
        "or" => {
            gw.realize();
            if let Some(window) = gw.window() {
                window.set_override_redirect(true);
            }
        }
        "normal" => {}
        _ => {
            if !gw.is_realized() {
                // On X11 a Dock is kept above everything and is the only kind of
                // window, besides the desktop, that "show desktop" leaves alone.
                // COUCOU_DOCK=0 falls back to a utility window.
                let dock = std::env::var("COUCOU_DOCK").map(|v| v != "0").unwrap_or(true);
                gw.set_type_hint(if dock {
                    gtk::gdk::WindowTypeHint::Dock
                } else {
                    gtk::gdk::WindowTypeHint::Utility
                });
            }
            gw.set_keep_above(true);
        }
    }
    if !mode.is_empty() {
        crate::log::line(format!("X11 window mode: {mode}"));
    }
}

/// The window that had the keyboard before the island took it (an X11 window id, as text).
static PREVIOUS_ACTIVE: Mutex<Option<String>> = Mutex::new(None);

/// What `xdotool` prints for `args`, trimmed; None without xdotool or on any failure.
fn xdotool(args: &[&str]) -> Option<String> {
    let out = Command::new("xdotool").args(args).stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string()).filter(|s| !s.is_empty())
}

/// Remembers who has the keyboard before the island takes it, and gives it back when the island
/// lets go — but only if the island is still the active window then: if the user went elsewhere in
/// the meantime they are left there. Needs `xdotool`; without it nothing is remembered or restored.
fn keep_keyboard_owner(activating: bool) {
    if activating {
        let active = xdotool(&["getactivewindow"]);
        let mine = active.as_deref().and_then(|id| xdotool(&["getwindowpid", id])).and_then(|p| p.parse::<u32>().ok());
        // Already the island's own window: keep the owner remembered from before.
        if mine != Some(std::process::id()) {
            *PREVIOUS_ACTIVE.lock().unwrap() = active;
        }
    } else if let Some(prev) = PREVIOUS_ACTIVE.lock().unwrap().take() {
        let active = xdotool(&["getactivewindow"]);
        let mine = active.as_deref().and_then(|id| xdotool(&["getwindowpid", id])).and_then(|p| p.parse::<u32>().ok());
        if mine == Some(std::process::id()) {
            let _ = xdotool(&["windowactivate", &prev]);
        }
    }
}

thread_local! {
    /// The island's GTK window, for the pointer watch (GTK main thread only).
    static ISLAND_GTK: std::cell::RefCell<Option<gtk::ApplicationWindow>> =
        const { std::cell::RefCell::new(None) };
}

/// Reports the pointer leaving the island, which the page cannot see by itself.
///
/// On a layer surface WebKitGTK does not send `mouseout` when the pointer leaves
/// for good, and GTK's leave-notify fires spuriously while the pointer is still
/// over the island (each followed by a re-enter), so neither tells "really
/// gone". GDK's pointer focus does: it follows the compositor's wl_pointer
/// enter/leave. It is read every 100 ms while the island is shown; both edges go
/// to the page as `pointer-inside`, and leaving also emits the far-away cursor
/// the page uses. While the island is hidden the timer is removed altogether
/// (set_pointer_watch), so a hidden island costs nothing.
static POINTER_WIN: std::sync::OnceLock<WebviewWindow> = std::sync::OnceLock::new();
static WATCH_WANTED: AtomicBool = AtomicBool::new(true);
static WATCH_RUNNING: AtomicBool = AtomicBool::new(false);

fn watch_pointer_leave(win: &WebviewWindow) {
    let _ = POINTER_WIN.set(win.clone());
    start_pointer_watch();
}

/// Starts or parks the pointer watch. Called with the island's collapsed state;
/// safe from any thread (the timer itself lives on the GTK main loop).
pub fn set_pointer_watch(active: bool) {
    WATCH_WANTED.store(active, Ordering::Relaxed);
    if active {
        gtk::glib::MainContext::default().invoke(start_pointer_watch);
    }
}

/// Main thread only.
fn start_pointer_watch() {
    if !WATCH_WANTED.load(Ordering::Relaxed) || WATCH_RUNNING.swap(true, Ordering::Relaxed) {
        return;
    }
    use tauri::Emitter;
    let Some(win) = POINTER_WIN.get().cloned() else {
        WATCH_RUNNING.store(false, Ordering::Relaxed);
        return;
    };
    let inside = std::cell::Cell::new(false);
    gtk::glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        if !WATCH_WANTED.load(Ordering::Relaxed) {
            WATCH_RUNNING.store(false, Ordering::Relaxed);
            return gtk::glib::ControlFlow::Break;
        }
        let now_inside = ISLAND_GTK
            .with(|cell| {
                let gw = cell.borrow().clone()?;
                let ours = gw.window()?;
                let pointer = gtk::gdk::Display::default()?.default_seat()?.pointer()?;
                let (under, _, _) = pointer.window_at_position();
                Some(under?.toplevel() == ours.toplevel())
            })
            .unwrap_or(false);
        if inside.replace(now_inside) != now_inside {
            // The page gates its own (late, sometimes stale) mouse events on
            // this state, so a mousemove queued before the pointer left cannot
            // pull the island back to "inside".
            let _ = win.emit("pointer-inside", now_inside);
            if !now_inside {
                let _ = win.emit("cursor", crate::island::CursorPayload { x: -10_000.0, y: -10_000.0 });
            }
        }
        gtk::glib::ControlFlow::Continue
    });
}

/// Puts the layer surface on the display whose logical origin is (`x`, `y`).
///
/// A layer surface ignores `set_position`: without an explicit output the
/// compositor maps it on whichever display has focus at that moment, so on a
/// multi-monitor Hyprland or Sway desktop the island wandered from one screen
/// to the other every time it was remapped. A no-op for a regular window, which
/// `set_position` already places.
pub fn pin_to_monitor(win: &WebviewWindow, x: i32, y: i32) {
    if !LAYER_SURFACE.load(Ordering::Relaxed) {
        return;
    }
    let Ok(gw) = win.gtk_window() else { return };
    let display = gw.display();
    let monitor = (0..display.n_monitors())
        .filter_map(|i| display.monitor(i))
        .find(|m| {
            let g = m.geometry();
            g.x() == x && g.y() == y
        });
    let Some(monitor) = monitor else { return };
    let mon_ptr: *mut gtk::gdk::ffi::GdkMonitor = monitor.to_glib_none().0;
    unsafe { layer::gtk_layer_set_monitor(gtk_window_ptr(&gw), mon_ptr) };
}

/// Temporarily allow keyboard focus so a text field inside the island can be
/// typed in.
pub fn set_activating(win: &WebviewWindow, activating: bool) {
    let Ok(gw) = win.gtk_window() else { return };
    if !LAYER_SURFACE.load(Ordering::Relaxed) {
        keep_keyboard_owner(activating);
    }
    // The island is created `focusable: false` (tauri.linux.conf.json), so GTK
    // refuses focus until we say otherwise — on a layer surface too.
    gw.set_accept_focus(activating);
    // A window manager may refuse the keyboard to the overlay window types (the ones GNOME's
    // overview does not list): while someone types, the island is an ordinary utility window,
    // and it goes back to its resting type when they are done.
    if !LAYER_SURFACE.load(Ordering::Relaxed) {
        let mode = std::env::var("COUCOU_X11_MODE").unwrap_or_default();
        if let (Some(window), Some(resting)) = (gw.window(), resting_hint(&mode)) {
            window.set_type_hint(if activating { gtk::gdk::WindowTypeHint::Utility } else { resting });
        }
    }
    if LAYER_SURFACE.load(Ordering::Relaxed) {
        let mode = if activating { layer::KEYBOARD_ON_DEMAND } else { layer::KEYBOARD_NONE };
        unsafe { layer::gtk_layer_set_keyboard_mode(gtk_window_ptr(&gw), mode) };
    }
}

/// Only this rectangle (window-logical pixels) takes the mouse; `None` means
/// the whole window does. Everything outside goes to the window underneath.
pub fn set_input_region(win: &WebviewWindow, rect: Region) {
    *INPUT_REGION.lock().unwrap() = rect;
    let Ok(gw) = win.gtk_window() else { return };
    apply_input_region(&gw, rect);
}

fn apply_input_region(gw: &impl IsA<gtk::Widget>, rect: Region) {
    match rect {
        None => gw.input_shape_combine_region(None),
        Some((x, y, w, h)) => {
            let Some(gdk_window) = gw.window() else { return };
            let region = gtk::cairo::Region::create_rectangle(&gtk::cairo::RectangleInt::new(
                x.floor() as i32,
                y.floor() as i32,
                w.ceil().max(0.0) as i32,
                h.ceil().max(0.0) as i32,
            ));
            gdk_window.input_shape_combine_region(&region, 0, 0);
        }
    }
}

// ── Global shortcuts ──────────────────────────────────────────────────────────

/// Global shortcuts are X11 key grabs. A Wayland session has no such thing: a
/// grab made through XWayland only sees keys typed into other X11 windows, so
/// it would look registered and never fire. On Wayland the shortcuts go to the
/// desktop through the XDG GlobalShortcuts portal instead (portal.rs), and
/// where there is none Settings explains how to bind `coucou --shortcut <id>`
/// in the desktop's own keyboard settings.
pub fn global_shortcuts_blocked() -> Option<&'static str> {
    let set = |var: &str| std::env::var_os(var).is_some_and(|v| !v.is_empty());
    let wayland = set("WAYLAND_DISPLAY")
        || std::env::var("XDG_SESSION_TYPE").is_ok_and(|v| v.eq_ignore_ascii_case("wayland"));
    if wayland {
        Some("wayland")
    } else if !set("DISPLAY") {
        Some("no-display")
    } else {
        None
    }
}

/// On X11, AltGr is a modifier of its own (ISO_Level3_Shift): Ctrl+Alt+key
/// never stands for it, so no combination takes a character away.
pub fn ctrl_alt_types(_vk: u16, _shift: bool) -> Option<String> {
    None
}

// ── Desktop Mochi window ──────────────────────────────────────────────────────
//
// Where the window may go decides everything here:
//   * X11 places any window where it is asked: an ordinary always-on-top
//     window, moved with set_position;
//   * a layer-shell compositor won't move a toplevel, but anchors a layer
//     surface wherever its margins say, on the island's display;
//   * GNOME on Wayland has neither, so the feature is off there.
// In every case the input region is Mochi's body, so clicks anywhere else go
// to the desktop underneath without any cursor polling.

/// Which of the three applies. Must run on the GTK main thread.
pub fn desktop_mode() -> super::DesktopMode {
    use gtk::glib::prelude::ObjectExt;
    let wayland = gtk::gdk::Display::default()
        .map(|d| d.type_().name().contains("Wayland"))
        .unwrap_or(false);
    let layer = layer_shell_wanted() && unsafe { layer::gtk_layer_is_supported() } != 0;
    if layer {
        super::DesktopMode::Layer
    } else if !wayland {
        super::DesktopMode::Window
    } else {
        super::DesktopMode::Off
    }
}

/// The input shape last asked for the desktop Mochi, re-applied on every map.
static MOCHI_SHAPE: Mutex<super::MouseShape> = Mutex::new(super::MouseShape::Empty);

/// Sets the window up for `mode` before it is ever shown. False means it can't
/// be used (a layer surface can't be made from a window already shown).
pub fn prepare_desktop_window(win: &WebviewWindow, mode: super::DesktopMode) -> bool {
    let Ok(gw) = win.gtk_window() else { return false };
    gw.set_accept_focus(false);
    match mode {
        super::DesktopMode::Layer => {
            if gw.is_realized() {
                crate::log::line("desktop Mochi: window already shown, no layer surface");
                return false;
            }
            gw.set_titlebar(None::<&gtk::Widget>);
            let ptr = gtk_window_ptr(&gw);
            unsafe {
                layer::gtk_layer_init_for_window(ptr);
                layer::gtk_layer_set_namespace(ptr, c"coucou-mochi".as_ptr());
                // Above windows, below fullscreen video — like a floating panel.
                layer::gtk_layer_set_layer(ptr, layer::LAYER_TOP);
                layer::gtk_layer_set_anchor(ptr, layer::EDGE_TOP, 1);
                layer::gtk_layer_set_anchor(ptr, layer::EDGE_LEFT, 1);
                // -1: margins count from the display's corner, panels or not,
                // the same origin the island uses.
                layer::gtk_layer_set_exclusive_zone(ptr, -1);
                layer::gtk_layer_set_keyboard_mode(ptr, layer::KEYBOARD_NONE);
            }
            // Same first-frame problem as the island (see make_non_activating).
            let remapped = std::cell::Cell::new(false);
            gw.connect_map_event(move |w, _| {
                apply_mouse_shape(w, *MOCHI_SHAPE.lock().unwrap());
                if !remapped.replace(true) {
                    let w = w.clone();
                    gtk::glib::idle_add_local_once(move || {
                        w.hide();
                        w.show_all();
                        apply_mouse_shape(&w, *MOCHI_SHAPE.lock().unwrap());
                    });
                }
                gtk::glib::Propagation::Proceed
            });
            true
        }
        super::DesktopMode::Window => {
            gw.connect_map_event(|w, _| {
                apply_mouse_shape(w, *MOCHI_SHAPE.lock().unwrap());
                gtk::glib::Propagation::Proceed
            });
            true
        }
        _ => false,
    }
}

/// Which part of the desktop Mochi takes the mouse. Main thread.
pub fn set_desktop_shape(win: &WebviewWindow, shape: super::MouseShape) {
    *MOCHI_SHAPE.lock().unwrap() = shape;
    let Ok(gw) = win.gtk_window() else { return };
    apply_mouse_shape(&gw, shape);
}

fn apply_mouse_shape(gw: &impl IsA<gtk::Widget>, shape: super::MouseShape) {
    use gtk::cairo::{RectangleInt, Region};
    if shape == super::MouseShape::Whole {
        gw.input_shape_combine_region(None);
        return;
    }
    let Some(gdk_window) = gw.window() else { return };
    match shape {
        super::MouseShape::Whole => {}
        super::MouseShape::Empty => {
            let region = Region::create_rectangle(&RectangleInt::new(0, 0, 0, 0));
            gdk_window.input_shape_combine_region(&region, 0, 0);
        }
        super::MouseShape::Disc { cx, cy, r } => {
            // A region is made of rectangles: stack 2 px rows into a disc.
            let region = Region::create_rectangle(&RectangleInt::new(0, 0, 0, 0));
            let mut y = (cy - r).floor();
            while y < cy + r {
                let mid = y + 1.0 - cy;
                let half = (r * r - mid * mid).max(0.0).sqrt();
                let row = RectangleInt::new(
                    (cx - half).floor() as i32,
                    y as i32,
                    (2.0 * half).ceil() as i32,
                    2,
                );
                let _ = region.union_rectangle(&row);
                y += 2.0;
            }
            gdk_window.input_shape_combine_region(&region, 0, 0);
        }
    }
}

/// Layer surface only: puts the top-left corner at (x, y), logical pixels from
/// the display's corner. Main thread.
pub fn set_layer_margins(win: &WebviewWindow, x: f64, y: f64) {
    let Ok(gw) = win.gtk_window() else { return };
    let ptr = gtk_window_ptr(&gw);
    unsafe {
        layer::gtk_layer_set_margin(ptr, layer::EDGE_LEFT, x.round() as i32);
        layer::gtk_layer_set_margin(ptr, layer::EDGE_TOP, y.round() as i32);
    }
}

/// Layer surface only: `on` stretches it over the whole display — how a drag
/// gets exact pointer positions on Wayland, where a surface that moves under
/// the pointer can't tell where it is — and `off` folds it back to Mochi's
/// size in the corner. Main thread.
pub fn set_layer_overlay(win: &WebviewWindow, on: bool) {
    let Ok(gw) = win.gtk_window() else { return };
    let ptr = gtk_window_ptr(&gw);
    unsafe {
        layer::gtk_layer_set_anchor(ptr, layer::EDGE_RIGHT, on as i32);
        layer::gtk_layer_set_anchor(ptr, layer::EDGE_BOTTOM, on as i32);
        if on {
            layer::gtk_layer_set_margin(ptr, layer::EDGE_LEFT, 0);
            layer::gtk_layer_set_margin(ptr, layer::EDGE_TOP, 0);
        }
    }
}

/// Layer surface only: puts the desktop Mochi on the island's display and
/// returns that display's logical size. Main thread.
pub fn layer_display(island: &WebviewWindow, mochi: &WebviewWindow) -> Option<(f64, f64)> {
    let island_gw = island.gtk_window().ok()?;
    let display = gtk::gdk::Display::default()?;
    let monitor = island_gw
        .window()
        .and_then(|w| display.monitor_at_window(&w))
        .or_else(|| display.primary_monitor())
        .or_else(|| display.monitor(0))?;
    if let Ok(gw) = mochi.gtk_window() {
        unsafe { layer::gtk_layer_set_monitor(gtk_window_ptr(&gw), monitor.to_glib_none().0) };
    }
    let g = monitor.geometry();
    Some((g.width() as f64, g.height() as f64))
}

#[cfg(test)]
mod zone_tests {
    use super::*;

    /// Reads the REAL session's windows (read-only, creates nothing): run by hand with
    /// `DISPLAY=:0 cargo test --lib real_session_probe -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_session_probe() {
        gtk::init().expect("gtk");
        let screen = gtk::gdk::Screen::default().expect("screen");
        println!("button layout (left, right): {:?}; current workspace: {:?}", button_sides(), current_desktop(&screen));
        for w in window_infos(&screen, None) {
            println!("{w:?} -> areas {:?}", button_areas(w.visible, button_sides().0, button_sides().1));
        }
        let zones = collect_zones(None, 460, 0, 1000, 320);
        println!("button areas that can be clicked and reach a 1000x320 window at x=460: {zones:?}");
    }

    fn win(visible: (i32, i32, i32, i32)) -> WinInfo {
        WinInfo { visible, normal: true, pid: Some(100), desktop: Some(0), hidden: false }
    }

    #[test]
    fn a_window_hidden_behind_another_has_no_buttons_to_reach() {
        // A tiled window under a maximized browser: its buttons are there but nobody can click them.
        let tiled = win((0, 0, 960, 1080));
        let maximized = win((0, 0, 1920, 1080));
        let areas = clickable_button_areas(&[tiled.clone(), maximized.clone()], Some(0), 1, 0, 3);
        assert_eq!(areas, vec![[1920 - 3 * BUTTON_W, 0, 3 * BUTTON_W, TITLEBAR_H]], "only the maximized window's");
        // The other way round (the tiled one on top, over a smaller one) its own buttons are there.
        let areas = clickable_button_areas(&[maximized, tiled], Some(0), 1, 0, 3);
        assert_eq!(areas.len(), 2, "the tiled one's, and the maximized one's at the far end it does not cover");
        assert!(areas.contains(&[960 - 3 * BUTTON_W, 0, 3 * BUTTON_W, TITLEBAR_H]));
    }

    #[test]
    fn a_window_beside_another_keeps_its_buttons() {
        let left = win((0, 0, 960, 1080));
        let right = win((960, 0, 960, 1080));
        assert_eq!(clickable_button_areas(&[left, right], Some(0), 1, 0, 3).len(), 2);
    }

    #[test]
    fn windows_that_are_not_there_or_not_ordinary_or_ours_do_not_count() {
        let base = win((0, 0, 960, 1080));
        let counted = |w: WinInfo| clickable_button_areas(&[w], Some(0), 1, 0, 3).len();
        assert_eq!(counted(base.clone()), 1);
        assert_eq!(counted(WinInfo { hidden: true, ..base.clone() }), 0, "minimised or full screen");
        assert_eq!(counted(WinInfo { normal: false, ..base.clone() }), 0, "a dock, the desktop icons, a popup");
        assert_eq!(counted(WinInfo { pid: Some(1), ..base.clone() }), 0, "Coucou's own windows (the pill, the Coucou window)");
        assert_eq!(counted(WinInfo { desktop: Some(2), ..base.clone() }), 0, "on another workspace");
        assert_eq!(counted(WinInfo { desktop: Some(ALL_DESKTOPS), ..base.clone() }), 1, "on every workspace");
        assert_eq!(counted(WinInfo { desktop: None, ..base.clone() }), 1, "not told: shown rather than lost");
        assert_eq!(clickable_button_areas(&[base], None, 1, 0, 3).len(), 1, "workspace unknown: shown");
    }

    #[test]
    fn a_hidden_or_foreign_window_does_not_hide_what_is_under_it() {
        // A minimised, a dock and another workspace's window lie "above" in the list but cover nothing.
        let under = win((0, 0, 960, 1080));
        let big = (0, 0, 1920, 1080);
        let covers_nothing = [
            WinInfo { hidden: true, ..win(big) },
            WinInfo { normal: false, ..win(big) },
            WinInfo { desktop: Some(3), ..win(big) },
        ];
        for over in covers_nothing {
            assert_eq!(clickable_button_areas(&[under.clone(), over], Some(0), 1, 0, 3).len(), 1);
        }
    }

    #[test]
    fn only_windows_with_a_title_bar_have_buttons() {
        let names = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(has_titlebar(&[]), "no type set is an ordinary window");
        assert!(has_titlebar(&names(&["_NET_WM_WINDOW_TYPE_NORMAL"])));
        assert!(has_titlebar(&names(&["_NET_WM_WINDOW_TYPE_DIALOG", "_NET_WM_WINDOW_TYPE_NORMAL"])));
        for other in ["_NET_WM_WINDOW_TYPE_DOCK", "_NET_WM_WINDOW_TYPE_DESKTOP", "_NET_WM_WINDOW_TYPE_NOTIFICATION", "other"] {
            assert!(!has_titlebar(&names(&[other])), "{other}");
        }
    }

    #[test]
    fn buttons_sit_at_the_end_of_the_title_bar_the_layout_names() {
        // A window tiled to the left half of a 1920 px screen, buttons on the right.
        let a = button_areas((0, 0, 960, 1080), 0, 3);
        assert_eq!(a, vec![[960 - 3 * BUTTON_W, 0, 3 * BUTTON_W, TITLEBAR_H]]);
        // Buttons on the left (macOS style) and a lone close button on the right.
        let b = button_areas((960, 0, 960, 1080), 3, 1);
        assert_eq!(b.len(), 2);
        assert_eq!(b[1], [960, 0, 3 * BUTTON_W, TITLEBAR_H]);
        // A window floating well below the top edge is not in the way.
        assert!(button_areas((100, 300, 800, 600), 0, 3).is_empty());
        // No buttons at all.
        assert!(button_areas((0, 0, 960, 1080), 0, 0).is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_private_directory_of_ours_can_hold_the_relay_socket() {
        let base = std::env::temp_dir().join(format!("coucou-rt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let dir = base.join("runtime");
        std::fs::create_dir_all(&dir).unwrap();
        let set = |mode| std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(mode)).unwrap();

        set(0o700);
        assert!(is_private_dir(&dir));

        // Readable or reachable by group or others: no.
        for open in [0o750, 0o705, 0o755, 0o777, 0o1777] {
            set(open);
            assert!(!is_private_dir(&dir), "{open:o} must be refused");
        }

        // A symlink to a private directory: no, the link itself is what we got.
        set(0o700);
        let link = base.join("link");
        std::os::unix::fs::symlink(&dir, &link).unwrap();
        assert!(!is_private_dir(&link));

        // Missing: no.
        assert!(!is_private_dir(&base.join("missing")));

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn x11_is_only_preferred_on_gnome_wayland_unless_opted_out() {
        assert!(should_prefer_x11("wayland", "ubuntu:GNOME", "", ":0"));
        assert!(!should_prefer_x11("x11", "ubuntu:GNOME", "", ":0"));
        assert!(!should_prefer_x11("wayland", "KDE", "", ":0"));
        assert!(!should_prefer_x11("wayland", "GNOME", "0", ":0"));
        // No XWayland: keep the Wayland window rather than no display at all.
        assert!(!should_prefer_x11("wayland", "GNOME", "", ""));
    }

    #[test]
    fn text_scaling_is_undone_for_the_island() {
        assert_eq!(dpi_scale_for("1.25\n").as_deref(), Some("0.8"));
        assert_eq!(dpi_scale_for("1.5").as_deref(), Some("0.6667"));
        assert_eq!(dpi_scale_for("1.0").as_deref(), None);
        assert_eq!(dpi_scale_for("").as_deref(), None);
        assert_eq!(dpi_scale_for("nonsense").as_deref(), None);
        assert_eq!(dpi_scale_for("40").as_deref(), None);
    }

    #[test]
    fn private_dirs_are_closed_to_everyone_else() {
        use std::os::unix::fs::MetadataExt;
        let dir = std::env::temp_dir().join(format!("coucou-priv-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
        ensure_private_dir(&dir).unwrap();
        assert_eq!(std::fs::metadata(&dir).unwrap().mode() & 0o777, 0o700);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_pictures_folder_comes_from_user_dirs() {
        let home = Path::new("/home/me");
        let text = "# written by xdg-user-dirs-update\nXDG_DESKTOP_DIR=\"$HOME/Desktop\"\nXDG_PICTURES_DIR=\"$HOME/Images\"\n";
        assert_eq!(xdg_user_dir(text, "XDG_PICTURES_DIR", home), Some(PathBuf::from("/home/me/Images")));
        let absolute = "XDG_PICTURES_DIR=\"/data/pics\"";
        assert_eq!(xdg_user_dir(absolute, "XDG_PICTURES_DIR", home), Some(PathBuf::from("/data/pics")));
        // "$HOME/" means the folder is disabled; relative paths are not paths.
        assert_eq!(xdg_user_dir("XDG_PICTURES_DIR=\"$HOME/\"", "XDG_PICTURES_DIR", home), None);
        assert_eq!(xdg_user_dir("XDG_PICTURES_DIR=\"pics\"", "XDG_PICTURES_DIR", home), None);
        assert_eq!(xdg_user_dir("", "XDG_PICTURES_DIR", home), None);
    }
}

// ── Flow dictation's pill ─────────────────────────────────────────────────────
//
// The pill rests at the bottom (or on the left or right edge) of the screen.
// On X11 it is an ordinary always-on-top window placed with set_position; on a
// layer-shell compositor a layer surface anchored to that edge. Either way the
// input region is the pill and its bubble, so clicks anywhere else go through.

/// The pill's input rectangles, re-applied on every map.
static PILL_SHAPE: Mutex<Vec<(f64, f64, f64, f64)>> = Mutex::new(Vec::new());

/// Sets the pill's window up before it is ever shown. Main thread.
pub fn prepare_pill_window(win: &WebviewWindow, mode: super::DesktopMode) -> bool {
    let Ok(gw) = win.gtk_window() else { return false };
    gw.set_accept_focus(false);
    let reapply = |w: &gtk::ApplicationWindow| apply_rects(w, &PILL_SHAPE.lock().unwrap());
    match mode {
        super::DesktopMode::Layer => {
            if gw.is_realized() {
                return false;
            }
            gw.set_titlebar(None::<&gtk::Widget>);
            let ptr = gtk_window_ptr(&gw);
            unsafe {
                layer::gtk_layer_init_for_window(ptr);
                layer::gtk_layer_set_namespace(ptr, c"coucou-pill".as_ptr());
                layer::gtk_layer_set_layer(ptr, layer::LAYER_OVERLAY);
                layer::gtk_layer_set_anchor(ptr, layer::EDGE_BOTTOM, 1);
                layer::gtk_layer_set_exclusive_zone(ptr, 0);
                layer::gtk_layer_set_keyboard_mode(ptr, layer::KEYBOARD_NONE);
            }
            let remapped = std::cell::Cell::new(false);
            gw.connect_map_event(move |w, _| {
                reapply(w);
                if !remapped.replace(true) {
                    let w = w.clone();
                    gtk::glib::idle_add_local_once(move || {
                        w.hide();
                        w.show_all();
                        apply_rects(&w, &PILL_SHAPE.lock().unwrap());
                    });
                }
                gtk::glib::Propagation::Proceed
            });
            true
        }
        super::DesktopMode::Window => {
            if !gw.is_realized() {
                gw.set_type_hint(gtk::gdk::WindowTypeHint::Dock);
            }
            gw.set_keep_above(true);
            gw.connect_map_event(move |w, _| {
                reapply(w);
                gtk::glib::Propagation::Proceed
            });
            true
        }
        _ => false,
    }
}

/// Layer surface only: anchors the pill to its edge, centred along it. Main thread.
pub fn set_pill_edge(win: &WebviewWindow, edge: &str) {
    let Ok(gw) = win.gtk_window() else { return };
    let ptr = gtk_window_ptr(&gw);
    unsafe {
        layer::gtk_layer_set_anchor(ptr, layer::EDGE_BOTTOM, (edge == "bottom") as i32);
        layer::gtk_layer_set_anchor(ptr, layer::EDGE_LEFT, (edge == "left") as i32);
        layer::gtk_layer_set_anchor(ptr, layer::EDGE_RIGHT, (edge == "right") as i32);
        layer::gtk_layer_set_anchor(ptr, layer::EDGE_TOP, 0);
    }
}

/// The parts of the pill's window that take the mouse. Main thread.
pub fn set_pill_input(win: &WebviewWindow, rects: Vec<(f64, f64, f64, f64)>) {
    *PILL_SHAPE.lock().unwrap() = rects.clone();
    let Ok(gw) = win.gtk_window() else { return };
    apply_rects(&gw, &rects);
}

fn apply_rects(gw: &impl IsA<gtk::Widget>, rects: &[(f64, f64, f64, f64)]) {
    use gtk::cairo::{RectangleInt, Region};
    let Some(gdk_window) = gw.window() else { return };
    let region = Region::create_rectangle(&RectangleInt::new(0, 0, 0, 0));
    for &(x, y, w, h) in rects {
        let _ = region.union_rectangle(&RectangleInt::new(
            x.floor() as i32,
            y.floor() as i32,
            w.ceil().max(0.0) as i32,
            h.ceil().max(0.0) as i32,
        ));
    }
    gdk_window.input_shape_combine_region(&region, 0, 0);
}
