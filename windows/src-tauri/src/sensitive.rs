// Private details that must not leave this computer for a language model: card numbers, bank
// account numbers, ID numbers, passwords. Coucou copies Alan's notes to the server (Jinx reads
// them) and hands Jinx the text of files he drops on her: any line that looks like one of these is
// replaced by a marker that names the KIND of thing, never its value ("[privado: tarjeta]").
//
// The detection (a keyword list, a Luhn check on card-like numbers, the SSN shape) follows the
// "sensitive facts" rules of Plip (hussainn7/plip-oss, MIT), plus the Mexican ones Alan's data
// has: CLABE and CURP. It is conservative on purpose: a false positive hides a line from Jinx,
// a miss sends a secret to a model.

/// Words that make a whole line private. Matched on word boundaries, lower-cased.
const KEYWORDS: &[(&str, &str)] = &[
    ("passport", "pasaporte"),
    ("pasaporte", "pasaporte"),
    ("social security", "seguro social"),
    ("ssn", "seguro social"),
    ("card number", "tarjeta"),
    ("credit card", "tarjeta"),
    ("debit card", "tarjeta"),
    ("tarjeta de crédito", "tarjeta"),
    ("tarjeta de credito", "tarjeta"),
    ("tarjeta de débito", "tarjeta"),
    ("tarjeta de debito", "tarjeta"),
    ("número de tarjeta", "tarjeta"),
    ("numero de tarjeta", "tarjeta"),
    ("cvv", "tarjeta"),
    ("cvc", "tarjeta"),
    ("clabe", "cuenta bancaria"),
    ("iban", "cuenta bancaria"),
    ("routing number", "cuenta bancaria"),
    ("account number", "cuenta bancaria"),
    ("número de cuenta", "cuenta bancaria"),
    ("numero de cuenta", "cuenta bancaria"),
    ("password", "contraseña"),
    ("passcode", "contraseña"),
    ("contraseña", "contraseña"),
    ("contrasena", "contraseña"),
    ("curp", "identificación"),
    ("private key", "clave privada"),
    ("-----begin", "clave privada"),
    ("api key", "clave de API"),
    ("api_key", "clave de API"),
];

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Does `needle` appear in `hay` as whole words (so "ssn" is not found inside "lessness")?
fn has_word(hay: &str, needle: &str) -> bool {
    let mut from = 0;
    while let Some(i) = hay[from..].find(needle) {
        let start = from + i;
        let end = start + needle.len();
        let before = hay[..start].chars().next_back().is_none_or(|c| !is_word_char(c));
        // "password" is also "passwords" / "passwords:" (one plural s is allowed).
        let rest = &hay[end..];
        let after = rest.chars().next().is_none_or(|c| !is_word_char(c))
            || (rest.starts_with('s') && rest[1..].chars().next().is_none_or(|c| !is_word_char(c)));
        // A needle that starts with a symbol ("-----begin") has no boundary on its left to speak of.
        let left_ok = before || !needle.chars().next().is_some_and(is_word_char);
        if left_ok && after {
            return true;
        }
        from = start + needle.len().max(1);
        while from < hay.len() && !hay.is_char_boundary(from) {
            from += 1;
        }
    }
    false
}

fn luhn(digits: &[u8]) -> bool {
    let mut total = 0u32;
    for (i, d) in digits.iter().rev().enumerate() {
        let mut n = u32::from(*d);
        if i % 2 == 1 {
            n *= 2;
            if n > 9 {
                n -= 9;
            }
        }
        total += n;
    }
    total % 10 == 0
}

/// The CLABE's check digit: weights 3, 7, 1 repeating over the first 17 digits.
fn clabe_ok(d: &[u8]) -> bool {
    if d.len() != 18 {
        return false;
    }
    let w = [3u32, 7, 1];
    let sum: u32 = d[..17].iter().enumerate().map(|(i, x)| (u32::from(*x) * w[i % 3]) % 10).sum();
    u32::from(d[17]) == (10 - sum % 10) % 10
}

/// Runs of digit groups (one space, hyphen or dot may sit between groups). Each run is the groups'
/// digits in order, and the cumulative end of every group.
struct Run {
    digits: Vec<u8>,
    ends: Vec<usize>,
}

