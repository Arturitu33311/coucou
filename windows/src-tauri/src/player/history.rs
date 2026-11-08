// Local play history of the glass player: newest first, no repeats, capped.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::Item;
use crate::settings;

pub const HISTORY_MAX: usize = 50;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub item: Item,
    pub at_ms: i64,
}

pub fn path() -> PathBuf {
    settings::local_dir().join("player-history.json")
}

/// Puts `item` first; an older entry with the same (source, id) is dropped.
pub fn push(list: &mut Vec<HistoryEntry>, item: Item, at_ms: i64) {
    list.retain(|e| !(e.item.source == item.source && e.item.id == item.id));
    list.insert(0, HistoryEntry { item, at_ms });
    list.truncate(HISTORY_MAX);
}

pub fn load() -> Vec<HistoryEntry> {
    load_from(&path())
}

pub fn save(list: &[HistoryEntry]) -> Result<(), String> {
    let p = path();
    if let Some(dir) = p.parent() {
        crate::platform::ensure_private_dir(dir).map_err(|e| e.to_string())?;
    }
    save_to(&p, list)
}

fn load_from(p: &Path) -> Vec<HistoryEntry> {
    std::fs::read_to_string(p)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_to(p: &Path, list: &[HistoryEntry]) -> Result<(), String> {
    let text = serde_json::to_string_pretty(list).map_err(|e| e.to_string())?;
    let tmp = p.with_extension("json.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, p).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::{Kind, Source};

    fn item(source: Source, id: &str) -> Item {
        Item {
            source,
            kind: Kind::Track,
            id: id.into(),
            title: format!("t-{id}"),
            subtitle: "s".into(),
            art_url: "https://x/a.jpg".into(),
        }
    }

    #[test]
    fn push_puts_newest_first() {
        let mut l = Vec::new();
        push(&mut l, item(Source::Spotify, "a"), 1);
        push(&mut l, item(Source::Spotify, "b"), 2);
        assert_eq!(l[0].item.id, "b");
        assert_eq!(l[1].item.id, "a");
    }

    #[test]
    fn push_dedupes_same_source_and_id() {
        let mut l = Vec::new();
        push(&mut l, item(Source::Spotify, "a"), 1);
        push(&mut l, item(Source::Ytmusic, "a"), 2);
        push(&mut l, item(Source::Spotify, "b"), 3);
        push(&mut l, item(Source::Spotify, "a"), 4);
        assert_eq!(l.len(), 3);
        assert_eq!((l[0].item.source, l[0].item.id.as_str(), l[0].at_ms), (Source::Spotify, "a", 4));
        assert!(l.iter().any(|e| e.item.source == Source::Ytmusic && e.item.id == "a"));
    }

    #[test]
    fn push_caps_at_50() {
        let mut l = Vec::new();
        for i in 0..60 {
            push(&mut l, item(Source::Spotify, &i.to_string()), i);
        }
        assert_eq!(l.len(), HISTORY_MAX);
        assert_eq!(l[0].item.id, "59");
        assert_eq!(l[49].item.id, "10");
    }

    #[test]
    fn roundtrip_json() {
        let mut l = Vec::new();
        push(&mut l, item(Source::Spotify, "a"), 1);
        push(&mut l, item(Source::Ytmusic, "b"), 2);
        let text = serde_json::to_string(&l).unwrap();
        assert!(text.contains("\"ytmusic\"") && text.contains("\"artUrl\"") && text.contains("\"track\""));
        let back: Vec<HistoryEntry> = serde_json::from_str(&text).unwrap();
        assert_eq!(back, l);
    }

    #[test]
    fn save_and_load_through_a_file() {
        let dir = std::env::temp_dir().join(format!("coucou-hist-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("player-history.json");
        assert!(load_from(&p).is_empty());
        let mut l = Vec::new();
        push(&mut l, item(Source::Spotify, "a"), 1);
        save_to(&p, &l).unwrap();
        assert_eq!(load_from(&p), l);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
