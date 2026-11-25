// « OK Coucou »: hearing the wake phrase in what Parakeet wrote, and telling
// speech from silence on the microphone. Nothing here touches the OS.

/// Lower case, accents gone, everything but letters and digits a space.
pub fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        let base = match c {
            'à' | 'á' | 'â' | 'ä' | 'ã' | 'å' => 'a',
            'ç' => 'c',
            'è' | 'é' | 'ê' | 'ë' => 'e',
            'ì' | 'í' | 'î' | 'ï' => 'i',
            'ò' | 'ó' | 'ô' | 'ö' | 'õ' => 'o',
            'ù' | 'ú' | 'û' | 'ü' => 'u',
            'ÿ' | 'ý' => 'y',
            'ñ' => 'n',
            c if c.is_alphanumeric() => c,
            _ => ' ',
        };
        out.push(base);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// How Parakeet may write "OK" (and the "oye" / "hey" a person may say instead).
fn is_ok(word: &str) -> bool {
    matches!(
        word,
        "ok" | "okay" | "oke" | "okey" | "okei" | "oquei" | "okai" | "okeh" | "oky" | "okk" | "okok" | "hey" | "hei" | "ey" | "ei" | "eh"
            | "oye" | "oie" | "hola" | "ola" | "hi" | "hello" | "hallo" | "yo"
    )
}

/// What a word sounds like once the spellings that sound the same are one: "coucou", "cuckoo", "kuku", "cucú" and
/// "cocou" all become "cucu"/"cocu". Parakeet writes what it hears, in whichever language it guesses.
fn phon(word: &str) -> String {
    let s = word.replace("ou", "u").replace("oo", "u").replace("qu", "c").replace('k', "c");
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if out.chars().last() != Some(ch) {
            out.push(ch);
        }
    }
    out
}

/// Edit distance, for short words.
fn distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            cur.push((prev[j] + usize::from(ca != cb)).min(prev[j + 1] + 1).min(cur[j] + 1));
        }
        prev = cur;
    }
    prev[b.len()]
}

/// How Parakeet may write "coucou", in one word: the usual spellings, or anything that sounds like "cucu" give or take
/// a letter. Only ever asked right after an "OK", so a loose match here costs little.
fn is_coucou(word: &str) -> bool {
    if matches!(word, "coucou" | "coucous" | "cuckoo" | "cucu" | "kuku" | "koukou" | "cookoo" | "coocoo" | "cocou" | "kukuu" | "couscous") {
        return true;
    }
    let p = phon(word);
    (3..=5).contains(&p.chars().count()) && ["cucu", "cocu", "cuco", "coco"].iter().any(|t| distance(&p, t) <= 1)
}

/// How the name "Jinx" comes out of a speech recognizer, for an English or a Spanish speaker: Jinx, Jinks, Yinks, Jings,
/// Gins, Hinks, Jenks… The first letter may be j, g, y or h; "x", "cs" and "ck" are all "ks"; one letter of slack.
fn is_jinx(word: &str) -> bool {
    let w: String = word.chars().filter(|c| c.is_alphabetic()).flat_map(char::to_lowercase).collect();
    let Some(first) = w.chars().next() else { return false };
    if !"jgyh".contains(first) {
        return false;
    }
    let mut p = String::from("j");
    p.push_str(&w[first.len_utf8()..]);
    let p = p.replace('x', "ks").replace("cs", "ks").replace("ck", "k").replace('c', "k").replace('z', "s").replace('y', "i");
    let mut dedup = String::with_capacity(p.len());
    for ch in p.chars() {
        if dedup.chars().last() != Some(ch) {
            dedup.push(ch);
        }
    }
    (3..=6).contains(&dedup.chars().count()) && distance(&dedup, "jinks") <= 1
}

/// After an "OK" the name may also come as the English word it sounds like: "ok jeans" is « OK Jinx ».
fn is_jinx_after_ok(word: &str) -> bool {
    is_jinx(word)
        || matches!(
            word,
            "jeans" | "jean" | "jeanx" | "jeanks" | "yins" | "yines" | "gins" | "gin" | "jins" | "jin" | "yin" | "jinn" | "jinns"
                | "ginz" | "jinz" | "jenx" | "jenks" | "jincs" | "chinx" | "hinx" | "jhinx" | "jyinx" | "jinnx" | "yinx"
        )
}

