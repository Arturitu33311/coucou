// Preferences, stored as plain JSON in settings.json under platform::config_dir().
// No secret ever lands here — API keys live in the OS keychain (see secrets.rs).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub sound_enabled: bool,
    pub sound_volume: f64,
    pub auto_close_interval: f64,
    pub absence_interval: f64,
    pub active_integrations: Vec<String>,
    /// "primary" = the main display, "cursor" = whichever display the mouse is on.
    pub screen: String,
    pub autostart: bool,
    pub hooks_installed: bool,
    /// Claude model used by the chat. Changeable in the settings window.
    /// Defaulted explicitly so a settings.json written by an older build still loads.
    #[serde(default = "default_model")]
    pub model: String,
    /// Music pill: fetch synchronised lyrics (lrclib.net) and album art given as
    /// an http(s) URL. Off until the user says so: both are network calls to
    /// services they did not configure otherwise.
    #[serde(default)]
    pub music_online: bool,
    /// Music pill visualizer: "off", "wave" or "beat".
    #[serde(default = "default_visualizer")]
    pub music_visualizer: String,
    /// Which script to prefer when lrclib has several versions of a song:
    /// "any", "original" or "latin".
    #[serde(default = "default_lyrics_language")]
    pub music_lyrics_language: String,
    /// Keep the Music pill on the last track after the player closes (the GNOME
    /// extension's "Always ON"). Off: the pill goes, and the minimised island with it.
    #[serde(default = "default_true")]
    pub music_keep: bool,
    /// What the wheel does over the minimised pill: "track" or "volume".
    #[serde(default = "default_scroll")]
    pub music_scroll: String,
    /// The minimised island opens when the pointer rests on it, and folds back quickly.
    #[serde(default)]
    pub open_on_hover: bool,
    /// Seconds the pointer must rest on the minimised island before it opens (open on hover).
    #[serde(default = "default_hover_delay")]
    pub hover_open_delay: f64,
    /// Where the island sits across the top of the screen: logical px from the centre
    /// (positive = right). Windows tiled beside the middle keep their buttons clear.
    #[serde(default)]
    pub island_offset_x: i32,
    /// The island steps aside when the pointer goes for the buttons of a window behind it.
    #[serde(default = "default_true")]
    pub dodge_windows: bool,
    /// The sound for "an agent finished": one of the built-in names, "none", or "file".
    #[serde(default = "default_finish_sound")]
    pub finish_sound: String,
    /// The audio file used when `finish_sound` is "file".
    #[serde(default)]
    pub finish_sound_file: String,
    /// The Hub's Server tab reads this machine over ssh ("" = none): an alias from ~/.ssh/config
    /// or user@host, with key authentication.
    #[serde(default)]
    pub server_host: String,
    /// The systemd services the Server tab shows, comma separated.
    #[serde(default = "default_server_services")]
    pub server_services: String,
    /// Keep Jinx in step with the Hub: notes, calendar and timer are pushed to the server's
    /// ~/.hermes/state/coucou/ so she can read them (needs `server_host`).
    #[serde(default = "default_true")]
    pub jinx_share: bool,
    /// The rclone remote used for the personal Google Drive, and the folder files go to there.
    #[serde(default = "default_drive_remote")]
    pub drive_remote: String,
    #[serde(default = "default_drive_folder")]
    pub drive_folder: String,
    /// The Google account (as listed in GNOME Online Accounts) whose Drive is used; empty = the first one.
    #[serde(default)]
    pub drive_account: String,
    /// The folder of the school's OneDrive files go to unless another is typed.
    #[serde(default = "default_school_folder")]
    pub school_folder: String,
    /// The Weather tab asks open-meteo.com for these cities (off until turned on).
    #[serde(default)]
    pub weather_on: bool,
    #[serde(default)]
    pub weather_city: String,
    #[serde(default)]
    pub weather_city2: String,
    /// Show other programs' notifications in the island (reads their content: off until turned on).
    #[serde(default)]
    pub notification_peek: bool,
    /// Let a phone follow Claude Code and answer its requests, through ssh (off until turned on).
    #[serde(default)]
    pub remote_enabled: bool,
}

fn default_drive_remote() -> String {
    "gdrive".into()
}

fn default_drive_folder() -> String {
    "Coucou".into()
}

fn default_school_folder() -> String {
    "Por clasificar".into()
}

fn default_server_services() -> String {
    "hermes-gateway,ollama,docker,tailscaled,smbd".into()
}

fn default_finish_sound() -> String {
    "finish".into()
}

fn default_hover_delay() -> f64 {
    0.9
}

fn default_scroll() -> String {
    "track".to_string()
}

fn default_true() -> bool {
    true
}

fn default_lyrics_language() -> String {
    "any".to_string()
}

fn default_model() -> String {
    crate::claude::DEFAULT_MODEL.to_string()
}

fn default_visualizer() -> String {
    // Real-time (cava) when it is installed; the island falls back to the animated
    // bars when it is not.
    "realtime".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            sound_enabled: true,
            sound_volume: 0.12,
            auto_close_interval: 15.0,
            absence_interval: 180.0,
            active_integrations: vec![
                "integration_resend".into(),
                "integration_n8n".into(),
                "integration_vercel".into(),
                "integration_github".into(),
            ],
            screen: "primary".into(),
            autostart: false,
            hooks_installed: false,
            model: default_model(),
            music_online: false,
            music_visualizer: default_visualizer(),
            music_lyrics_language: default_lyrics_language(),
            music_keep: true,
            music_scroll: default_scroll(),
            open_on_hover: false,
            hover_open_delay: default_hover_delay(),
            island_offset_x: 0,
            dodge_windows: true,
            finish_sound: default_finish_sound(),
            finish_sound_file: String::new(),
            server_host: String::new(),
            server_services: default_server_services(),
            jinx_share: true,
            drive_remote: default_drive_remote(),
            drive_folder: default_drive_folder(),
            drive_account: String::new(),
            school_folder: default_school_folder(),
            weather_on: false,
            weather_city: String::new(),
            weather_city2: String::new(),
            notification_peek: false,
            remote_enabled: false,
        }
    }
}

pub use crate::platform::{config_dir, local_dir};

pub fn hook_exe_path() -> PathBuf {
    local_dir().join("bin").join(crate::platform::HOOK_EXE)
}

fn settings_path() -> PathBuf {
    config_dir().join("settings.json")
}

pub fn load() -> Settings {
    match std::fs::read(settings_path()) {
        Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
        Err(_) => Settings::default(),
    }
}

pub fn save(settings: &Settings) -> std::io::Result<()> {
    let dir = config_dir();
    crate::platform::ensure_private_dir(&dir)?;
    let json = serde_json::to_vec_pretty(settings)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(settings_path(), json)
}
