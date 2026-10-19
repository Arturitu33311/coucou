// The Hub's notes: one scratch pad, kept as a plain text file in Coucou's folder, so it can
// be read (and, if Jinx is shared, is) anywhere a text file can.

use std::path::PathBuf;

use crate::settings;

/// A pad, not a library.
pub const MAX_BYTES: usize = 100 * 1024;

fn path() -> PathBuf {
    settings::local_dir().join("notes.md")
}

pub fn load() -> String {
    std::fs::read_to_string(path()).unwrap_or_default()
}

/// Saves the pad (through a temporary file, so a crash never leaves half of it).
pub fn save(text: &str) -> Result<(), String> {
    if text.len() > MAX_BYTES {
        return Err("The notes are too long (100 KB at most)".into());
    }
    let p = path();
    if let Some(dir) = p.parent() {
        crate::platform::ensure_private_dir(dir).map_err(|e| e.to_string())?;
    }
    save_to(&p, text)
}

fn save_to(p: &std::path::Path, text: &str) -> Result<(), String> {
    let tmp = p.with_extension("md.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, p).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pad_is_saved_whole_and_a_huge_one_is_refused() {
        let dir = std::env::temp_dir().join(format!("coucou-notes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("notes.md");
        save_to(&p, "comprar café\n- llamar a mamá").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "comprar café\n- llamar a mamá");
        save_to(&p, "otra cosa").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "otra cosa");
        assert!(!dir.join("notes.md.tmp").exists(), "no temporary file is left");
        assert!(save(&"x".repeat(MAX_BYTES + 1)).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
