// System and server vitals for the Hub's System and Server tabs.
//
// Nothing here runs on its own: the Hub asks for a sample while its panel is on screen
// and for nothing otherwise. The laptop is read from /proc and /sys; the server through
// one `ssh` call that prints the same files (and a few service states), parsed by the
// same code. The first sample after a pause has no earlier one to compare with, so its
// rates are zero.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Battery {
    pub pct: f64,
    pub charging: bool,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DiskUse {
    pub mount: String,
    pub pct: f64,
    pub size_gb: f64,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ServiceState {
    pub name: String,
    pub active: bool,
}

#[derive(Serialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Vitals {
    pub cpu: f64,
    pub mem_pct: f64,
    pub mem_used_mb: u64,
    pub mem_total_mb: u64,
    pub swap_pct: f64,
    pub load: [f64; 3],
    pub uptime_s: u64,
    pub rx_bps: f64,
    pub tx_bps: f64,
    pub disk_read_bps: f64,
    pub disk_write_bps: f64,
    pub temp_c: Option<f64>,
    pub battery: Option<Battery>,
    pub disks: Vec<DiskUse>,
    /// Server only: the services asked about, and the Docker containers (running, unhealthy).
    pub services: Vec<ServiceState>,
    pub containers: Option<(u32, u32)>,
}

/// Counters that only mean something between two samples.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Raw {
    pub cpu_total: u64,
    pub cpu_idle: u64,
    pub rx: u64,
    pub tx: u64,
    pub disk_read: u64,
    pub disk_write: u64,
}

// ── Parsers (pure) ────────────────────────────────────────────────────────────

/// (total, idle) jiffies from the first `cpu` line of /proc/stat.
pub fn parse_cpu(stat: &str) -> Option<(u64, u64)> {
    let line = stat.lines().find(|l| l.starts_with("cpu "))?;
    let n: Vec<u64> = line.split_whitespace().skip(1).filter_map(|v| v.parse().ok()).collect();
    if n.len() < 5 {
        return None;
    }
    let total: u64 = n.iter().take(8).sum();
    Some((total, n[3] + n[4]))
}

/// (mem_total_kb, mem_available_kb, swap_total_kb, swap_free_kb) from /proc/meminfo.
pub fn parse_meminfo(s: &str) -> (u64, u64, u64, u64) {
    let get = |key: &str| {
        s.lines()
            .find(|l| l.starts_with(key))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(0)
    };
    (get("MemTotal:"), get("MemAvailable:"), get("SwapTotal:"), get("SwapFree:"))
}

/// (rx_bytes, tx_bytes) summed over the real interfaces of /proc/net/dev.
pub fn parse_net(dev: &str) -> (u64, u64) {
    let (mut rx, mut tx) = (0, 0);
    for line in dev.lines().skip(2) {
        let Some((name, rest)) = line.split_once(':') else { continue };
        let name = name.trim();
        if name == "lo" || ["docker", "br-", "veth", "virbr", "tailscale"].iter().any(|p| name.starts_with(p)) {
            continue;
        }
        let f: Vec<u64> = rest.split_whitespace().filter_map(|v| v.parse().ok()).collect();
        if f.len() >= 9 {
            rx += f[0];
            tx += f[8];
        }
    }
    (rx, tx)
}

fn is_whole_disk(name: &str) -> bool {
    let digits_after = |prefix: &str| name.strip_prefix(prefix).is_some_and(|r| !r.is_empty() && r.bytes().all(|b| b.is_ascii_digit()));
    // sda, vdb, xvda… (letters only) and nvme0n1, mmcblk0 (no partition suffix)
    let letters = ["sd", "vd", "xvd", "hd"].iter().any(|p| name.strip_prefix(p).is_some_and(|r| !r.is_empty() && r.bytes().all(|b| b.is_ascii_lowercase())));
    letters
        || (name.starts_with("nvme") && name.contains('n') && !name.contains('p') && name[4..].split('n').count() == 2)
        || (name.starts_with("mmcblk") && !name.contains('p') && digits_after("mmcblk"))
}

/// (bytes_read, bytes_written) over the whole disks of /proc/diskstats (512-byte sectors).
pub fn parse_disks(s: &str) -> (u64, u64) {
    let (mut r, mut w) = (0u64, 0u64);
    for line in s.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 10 || !is_whole_disk(f[2]) {
            continue;
        }
        r += f[5].parse::<u64>().unwrap_or(0) * 512;
        w += f[9].parse::<u64>().unwrap_or(0) * 512;
    }
    (r, w)
}