fn digit_runs(line: &str) -> Vec<Run> {
    let mut runs = Vec::new();
    let mut cur = Run { digits: Vec::new(), ends: Vec::new() };
    let mut gap = false;
    let mut flush = |cur: &mut Run, runs: &mut Vec<Run>| {
        if !cur.digits.is_empty() {
            if cur.ends.last() != Some(&cur.digits.len()) {
                cur.ends.push(cur.digits.len());
            }
            runs.push(std::mem::replace(cur, Run { digits: Vec::new(), ends: Vec::new() }));
        }
    };
    for c in line.chars() {
        if let Some(d) = c.to_digit(10) {
            if gap {
                cur.ends.push(cur.digits.len()); // the group before the gap ended here
            }
            cur.digits.push(d as u8);
            gap = false;
        } else if (c == ' ' || c == '-' || c == '.') && !cur.digits.is_empty() && !gap {
            gap = true;
        } else {
            flush(&mut cur, &mut runs);
            gap = false;
        }
    }
    flush(&mut cur, &mut runs);
    runs
}

/// The Mexican CURP shape: 4 letters, 6 digits, H or M, 5 letters, one letter or digit, one digit.
fn is_curp(token: &str) -> bool {
    let b: Vec<char> = token.chars().collect();
    b.len() == 18
        && b[..4].iter().all(|c| c.is_ascii_alphabetic())
        && b[4..10].iter().all(|c| c.is_ascii_digit())
        && matches!(b[10].to_ascii_uppercase(), 'H' | 'M')
        && b[11..16].iter().all(|c| c.is_ascii_alphabetic())
        && b[16].is_ascii_alphanumeric()
        && b[17].is_ascii_digit()
}

/// The shape ddd-dd-dddd.
fn has_ssn(line: &str) -> bool {
    let b = line.as_bytes();
    (0..b.len().saturating_sub(10)).any(|i| {
        let w = &b[i..i + 11];
        let before_ok = i == 0 || !b[i - 1].is_ascii_digit();
        let after_ok = b.get(i + 11).is_none_or(|c| !c.is_ascii_digit());
        before_ok
            && after_ok
            && w[3] == b'-'
            && w[6] == b'-'
            && w.iter().enumerate().all(|(k, c)| k == 3 || k == 6 || c.is_ascii_digit())
    })
}

/// The line as the detector reads it, so a spelling trick does not walk around it: JSON `\uXXXX`
/// escapes decoded, zero-width and combining marks dropped, accents removed, full-width and
/// Arabic-Indic digits turned into ASCII digits, lower case.
pub fn fold(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut decoded = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && i + 6 <= chars.len() && chars[i + 1] == 'u' && chars[i + 2..i + 6].iter().all(char::is_ascii_hexdigit) {
            let hex: String = chars[i + 2..i + 6].iter().collect();
            if let Some(c) = u32::from_str_radix(&hex, 16).ok().and_then(char::from_u32) {
                decoded.push(c);
                i += 6;
                continue;
            }
        }
        decoded.push(chars[i]);
        i += 1;
    }
    let mut out = String::with_capacity(decoded.len());
    for c in decoded.chars() {
        let c = match c {
            '\u{200B}'..='\u{200D}' | '\u{FEFF}' | '\u{00AD}' | '\u{2060}' | '\u{0300}'..='\u{036F}' => continue,
            '\u{FF10}'..='\u{FF19}' => char::from(b'0' + (c as u32 - 0xFF10) as u8),
            '\u{0660}'..='\u{0669}' => char::from(b'0' + (c as u32 - 0x0660) as u8),
            '\u{06F0}'..='\u{06F9}' => char::from(b'0' + (c as u32 - 0x06F0) as u8),
            other => other,
        };
        for l in c.to_lowercase() {
            out.push(match l {
                'á' | 'à' | 'ä' | 'â' | 'ã' => 'a',
                'é' | 'è' | 'ë' | 'ê' => 'e',
                'í' | 'ì' | 'ï' | 'î' => 'i',
                'ó' | 'ò' | 'ö' | 'ô' | 'õ' => 'o',
                'ú' | 'ù' | 'ü' | 'û' => 'u',
                'ñ' => 'n',
                'ç' => 'c',
                x => x,
            });
        }
    }
    out
}