/// How many words, from `at`, make her name (0 when they do not): usually one, sometimes the name comes split in two
/// ("jin x", "gin ks"). `after_ok` allows the looser spellings that are safe once an "OK" came first.
fn jinx_at(norm: &[String], at: usize, after_ok: bool) -> usize {
    let one = |w: &str| if after_ok { is_jinx_after_ok(w) } else { is_jinx(w) };
    let Some(w) = norm.get(at) else { return 0 };
    // The two words together first: "jin" + "x" is « Jinx », and taking "jin" alone would leave the "x" in the order.
    if let Some(next) = norm.get(at + 1) {
        if w.chars().count() <= 4 && next.chars().count() <= 3 && one(&format!("{w}{next}")) {
            return 2;
        }
    }
    usize::from(one(w))
}

/// The text after what was said, with the punctuation of the call removed.
fn rest_of(words: &[&str], from: usize) -> String {
    words.get(from..).unwrap_or(&[]).join(" ").trim_start_matches([',', '.', '!', ':', ' ']).trim().to_string()
}

/// …or in two.
fn is_coucou_pair(a: &str, b: &str) -> bool {
    if matches!((a, b), ("cuck", "oo")) {
        return true;
    }
    let (pa, pb) = (phon(a), phon(b));
    pa == pb && matches!(pa.as_str(), "cu" | "co" | "c")
}

/// When `text` holds the wake phrase, what was said after it (maybe nothing).
/// The words keep their case and punctuation, as Parakeet wrote them.
pub fn after_wake(text: &str) -> Option<String> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let norm: Vec<String> = words.iter().map(|w| normalize(w)).collect();
    // "o.k." and "o k" become two tokens: glue them.
    let mut i = 0;
    while i < norm.len() {
        let mut ok_end = None;
        if is_ok(&norm[i]) {
            ok_end = Some(i + 1);
        } else if norm[i] == "o k" || (norm[i] == "o" && norm.get(i + 1).map(String::as_str) == Some("k")) {
            ok_end = Some(if norm[i] == "o" { i + 2 } else { i + 1 });
        }
        if let Some(j) = ok_end {
            // « OK Jinx », « Hey Jinx », « Oye Jinx ».
            let n = jinx_at(&norm, j, true);
            if n > 0 {
                return Some(rest_of(&words, j + n));
            }
            // A word made of two ("ok coucou" written as one token is rare but happens).
            if let Some(w) = norm.get(j) {
                let parts: Vec<&str> = w.split(' ').collect();
                if parts.len() == 1 && is_coucou(parts[0]) {
                    return Some(words[j + 1..].join(" ").trim_start_matches([',', '.', '!', ' ']).trim().to_string());
                }
                if let Some(next) = norm.get(j + 1) {
                    if is_coucou_pair(w, next) {
                        return Some(words[j + 2..].join(" ").trim_start_matches([',', '.', '!', ' ']).trim().to_string());
                    }
                }
            }
        }
        // « Jinx, ... » on its own, only as the first word of what was said (so that her name in the middle of a
        // sentence does not wake her).
        if i == 0 {
            let n = jinx_at(&norm, 0, false);
            if n > 0 {
                return Some(rest_of(&words, n));
            }
        }
        // "okjinx" in one piece.
        if let Some(rest) = norm[i].strip_prefix("okay").or_else(|| norm[i].strip_prefix("ok")) {
            if is_jinx_after_ok(rest.trim()) {
                return Some(rest_of(&words, i + 1));
            }
        }
        // "okcoucou" in one piece.
        if let Some(rest) = norm[i].strip_prefix("okay").or_else(|| norm[i].strip_prefix("ok")) {
            if is_coucou(rest.trim()) {
                return Some(words[i + 1..].join(" ").trim().to_string());
            }
        }
        i += 1;
    }
    None
}

/// Speech or silence, 30 ms at a time, against a noise floor that follows the
/// room. A stretch of speech is handed out once followed by `hangover` frames
/// of silence, or when it gets too long.
pub struct Vad {
    floor: f32,
    in_speech: bool,
    speech_frames: usize,
    silent_frames: usize,
    frame: Vec<f32>,
    /// The current stretch, with a little audio from before it began.
    current: Vec<f32>,
    preroll: std::collections::VecDeque<f32>,
    /// Silence that ends a stretch, in frames.
    pub hangover: usize,
}

