//! Clipboard text that the phone sends to this computer (phone -> PC; the other direction is KDE Connect's).
//! Pure rules, the same ones the phone applies before sending (android/.../coucou/clip/ClipSync.kt): the fingerprint
//! values are pinned in both test suites so the two sides always agree.

use std::sync::Mutex;

/// Same limit as the clipboard history (`clipboard.rs`).
pub const MAX_CHARS: usize = 4_000;

/// The text as it may be used: control characters other than newline and tab dropped, trimmed; `None` when what is
/// left is empty or longer than `MAX_CHARS` (a long text is not cut, it is refused).
pub fn clean(text: &str) -> Option<String> {
    let kept: String = text.chars().filter(|c| *c == '\n' || *c == '\t' || !c.is_control()).collect();
    let out = kept.trim();
    if out.is_empty() || out.chars().count() > MAX_CHARS {
        None
    } else {
        Some(out.to_string())
    }
}

/// FNV-1a 64 over the UTF-8 bytes. The same value on the phone; never put the text itself in a log.
pub fn fingerprint(text: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Applies a text only when it is not the one just applied (a phone that sends the same copy twice, or its echo
/// after KDE Connect brought it back, must not rewrite the clipboard again).
#[derive(Debug, Default)]
pub struct ApplyGuard {
    last: Option<u64>,
}

impl ApplyGuard {
    pub const fn new() -> Self {
        ApplyGuard { last: None }
    }

    pub fn should_apply(&mut self, text: &str) -> bool {
        let fp = fingerprint(text);
        if self.last == Some(fp) {
            return false;
        }
        self.last = Some(fp);
        true
    }
}

static GUARD: Mutex<ApplyGuard> = Mutex::new(ApplyGuard::new());

/// The shared guard for the phone link.
pub fn should_apply(text: &str) -> bool {
    GUARD.lock().unwrap().should_apply(text)
}

/// A short, harmless description for a log or a notification: how long, never what.
pub fn describe(text: &str) -> String {
    format!("{} characters", text.chars().count())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_code_survives_and_whitespace_is_trimmed() {
        assert_eq!(clean("482913").as_deref(), Some("482913"));
        assert_eq!(clean("  a\n\tb \n").as_deref(), Some("a\n\tb"));
    }

    #[test]
    fn control_characters_are_dropped() {
        assert_eq!(clean("ho\u{0}la").as_deref(), Some("hola"));
        assert_eq!(clean("a\u{7}b").as_deref(), Some("ab"));
    }

    #[test]
    fn nothing_useful_is_none() {
        assert_eq!(clean(""), None);
        assert_eq!(clean("  \n\t "), None);
        assert_eq!(clean("\u{7}\u{0}"), None);
    }

    #[test]
    fn the_limit_counts_characters_not_bytes_and_longer_is_refused() {
        assert_eq!(clean(&"x".repeat(4000)).map(|s| s.len()), Some(4000));
        assert_eq!(clean(&"x".repeat(4001)), None);
        assert_eq!(clean(&"😀".repeat(4000)).map(|s| s.chars().count()), Some(4000));
        assert_eq!(clean(&"😀".repeat(4001)), None);
    }

    #[test]
    fn fingerprints_match_the_phones_pinned_values() {
        // The same numbers as ClipSyncTest.kt (there as signed longs).
        assert_eq!(fingerprint(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fingerprint("a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fingerprint("482913"), 0xc904_3737_b0d7_49a0);
        assert_ne!(fingerprint("482913"), fingerprint("482914"));
    }

    #[test]
    fn the_same_text_twice_in_a_row_is_applied_once() {
        let mut g = ApplyGuard::new();
        assert!(g.should_apply("x"));
        assert!(!g.should_apply("x"));
        assert!(g.should_apply("y"));
        assert!(g.should_apply("x"));
    }

    #[test]
    fn describing_never_leaks_the_text() {
        assert_eq!(describe("482913"), "6 characters");
        assert!(!describe("482913").contains("48"));
    }
}