fn check_number(w: &[u8]) -> Option<&'static str> {
    if w.len() == 18 && clabe_ok(w) {
        return Some("cuenta bancaria");
    }
    if (13..=19).contains(&w.len()) && luhn(w) {
        return Some("tarjeta");
    }
    None
}

/// Is a card or a CLABE written in this run of digit groups? Any stretch of consecutive groups is
/// tried (so a card followed by a year, an amount or another number is still found), and a single
/// glued group longer than a card is tried in every window.
fn run_hits(run: &Run) -> Option<&'static str> {
    let ends: Vec<usize> = run.ends.iter().copied().take(24).collect();
    for i in 0..ends.len() {
        let start = if i == 0 { 0 } else { ends[i - 1] };
        for end in &ends[i..] {
            if let Some(kind) = check_number(&run.digits[start..*end]) {
                return Some(kind);
            }
        }
        let group = &run.digits[start..ends[i]];
        if group.len() > 19 {
            let g = if group.len() > 64 { &group[..64] } else { group };
            for s in 0..g.len() {
                for len in 13..=19usize.min(g.len() - s) {
                    if let Some(kind) = check_number(&g[s..s + len]) {
                        return Some(kind);
                    }
                }
            }
        }
    }
    None
}

/// What makes this line private ("tarjeta", "contraseña"…), or None.
pub fn kind_of(line: &str) -> Option<&'static str> {
    // Our own markers ("[privado: contraseña]") name a kind and must not read as one themselves.
    let mut f = fold(line);
    while let Some(i) = f.find("[privado: ") {
        match f[i..].find(']') {
            Some(j) => f.replace_range(i..=i + j, ""),
            None => {
                f.truncate(i);
                break;
            }
        }
    }
    for (word, kind) in KEYWORDS {
        if has_word(&f, &fold(word)) {
            return Some(kind);
        }
    }
    for run in digit_runs(&f) {
        if let Some(kind) = run_hits(&run) {
            return Some(kind);
        }
    }
    if has_ssn(&f) {
        return Some("seguro social");
    }
    if f.split(|c: char| !c.is_ascii_alphanumeric()).any(is_curp) {
        return Some("identificación");
    }
    None
}

/// A line that names a secret without giving it ("Contraseña:", "password =") has its value on the
/// next line.
fn names_without_value(line: &str) -> bool {
    let t = line.trim_end();
    t.ends_with(':') || t.ends_with('=') || t.ends_with("es:") || t.ends_with("is")
}

/// " (n línea(s) privada(s) no enviadas)" for a note to the model, or nothing.
pub fn note(hidden: usize) -> String {
    if hidden > 0 {
        format!(" ({hidden} línea(s) privada(s) no enviadas)")
    } else {
        String::new()
    }
}

/// `text` with every private line replaced by a marker naming its kind, and how many were.
///
/// Three things reach beyond the line itself: a key block ("-----BEGIN … -----END") goes whole, the
/// line after a label that names a secret without giving it ("Contraseña:") goes too, and in JSON
/// only the value of the line is replaced, so the file still parses.
pub fn redact(text: &str) -> (String, usize) {
    let mut hidden = 0;
    let mut in_key = false;
    let mut value_next: Option<&'static str> = None;
    let mut out: Vec<String> = Vec::new();
    // Every kind of line break is a line break (a lone CR, NEL, LS, PS), so a secret cannot hide
    // behind one that the splitter does not know.
    let text = text.replace("\r\n", "\n").replace(['\r', '\u{85}', '\u{2028}', '\u{2029}'], "\n");
    for line in text.split('\n') {
        let lower = line.to_lowercase();
        if in_key {
            hidden += 1;
            out.push("[privado: clave privada]".to_string());
            if lower.contains("-----end") {
                in_key = false;
            }
            continue;
        }
        if let Some(kind) = value_next.take() {
            if !line.trim().is_empty() {
                hidden += 1;
                out.push(format!("[privado: {kind}]"));
                continue;
            }
            value_next = Some(kind); // blank lines in between: the value is still to come
        }
        match kind_of(line) {
            Some(kind) => {
                hidden += 1;
                if lower.contains("-----begin") && !lower.contains("-----end") {
                    in_key = true;
                }
                if names_without_value(line) {
                    value_next = Some(kind);
                }
                out.push(mark(line, kind));
            }
            None => out.push(line.to_string()),
        }
    }
    (out.join("\n"), hidden)
}