pub fn parse_loadavg(s: &str) -> [f64; 3] {
    let mut it = s.split_whitespace().filter_map(|v| v.parse().ok());
    [it.next().unwrap_or(0.0), it.next().unwrap_or(0.0), it.next().unwrap_or(0.0)]
}

pub fn parse_uptime(s: &str) -> u64 {
    s.split_whitespace().next().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0) as u64
}

/// `df -P` rows (after the header) into disks: mount, use %, size in GB.
pub fn parse_df(s: &str) -> Vec<DiskUse> {
    s.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            if f.len() < 6 {
                return None;
            }
            Some(DiskUse {
                mount: f[5..].join(" "),
                pct: f[4].trim_end_matches('%').parse().ok()?,
                size_gb: f[1].parse::<f64>().ok()? / 1024.0 / 1024.0,
            })
        })
        .collect()
}

/// "name milli-degrees" lines (hwmon): the package/CPU sensor, else the hottest one.
pub fn pick_temp(lines: &str) -> Option<f64> {
    let rows: Vec<(String, f64)> = lines
        .lines()
        .filter_map(|l| {
            let (n, v) = l.rsplit_once(' ')?;
            Some((n.trim().to_string(), v.trim().parse::<f64>().ok()? / 1000.0))
        })
        .filter(|(_, c)| *c > 0.0 && *c < 150.0)
        .collect();
    rows.iter()
        .find(|(n, _)| ["coretemp", "k10temp", "cpu_thermal", "x86_pkg_temp", "zenpower"].contains(&n.as_str()))
        .or_else(|| rows.iter().max_by(|a, b| a.1.total_cmp(&b.1)))
        .map(|(_, c)| *c)
}

/// Rates from two counter readings `secs` apart; zero without an earlier one.
fn rates(prev: &Option<Raw>, now: &Raw, secs: f64) -> (f64, f64, f64, f64, f64) {
    let Some(p) = prev else { return (0.0, 0.0, 0.0, 0.0, 0.0) };
    let d = |a: u64, b: u64| a.saturating_sub(b) as f64;
    let total = d(now.cpu_total, p.cpu_total);
    let idle = d(now.cpu_idle, p.cpu_idle);
    let cpu = if total > 0.0 { ((total - idle) / total * 100.0).clamp(0.0, 100.0) } else { 0.0 };
    let per = |a: u64, b: u64| if secs > 0.0 { d(a, b) / secs } else { 0.0 };
    (cpu, per(now.rx, p.rx), per(now.tx, p.tx), per(now.disk_read, p.disk_read), per(now.disk_write, p.disk_write))
}

/// Fills `v` from the raw files' text and the previous counters.
fn fill(v: &mut Vitals, raw: &Raw, prev: &Option<Raw>, secs: f64, mem: &str, load: &str, up: &str) {
    let (cpu, rx, tx, dr, dw) = rates(prev, raw, secs);
    v.cpu = cpu;
    v.rx_bps = rx;
    v.tx_bps = tx;
    v.disk_read_bps = dr;
    v.disk_write_bps = dw;
    let (mt, ma, st, sf) = parse_meminfo(mem);
    v.mem_total_mb = mt / 1024;
    v.mem_used_mb = mt.saturating_sub(ma) / 1024;
    v.mem_pct = if mt > 0 { mt.saturating_sub(ma) as f64 / mt as f64 * 100.0 } else { 0.0 };
    v.swap_pct = if st > 0 { st.saturating_sub(sf) as f64 / st as f64 * 100.0 } else { 0.0 };
    v.load = parse_loadavg(load);
    v.uptime_s = parse_uptime(up);
}

fn raw_from(stat: &str, net: &str, disks: &str) -> Raw {
    let (cpu_total, cpu_idle) = parse_cpu(stat).unwrap_or((0, 0));
    let (rx, tx) = parse_net(net);
    let (disk_read, disk_write) = parse_disks(disks);
    Raw { cpu_total, cpu_idle, rx, tx, disk_read, disk_write }
}

// ── This computer ─────────────────────────────────────────────────────────────

struct Last {
    at: Instant,
    raw: Raw,
}

static LOCAL: Mutex<Option<Last>> = Mutex::new(None);
static REMOTE: Mutex<Option<Last>> = Mutex::new(None);