pub const FRAME: usize = 480; // 30 ms at 16 kHz
/// Quietest frame that can count as speech (RMS, full scale 1.0). The noise floor still raises the bar in a noisy room.
/// 0.012 (-38 dB) missed a voice that reaches the microphone at -48 dB; this is -50 dB.
const MIN_SPEECH_RMS: f32 = 0.003;
/// Level a stretch of speech is brought to before it is transcribed (RMS) and the most it is ever amplified (x31, 30 dB).
const TARGET_RMS: f32 = 0.08;
const MAX_GAIN: f32 = 31.6;

/// Brings a quiet stretch of speech up to a normal level, never past clipping. Louder ones are left alone.
pub fn level_up(seg: &mut [f32]) {
    if seg.is_empty() {
        return;
    }
    let rms = (seg.iter().map(|x| x * x).sum::<f32>() / seg.len() as f32).sqrt();
    let peak = seg.iter().fold(0.0f32, |m, x| m.max(x.abs()));
    if rms < 1e-5 || peak <= 0.0 {
        return;
    }
    let gain = (TARGET_RMS / rms).min(MAX_GAIN).min(0.9 / peak);
    if gain > 1.0 {
        for x in seg.iter_mut() {
            *x *= gain;
        }
    }
}
const PREROLL: usize = 16_000 * 3 / 10;
const MAX_SEGMENT: usize = 16_000 * 12;

impl Default for Vad {
    fn default() -> Self {
        Self {
            floor: 0.004,
            in_speech: false,
            speech_frames: 0,
            silent_frames: 0,
            frame: Vec::with_capacity(FRAME),
            current: Vec::new(),
            preroll: std::collections::VecDeque::with_capacity(PREROLL),
            hangover: 16,
        }
    }
}

impl Vad {
    #[cfg(test)]
    pub fn in_speech(&self) -> bool {
        self.in_speech
    }

    /// Feeds 16 kHz samples; the stretches of speech that ended.
    pub fn feed(&mut self, samples: &[f32]) -> Vec<Vec<f32>> {
        let mut done = Vec::new();
        for &s in samples {
            self.frame.push(s);
            if self.frame.len() < FRAME {
                continue;
            }
            let rms = (self.frame.iter().map(|x| x * x).sum::<f32>() / FRAME as f32).sqrt();
            let speech = rms > (self.floor * 3.0).max(MIN_SPEECH_RMS);
            if !speech && !self.in_speech {
                self.floor = self.floor * 0.97 + rms * 0.03;
            }
            if self.in_speech {
                self.current.extend_from_slice(&self.frame);
                if speech {
                    self.silent_frames = 0;
                } else {
                    self.silent_frames += 1;
                }
                if self.silent_frames >= self.hangover || self.current.len() >= MAX_SEGMENT {
                    done.push(std::mem::take(&mut self.current));
                    self.in_speech = false;
                    self.speech_frames = 0;
                    self.silent_frames = 0;
                }
            } else if speech {
                self.speech_frames += 1;
                if self.speech_frames >= 3 {
                    self.in_speech = true;
                    self.current = self.preroll.drain(..).collect();
                    self.current.extend_from_slice(&self.frame);
                }
            } else {
                self.speech_frames = 0;
            }
            if !self.in_speech {
                for &x in &self.frame {
                    if self.preroll.len() == PREROLL {
                        self.preroll.pop_front();
                    }
                    self.preroll.push_back(x);
                }
            }
            self.frame.clear();
        }
        done
    }

    /// Forgets what is being heard (Coucou speaks, or dictation starts).
    pub fn reset(&mut self) {
        self.in_speech = false;
        self.speech_frames = 0;
        self.silent_frames = 0;
        self.current.clear();
        self.preroll.clear();
        self.frame.clear();
    }
}