/// The marker that stands in for `line`. In a JSON line only the last string's value is replaced
/// (`  "title": "Pagar 4111…",` → `  "title": "[privado: tarjeta]",`).
fn mark(line: &str, kind: &str) -> String {
    let marker = format!("[privado: {kind}]");
    let t = line.trim_end_matches(|c: char| c == ',' || c.is_whitespace());
    let quotes: Vec<usize> = t.match_indices('"').map(|(i, _)| i).collect();
    let json_like = t.trim_start().starts_with(['"', '{', '[']) && quotes.len() >= 2;
    if json_like {
        // A line that is one `"key": "value"` pair: only the value (the last string) is replaced.
        let one_pair = t.trim_start().starts_with('"') && t.ends_with('"');
        // The opening quote of the last string: the previous unescaped quote before the closing one.
        let open = quotes[..quotes.len() - 1]
            .iter()
            .rev()
            .find(|&&i| i == 0 || t.as_bytes()[i - 1] != b'\\')
            .copied();
        if let (true, Some(open)) = (one_pair, open) {
            // prefix up to and including the opening quote, the marker, the closing quote, then the
            // comma and spaces that followed.
            let candidate = format!("{}{}\"{}", &t[..=open], marker, &line[t.len()..]);
            if kind_of(&candidate).is_none() {
                return candidate;
            }
        }
        // The secret is not (only) in the last string: a line with several values, or a bare
        // number. Every string's content goes; and if that is still not clean, the whole line.
        let mut all = String::new();
        let mut inside = false;
        let mut escaped = false;
        for c in line.chars() {
            if inside {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    inside = false;
                    all.push_str(&marker);
                    all.push('"');
                }
            } else {
                all.push(c);
                if c == '"' {
                    inside = true;
                }
            }
        }
        // A bare number outside any string ("card": 4111111111111111) becomes a zero.
        let mut cleaned = String::with_capacity(all.len());
        let mut digits = String::new();
        for c in all.chars().chain(std::iter::once('\0')) {
            if c.is_ascii_digit() {
                digits.push(c);
                continue;
            }
            if !digits.is_empty() {
                cleaned.push_str(if digits.len() >= 8 { "0" } else { &digits });
                digits.clear();
            }
            if c != '\0' {
                cleaned.push(c);
            }
        }
        if !inside && kind_of(&cleaned).is_none() {
            return cleaned;
        }
    }
    marker
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_numbers_are_found_by_their_checksum_not_by_length_alone() {
        assert_eq!(kind_of("mi visa 4111 1111 1111 1111 vence 12/28"), Some("tarjeta"));
        assert_eq!(kind_of("4111-1111-1111-1111"), Some("tarjeta"));
        assert_eq!(kind_of("pedido 1234567890123456"), None, "16 digits that are not a card");        assert_eq!(kind_of("teléfono 55 1234 5678"), None);
    }

    #[test]
    fn a_clabe_is_found_by_its_check_digit() {
        // 002010077777777771: BANAMEX example from the public CLABE description.
        assert_eq!(kind_of("transferir a 002010077777777771"), Some("cuenta bancaria"));
        assert_eq!(kind_of("002010077777777772"), None, "wrong check digit");
    }

    #[test]
    fn id_numbers_and_words_make_the_line_private() {
        assert_eq!(kind_of("SSN 123-45-6789"), Some("seguro social"));
        assert_eq!(kind_of("123-45-6789"), Some("seguro social"));
        assert_eq!(kind_of("mi curp es GOMC800101HDFRRL09"), Some("identificación"));
        assert_eq!(kind_of("Contraseña del wifi: abc"), Some("contraseña"));
        assert_eq!(kind_of("password = hunter2"), Some("contraseña"));
        assert_eq!(kind_of("-----BEGIN OPENSSH PRIVATE KEY-----"), Some("clave privada"));
    }

    #[test]
    fn ordinary_lines_pass_untouched() {
        for l in ["llamar a mamá a las 17:30", "tarea de biología, entregar el 12", "el ssnake game", "classness", ""] {
            assert_eq!(kind_of(l), None, "{l:?}");
        }
    }

    #[test]
    fn redaction_names_the_kind_and_keeps_the_rest() {
        let (out, n) = redact("compras\ntarjeta 4111 1111 1111 1111\nleche\ncontraseña: x");
        assert_eq!(n, 2);
        assert_eq!(out, "compras\n[privado: tarjeta]\nleche\n[privado: contraseña]");
        assert!(!out.contains("4111"));
        assert_eq!(redact("nada que ocultar"), ("nada que ocultar".to_string(), 0));
    }

    #[test]
    fn a_card_stuck_to_other_digits_is_still_found() {
        assert_eq!(kind_of("4111 1111 1111 1111 2028"), Some("tarjeta"), "followed by a year");
        assert_eq!(kind_of("Total 4111111111111111.50"), Some("tarjeta"), "followed by an amount");
        assert_eq!(kind_of("ref 7 4111 1111 1111 1111"), Some("tarjeta"), "preceded by a number");
    }

    #[test]
    fn spelling_tricks_do_not_walk_around_it() {
        assert_eq!(kind_of("\\u0034111 1111 1111 1111"), Some("tarjeta"), "JSON \\u escape");
        assert_eq!(kind_of("４１１１ １１１１ １１１１ １１１１"), Some("tarjeta"), "full-width digits");
        assert_eq!(kind_of("contrasen\u{303}a: x"), Some("contraseña"), "n plus a combining tilde");
        assert_eq!(kind_of("CONTRASEÑA: x"), Some("contraseña"), "upper case");
        assert_eq!(kind_of("pass\u{200B}word = x"), Some("contraseña"), "zero-width space inside");
        assert_eq!(kind_of("Número de tarjeta"), Some("tarjeta"));
        assert_eq!(kind_of("numero de tarjeta"), Some("tarjeta"), "without the accent");
    }

    #[test]
    fn a_lone_carriage_return_is_a_line_break() {
        let (out, n) = redact("hola\rcontraseña: x\rdespués");
        assert_eq!(n, 1);
        assert!(!out.contains(": x"), "{out}");
    }

    #[test]
    fn json_with_several_values_on_a_line_is_cleaned_whole() {
        let (out, n) = redact("{\"a\": \"tarjeta 4111 1111 1111 1111\", \"b\": \"x\"}");
        assert_eq!(n, 1);
        assert!(!out.contains("4111"), "{out}");
        assert!(serde_json::from_str::<serde_json::Value>(&out).is_ok(), "{out}");
        let (out, _) = redact("{\"card\": 4111111111111111}");
        assert!(!out.contains("4111"), "{out}");
        // Our own marker is not a secret, so cleaning twice changes nothing.
        let (again, n2) = redact(&out);
        assert_eq!((again, n2), (out, 0));
    }

    #[test]
    fn a_key_block_goes_whole() {
        let pem = "antes\n-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXk\nAAAAB3NzaC1\n-----END OPENSSH PRIVATE KEY-----\ndespués";
        let (out, n) = redact(pem);
        assert_eq!(n, 4);
        assert_eq!(out, "antes\n[privado: clave privada]\n[privado: clave privada]\n[privado: clave privada]\n[privado: clave privada]\ndespués");
        assert!(!out.contains("b3Blbn"));
    }

    #[test]
    fn a_label_alone_takes_the_next_line_with_it() {
        let (out, n) = redact("wifi\nContraseña:\n\nhunter2\notra cosa");
        assert_eq!(n, 2);
        assert!(!out.contains("hunter2"), "{out}");
        assert!(out.ends_with("otra cosa"));
        let (out, _) = redact("password =\nhunter2");
        assert!(!out.contains("hunter2"));
    }

    #[test]
    fn plurals_and_dotted_numbers_are_caught() {
        assert_eq!(kind_of("Mis passwords: ver abajo"), Some("contraseña"));
        assert_eq!(kind_of("4111.1111.1111.1111"), Some("tarjeta"));
    }

    #[test]
    fn json_keeps_its_shape() {
        let json = "{\n  \"title\": \"Pagar tarjeta 4111 1111 1111 1111\",\n  \"start\": 12\n}";
        let (out, n) = redact(json);
        assert_eq!(n, 1);
        assert!(serde_json::from_str::<serde_json::Value>(&out).is_ok(), "{out}");
        assert!(out.contains("\"[privado: tarjeta]\""));
        assert!(!out.contains("4111"));
    }
}