fn read(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

fn local_temp() -> Option<f64> {
    let mut lines = String::new();
    if let Ok(dirs) = std::fs::read_dir("/sys/class/hwmon") {
        for d in dirs.flatten() {
            let p = d.path();
            let name = read(&p.join("name").to_string_lossy()).trim().to_string();
            // temp1 is the package on coretemp/k10temp; the others are per core or board sensors
            let t = read(&p.join("temp1_input").to_string_lossy());
            if !t.trim().is_empty() {
                lines.push_str(&format!("{name} {}\n", t.trim()));
            }
        }
    }
    pick_temp(&lines)
}

fn local_battery() -> Option<Battery> {
    let dirs = std::fs::read_dir("/sys/class/power_supply").ok()?;
    for d in dirs.flatten() {
        let p: PathBuf = d.path();
        if !d.file_name().to_string_lossy().starts_with("BAT") {
            continue;
        }
        let pct: f64 = read(&p.join("capacity").to_string_lossy()).trim().parse().ok()?;
        let status = read(&p.join("status").to_string_lossy());
        return Some(Battery { pct, charging: matches!(status.trim(), "Charging" | "Full") });
    }
    None
}

fn root_use() -> Vec<DiskUse> {
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    let path = std::ffi::CString::new("/").unwrap();
    if unsafe { libc::statvfs(path.as_ptr(), &mut st) } != 0 || st.f_blocks == 0 {
        return Vec::new();
    }
    let size = st.f_blocks as f64 * st.f_frsize as f64;
    let free = st.f_bavail as f64 * st.f_frsize as f64;
    vec![DiskUse { mount: "/".into(), pct: ((size - free) / size * 100.0).round(), size_gb: size / 1e9 }]
}

/// One reading of this computer.
pub fn local() -> Vitals {
    let raw = raw_from(&read("/proc/stat"), &read("/proc/net/dev"), &read("/proc/diskstats"));
    let now = Instant::now();
    let mut last = LOCAL.lock().unwrap();
    // A long gap (the panel was closed) is not a rate: start over.
    let prev = last.as_ref().filter(|l| now.duration_since(l.at) < Duration::from_secs(10));
    let secs = prev.map(|l| now.duration_since(l.at).as_secs_f64()).unwrap_or(0.0);
    let mut v = Vitals::default();
    fill(&mut v, &raw, &prev.map(|l| l.raw.clone()), secs, &read("/proc/meminfo"), &read("/proc/loadavg"), &read("/proc/uptime"));
    v.temp_c = local_temp();
    v.battery = local_battery();
    v.disks = root_use();
    *last = Some(Last { at: now, raw });
    v
}

// ── The server ────────────────────────────────────────────────────────────────

/// Names that may go on the remote command line and into `ssh`'s argument list.
fn safe_word(s: &str) -> bool {
    !s.is_empty() && !s.starts_with('-') && s.bytes().all(|b| b.is_ascii_alphanumeric() || b"._@-:".contains(&b))
}

/// Where ssh keeps its shared connection. A Unix socket's path is limited to 108 bytes and ssh
/// adds a temporary suffix while creating it, so a long runtime directory is not used.
pub fn control_path() -> String {
    control_path_in(&std::env::var("XDG_RUNTIME_DIR").unwrap_or_default())
}

fn control_path_in(runtime: &str) -> String {
    let name = "coucou-ssh-%C";
    // %C expands to 40 hex digits; ssh appends 17 more characters while binding.
    if !runtime.is_empty() && runtime.len() + 1 + name.len() + 40 + 17 <= 104 {
        format!("{runtime}/{name}")
    } else {
        format!("/tmp/{name}")
    }
}

/// The text printed by the remote script, split into its `@@name` sections.
pub fn sections(out: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    let mut name: Option<String> = None;
    for line in out.lines() {
        if let Some(n) = line.strip_prefix("@@") {
            name = Some(n.trim().to_string());
            map.entry(n.trim().to_string()).or_insert_with(String::new);
        } else if let Some(n) = &name {
            let e = map.get_mut(n).unwrap();
            e.push_str(line);
            e.push('\n');
        }
    }
    map
}

/// Vitals from the sections of a remote reading.
pub fn from_sections(sec: &std::collections::HashMap<String, String>, prev: &Option<Raw>, secs: f64) -> (Vitals, Raw) {
    let get = |k: &str| sec.get(k).map(String::as_str).unwrap_or("");
    let raw = raw_from(get("stat"), get("net"), get("disk"));
    let mut v = Vitals::default();
    fill(&mut v, &raw, prev, secs, get("meminfo"), get("loadavg"), get("uptime"));
    v.disks = parse_df(get("df"));
    v.temp_c = pick_temp(get("temp"));
    let bat: Vec<&str> = get("bat").lines().collect();
    if let (Some(p), Some(s)) = (bat.first().and_then(|p| p.trim().parse::<f64>().ok()), bat.get(1)) {
        v.battery = Some(Battery { pct: p, charging: matches!(s.trim(), "Charging" | "Full") });
    }
    v.services = get("svc")
        .lines()
        .filter_map(|l| {
            let (n, st) = l.split_once(' ')?;
            Some(ServiceState { name: n.to_string(), active: st.trim() == "active" })
        })
        .collect();
    let rows: Vec<&str> = get("docker").lines().filter(|l| !l.trim().is_empty()).collect();
    v.containers = if sec.contains_key("docker") && !rows.is_empty() {
        Some((rows.len() as u32, rows.iter().filter(|l| l.contains("unhealthy")).count() as u32))
    } else {
        None
    };
    (v, raw)
}

const REMOTE_SCRIPT: &str = r#"for f in stat meminfo loadavg uptime; do echo "@@$f"; cat /proc/$f; done
echo @@net; cat /proc/net/dev
echo @@disk; cat /proc/diskstats
echo @@df; df -P -x tmpfs -x devtmpfs -x squashfs -x overlay -x efivarfs 2>/dev/null | tail -n +2
echo @@temp; for h in /sys/class/hwmon/hwmon*; do echo "$(cat $h/name 2>/dev/null) $(cat $h/temp1_input 2>/dev/null)"; done
echo @@bat; cat /sys/class/power_supply/BAT*/capacity /sys/class/power_supply/BAT*/status 2>/dev/null
echo @@svc; for s in __SERVICES__; do echo "$s $(systemctl is-active $s 2>/dev/null)"; done
echo @@docker; docker ps --format '{{.Status}}' 2>/dev/null"#;

/// One reading of the server over `ssh` (key authentication only, one shared connection).
pub fn remote(host: &str, services: &str) -> Result<Vitals, String> {
    if !safe_word(host) {
        return Err("The server's name is not valid".into());
    }
    let names: Vec<&str> = services.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
    if !names.iter().all(|n| safe_word(n)) {
        return Err("A service name is not valid".into());
    }
    let script = REMOTE_SCRIPT.replace("__SERVICES__", &names.join(" "));
    let control = control_path();
    let out = Command::new("ssh")
        .args(["-o", "BatchMode=yes", "-o", "ConnectTimeout=6", "-o", "ServerAliveInterval=5", "-o", "ServerAliveCountMax=2"])
        .args(["-o", "ControlMaster=auto", "-o", "ControlPersist=120"])
        .args(["-o", &format!("ControlPath={control}")])
        .arg(host)
        .arg(script)
        .stdin(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|_| "ssh is not installed".to_string())?;
    if !out.status.success() || out.stdout.is_empty() {
        // ssh's own first line says why (no key, host unknown, timed out…).
        let why = String::from_utf8_lossy(&out.stderr);
        let why = why.lines().next().unwrap_or("").trim().chars().take(110).collect::<String>();
        return Err(if why.is_empty() { format!("Cannot reach {host} over ssh") } else { format!("Cannot reach {host}: {why}") });
    }
    let sec = sections(&String::from_utf8_lossy(&out.stdout));
    let now = Instant::now();
    let mut last = REMOTE.lock().unwrap();
    let prev = last.as_ref().filter(|l| now.duration_since(l.at) < Duration::from_secs(15));
    let secs = prev.map(|l| now.duration_since(l.at).as_secs_f64()).unwrap_or(0.0);
    let (v, raw) = from_sections(&sec, &prev.map(|l| l.raw.clone()), secs);
    *last = Some(Last { at: now, raw });
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAT: &str = "cpu  100 0 50 800 50 0 0 0 0 0\ncpu0 50 0 25 400 25 0 0 0 0 0\n";

    #[test]
    fn cpu_is_the_share_of_non_idle_time_between_two_readings() {
        assert_eq!(parse_cpu(STAT), Some((1000, 850)));
        let before = Raw { cpu_total: 1000, cpu_idle: 850, ..Default::default() };
        let after = Raw { cpu_total: 1200, cpu_idle: 950, ..Default::default() };
        let (cpu, ..) = rates(&Some(before), &after, 1.0);
        assert!((cpu - 50.0).abs() < 1e-9, "100 of 200 jiffies busy = 50%");
        assert_eq!(rates(&None, &after, 1.0).0, 0.0, "no earlier reading, no rate");
    }

    #[test]
    fn memory_net_and_disks_are_read_from_the_proc_text() {
        let mem = "MemTotal: 8000000 kB\nMemFree: 1 kB\nMemAvailable: 2000000 kB\nSwapTotal: 1000 kB\nSwapFree: 500 kB\n";
        assert_eq!(parse_meminfo(mem), (8_000_000, 2_000_000, 1000, 500));
        let net = "Inter-|   Receive\n face |bytes    packets\n    lo: 999 1 0 0 0 0 0 0 999 1 0 0 0 0 0 0\n  eth0: 1000 1 0 0 0 0 0 0 500 1 0 0 0 0 0 0\ndocker0: 7 1 0 0 0 0 0 0 7 1 0 0 0 0 0 0\n wlan0: 200 1 0 0 0 0 0 0 100 1 0 0 0 0 0 0\n";
        assert_eq!(parse_net(net), (1200, 600));
        let disks = "   8       0 sda 1 0 100 0 1 0 200 0 0 0 0\n   8       1 sda1 1 0 999 0 1 0 999 0 0 0 0\n 259       0 nvme0n1 1 0 10 0 1 0 20 0 0 0 0\n 259       1 nvme0n1p1 1 0 5 0 1 0 5 0 0 0 0\n   7       0 loop0 1 0 77 0 1 0 77 0 0 0 0\n";
        assert_eq!(parse_disks(disks), ((100 + 10) * 512, (200 + 20) * 512));
    }

    #[test]
    fn load_uptime_df_and_temperature() {
        assert_eq!(parse_loadavg("0.50 0.25 0.10 1/200 999\n"), [0.5, 0.25, 0.1]);
        assert_eq!(parse_uptime("12345.67 9999.0\n"), 12345);
        let df = "/dev/sda2 235929600 89650172 134 38% /\n/dev/sdb1 960000000 259200000 700000000 27% /mnt/hdd\n";
        let d = parse_df(df);
        assert_eq!(d.len(), 2);
        assert_eq!(d[1].mount, "/mnt/hdd");
        assert_eq!(d[1].pct, 27.0);
        assert!(pick_temp("acpitz 50000\ncoretemp 61000\nnvme 40000\n") == Some(61.0));
        assert_eq!(pick_temp("acpitz 50000\nnvme 70000\n"), Some(70.0), "no CPU sensor: the hottest");
        assert_eq!(pick_temp("hp \n"), None);
    }

    #[test]
    fn the_remote_reading_is_split_by_its_markers_and_services_are_read() {
        let out = "@@stat\ncpu  10 0 10 70 10 0 0 0 0 0\n@@meminfo\nMemTotal: 1000 kB\nMemAvailable: 250 kB\n@@loadavg\n1.0 2.0 3.0 1/1 1\n@@uptime\n100.0 1.0\n@@net\nh\nh\neth0: 10 0 0 0 0 0 0 0 20 0\n@@disk\n@@df\n/dev/sda2 100000000 38000000 1 38% /\n@@temp\ncoretemp 55000\n@@bat\n88\nDischarging\n@@svc\nhermes-gateway active\nollama inactive\n@@docker\nUp 2 days (healthy)\nUp 1 day (unhealthy)\nUp 5 hours\n";
        let sec = sections(out);
        let (v, _) = from_sections(&sec, &None, 0.0);
        assert_eq!(v.mem_pct, 75.0);
        assert_eq!(v.load, [1.0, 2.0, 3.0]);
        assert_eq!(v.temp_c, Some(55.0));
        assert_eq!(v.battery, Some(Battery { pct: 88.0, charging: false }));
        assert_eq!(v.services, vec![ServiceState { name: "hermes-gateway".into(), active: true }, ServiceState { name: "ollama".into(), active: false }]);
        assert_eq!(v.containers, Some((3, 1)));
        assert_eq!(v.disks[0].pct, 38.0);
    }

    #[test]
    fn the_ssh_socket_path_stays_short_enough() {
        assert_eq!(control_path_in("/run/user/1000"), "/run/user/1000/coucou-ssh-%C");
        assert_eq!(control_path_in("/home/alan/.claude/jobs/48fd83f2/tmp/xdg7/with/a/very/long/path/indeed"), "/tmp/coucou-ssh-%C");
        assert_eq!(control_path_in(""), "/tmp/coucou-ssh-%C");
    }

    #[test]
    fn only_plain_words_reach_the_ssh_command_line() {
        assert!(safe_word("server") && safe_word("alan@100.92.254.125") && safe_word("hermes-gateway"));
        assert!(!safe_word("-oProxyCommand=x") && !safe_word("a b") && !safe_word("x;rm") && !safe_word(""));
    }
}