/// "fr-FR" for a French answer, else None (the system's language).
pub fn speech_lang(text: &str) -> Option<&'static str> {
    let n = normalize(text);
    let words: Vec<&str> = n.split(' ').collect();
    let french = ["le", "la", "les", "est", "tu", "vous", "je", "un", "une", "des", "pour", "pas", "c", "et", "sur", "avec", "ton", "ta"];
    let count = words.iter().filter(|w| french.contains(w)).count();
    (count * 6 >= words.len().max(1) && count >= 1).then_some("fr-FR")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wake_phrase_however_parakeet_writes_it() {
        assert_eq!(after_wake("OK Coucou, play some music.").as_deref(), Some("play some music."));
        assert_eq!(after_wake("Okay, coucou! Quelle heure est-il ?").as_deref(), Some("Quelle heure est-il ?"));
        assert_eq!(after_wake("O.K. Cuckoo what's the weather").as_deref(), Some("what's the weather"));
        assert_eq!(after_wake("ok cou cou ajoute Stripe").as_deref(), Some("ajoute Stripe"));
        assert_eq!(after_wake("OK Coucou.").as_deref(), Some(""));
        assert_eq!(after_wake("Hey coucou, ça va ?").as_deref(), Some("ça va ?"));
        assert_eq!(after_wake("on a dit coucou à Paul"), None);
        assert_eq!(after_wake("ok let's go"), None);
        assert_eq!(normalize("Ça VA, Élodie ?"), "ca va elodie");
    }

    #[test]
    fn spanish_speakers_and_loose_spellings_are_heard() {
        assert_eq!(after_wake("OK, cucú, enciende la luz").as_deref(), Some("enciende la luz"));
        assert_eq!(after_wake("Okey cuco ¿qué hora es?").as_deref(), Some("¿qué hora es?"));
        assert_eq!(after_wake("Oye cucu apaga la tele").as_deref(), Some("apaga la tele"));
        assert_eq!(after_wake("ok cú cú sube el volumen").as_deref(), Some("sube el volumen"));
        assert_eq!(after_wake("Okei coco").as_deref(), Some(""));
        assert_eq!(after_wake("ok kou kou pon música").as_deref(), Some("pon música"));
        assert_eq!(after_wake("okcucu").as_deref(), Some(""));
        // Not the wake phrase: an "OK" followed by something that does not sound like it.
        assert_eq!(after_wake("ok cuando llegamos"), None);
        assert_eq!(after_wake("ok cucumber salad"), None);
        assert_eq!(after_wake("ok cuña de madera"), None);
        assert_eq!(after_wake("ok coche rojo"), None);
        assert_eq!(after_wake("oye mira eso"), None);
        assert_eq!(phon("coucou"), "cucu");
        assert_eq!(phon("cuckoo"), "cucu");
        assert_eq!(distance("cucu", "cuco"), 1);
    }

    #[test]
    fn her_name_wakes_her_in_the_ways_a_recognizer_writes_it() {
        assert_eq!(after_wake("OK Jinx, apaga la tele").as_deref(), Some("apaga la tele"));
        assert_eq!(after_wake("Hey Jinx.").as_deref(), Some(""));
        assert_eq!(after_wake("Oye Jinx, ¿qué hora es?").as_deref(), Some("¿qué hora es?"));
        assert_eq!(after_wake("Okay Jinks sube el volumen").as_deref(), Some("sube el volumen"));
        assert_eq!(after_wake("ok yinks pon música").as_deref(), Some("pon música"));
        assert_eq!(after_wake("Okey Jeans apaga la luz").as_deref(), Some("apaga la luz"));
        assert_eq!(after_wake("okjinx").as_deref(), Some(""));
        // On its own, as the first word.
        assert_eq!(after_wake("Jinx, apaga la tele").as_deref(), Some("apaga la tele"));
        assert_eq!(after_wake("Jinx.").as_deref(), Some(""));
        assert_eq!(after_wake("Jings enciende la tele").as_deref(), Some("enciende la tele"));
        // OK Coucou still works.
        assert_eq!(after_wake("OK Coucou, play some music.").as_deref(), Some("play some music."));
    }

    #[test]
    fn many_ways_of_writing_her_name_and_calling_her_are_heard() {
        // Every spelling after an "OK" / "Hey" / a greeting.
        for name in ["Jinx", "Jinks", "Jink", "Jinc", "Jincs", "Jings", "Jynx", "Ginx", "Ginks", "Yinx", "Yinks", "Hinks", "Hinx",
                     "Jenks", "Jenx", "Jeans", "Jean", "Gins", "Gin", "Jins", "Yins", "Jinz", "Jinx's"] {
            for call in ["OK", "Okay", "Okey", "Okei", "Hey", "Oye", "Ey", "Hola", "Hi", "Hello"] {
                let said = format!("{call} {name}, apaga la tele");
                assert_eq!(after_wake(&said).as_deref(), Some("apaga la tele"), "{said}");
            }
        }
        // On its own, as the first word (no "jeans" or "gin" here: they are words people say).
        for name in ["Jinx", "Jinks", "Jink", "Jings", "Jynx", "Ginx", "Yinks", "Hinks", "Jenks"] {
            let said = format!("{name}, enciende la luz");
            assert_eq!(after_wake(&said).as_deref(), Some("enciende la luz"), "{said}");
        }
        // The name split in two words.
        assert_eq!(after_wake("ok jin x apaga la tele").as_deref(), Some("apaga la tele"));
        assert_eq!(after_wake("hey gin ks sube el volumen").as_deref(), Some("sube el volumen"));
        assert_eq!(after_wake("Jin x, dime la hora").as_deref(), Some("dime la hora"));
        // With the punctuation and case a recognizer adds.
        assert_eq!(after_wake("Ok, Jinx! ¿Qué tal?").as_deref(), Some("¿Qué tal?"));
        assert_eq!(after_wake("HEY JINX").as_deref(), Some(""));
    }

    #[test]
    fn her_name_in_the_middle_of_a_sentence_or_similar_words_do_not_wake_her() {
        assert_eq!(after_wake("ayer le dije a Jinx que lo hiciera"), None);
        assert_eq!(after_wake("me gustan los jeans azules"), None); // "jeans" counts only after an OK
        assert_eq!(after_wake("ok jinete de la noche"), None);
        assert_eq!(after_wake("hola buenos días a todos"), None);
        assert_eq!(after_wake("hi everyone, ginger is here"), None);
        assert_eq!(after_wake("hello hinge"), None);
        assert_eq!(after_wake("oye gimnasio abierto"), None);
        assert_eq!(after_wake("gin tonic por favor"), None); // "gin" alone is a drink; it counts only after an OK
        assert_eq!(after_wake("jeans y camisa"), None);
        assert_eq!(after_wake("ok gente vamos"), None);
        assert_eq!(after_wake("hinge es una app"), None);
        assert_eq!(after_wake("ok let's go"), None);
        assert!(is_jinx("Jinx") && is_jinx("jinks") && is_jinx("yinks") && is_jinx("Gins") && !is_jinx("jinete"));
    }

    #[test]
    fn a_quiet_voice_is_brought_up_but_never_clipped() {
        let mut quiet: Vec<f32> = (0..16_000).map(|i| (i as f32 * 0.1).sin() * 0.005).collect();
        level_up(&mut quiet);
        let rms = (quiet.iter().map(|x| x * x).sum::<f32>() / quiet.len() as f32).sqrt();
        assert!(rms > 0.05 && quiet.iter().all(|x| x.abs() <= 0.9 + 1e-3));
        let mut loud: Vec<f32> = (0..16_000).map(|i| (i as f32 * 0.1).sin() * 0.5).collect();
        let before = loud.clone();
        level_up(&mut loud);
        assert_eq!(loud, before);
        let mut silence = vec![0.0f32; 1600];
        level_up(&mut silence);
        assert!(silence.iter().all(|x| *x == 0.0));
    }

    #[test]
    fn a_voice_at_minus_48_db_counts_as_speech() {
        let mut vad = Vad::default();
        let silence = vec![0.0004f32; 32_000];
        let voice: Vec<f32> = (0..16_000).map(|i| (i as f32 * 0.1).sin() * 0.0055).collect(); // RMS ~0.004
        vad.feed(&silence);
        vad.feed(&voice);
        assert!(vad.in_speech());
    }

    #[test]
    fn speech_is_cut_after_a_pause() {
        let mut vad = Vad::default();
        let silence = vec![0.0f32; 16_000];
        let tone: Vec<f32> = (0..16_000).map(|i| (i as f32 * 0.1).sin() * 0.3).collect();
        assert!(vad.feed(&silence).is_empty());
        assert!(vad.feed(&tone).is_empty());
        assert!(vad.in_speech());
        let out = vad.feed(&silence);
        assert_eq!(out.len(), 1);
        // The tone, a pre-roll before it and the hangover after it.
        assert!(out[0].len() > 16_000 && out[0].len() <= 16_000 + 25 * FRAME + PREROLL + 2 * FRAME);
        assert!(!vad.in_speech());
    }

    #[test]
    fn french_answers_are_spoken_in_french() {
        assert_eq!(speech_lang("C'est fait, j'ai ajouté Stripe pour toi."), Some("fr-FR"));
        assert_eq!(speech_lang("Done, Stripe is in the island."), None);
    }
}
