// What travels between the laptop and the phone for the audio relay, with no I/O.
//
// Laptop → phone: frames `[type: u8][len: u16 big-endian][payload]`.
//   1 audio      PCM s16le, 48000 Hz, 2 channels; at most `MAX_AUDIO` bytes
//   2 heartbeat  no payload
//   3 hello      JSON describing the audio; at most `MAX_HELLO` bytes
//   4 pause      the computer asks the phone to pause its media; no payload
//   5 swap       the computer asks the phone to prompt about the headphones; one byte (1 release, 2 connect)
// Phone → laptop: one JSON object per line, at most `MAX_LINE` bytes:
//   {"op":"hb"}
//   {"op":"state","headphones":"phone"|"pc"|"none","call":bool,"phone_playing":bool}
//   {"op":"now","title":s,"artist":s,"album":s,"status":"Playing"|"Paused"|"Stopped","pos":ms,"len":ms}  (hello says "now":1)
//   {"op":"art","hash":s,"b64":s}  the cover as base64, at most `MAX_ART` bytes decoded; empty `b64` = no cover
//
// Whatever arrives is checked here before anything is allocated or acted on: a bad length or an unknown
// type is an error and the connection is dropped by the caller. The phone's Kotlin twin is
// `AudioLinkWire.kt`; both read `scripts/phone/audiolink-vectors.json`.

use crate::audiolink_logic::Where;

pub const MAX_AUDIO: usize = 8192;
pub const MAX_HELLO: usize = 256;
pub const MAX_LINE: usize = 512;
/// A `now` line (titles and names, cut by the phone to fit).
pub const MAX_NOW_LINE: usize = 1536;
/// The largest cover, decoded, and the longest `art` line that can carry it (base64 plus the JSON around it).
pub const MAX_ART: usize = 65536;
pub const MAX_ART_LINE: usize = 90_000;
/// What the phone is told the audio is. `now` says the computer understands the phone's now-playing lines (an older
/// computer drops the link at any op it does not know, so the phone sends them only when it reads this).
pub const HELLO_JSON: &str = r#"{"rate":48000,"channels":2,"fmt":"s16le","now":1}"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    Audio = 1,
    Heartbeat = 2,
    Hello = 3,
    /// The computer asks the phone to pause what it is playing. No payload.
    PausePhone = 4,
    /// The computer asks the phone to prompt the person about the headphones: one byte, `SWAP_RELEASE` or `SWAP_CONNECT`.
    SwapPrompt = 5,
}

/// Prompt: "let go of the headphones" (they are about to move to the computer).
pub const SWAP_RELEASE: u8 = 1;
/// Prompt: "connect the headphones to this phone" (they are coming back).
pub const SWAP_CONNECT: u8 = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub kind: FrameKind,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WireError {
    UnknownType(u8),
    TooLong,
    HeartbeatWithPayload,
    BadLine,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PhoneMsg {
    Heartbeat,
    State { headphones: Where, call: bool, phone_playing: bool },
    /// What the phone is playing (empty title and `Stopped` when nothing).
    Now { title: String, artist: String, album: String, status: String, pos_ms: i64, len_ms: i64 },
    /// The cover of what the phone is playing (an image, or nothing at all).
    Art { hash: String, image: Vec<u8> },
}

/// Standard base64 (RFC 4648, padded); `None` when the text is not exactly that.
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let b = text.as_bytes();
    if b.len() % 4 != 0 {
        return None;
    }
    let val = |c: u8| -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some((c - b'A') as u32),
            b'a'..=b'z' => Some((c - b'a') as u32 + 26),
            b'0'..=b'9' => Some((c - b'0') as u32 + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    };
    let mut out = Vec::with_capacity(b.len() / 4 * 3);
    for (i, q) in b.chunks(4).enumerate() {
        let last = (i + 1) * 4 == b.len();
        let pad = if last { q.iter().rev().take_while(|c| **c == b'=').count() } else { 0 };
        if pad > 2 {
            return None;
        }
        let mut n = 0u32;
        for (k, c) in q.iter().enumerate() {
            n <<= 6;
            if k < 4 - pad {
                n |= val(*c)?;
            }
        }
        out.push((n >> 16) as u8);
        if pad < 2 {
            out.push((n >> 8) as u8);
        }
        if pad < 1 {
            out.push(n as u8);
        }
    }
    Some(out)
}

/// One frame, ready to write.
pub fn encode(kind: FrameKind, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(3 + payload.len());
    out.push(kind as u8);
    out.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    out.extend_from_slice(payload);
    out
}

/// Reads the first frame in `buf`. `Ok(None)` means more bytes are needed; `Ok(Some((frame, n)))` means
/// `n` bytes were used. A header that cannot be valid is an error as soon as the header is complete.
pub fn decode(buf: &[u8]) -> Result<Option<(Frame, usize)>, WireError> {
    if buf.len() < 3 {
        return Ok(None);
    }
    let (kind, max) = match buf[0] {
        1 => (FrameKind::Audio, MAX_AUDIO),
        2 => (FrameKind::Heartbeat, 0),
        3 => (FrameKind::Hello, MAX_HELLO),
        4 => (FrameKind::PausePhone, 0),
        5 => (FrameKind::SwapPrompt, 1),
        other => return Err(WireError::UnknownType(other)),
    };
    let len = u16::from_be_bytes([buf[1], buf[2]]) as usize;
    if len > max {
        return Err(if max == 0 { WireError::HeartbeatWithPayload } else { WireError::TooLong });
    }
    if buf.len() < 3 + len {
        return Ok(None);
    }
    Ok(Some((Frame { kind, payload: buf[3..3 + len].to_vec() }, 3 + len)))
}

/// One line from the phone (without its newline).
pub fn parse_phone_line(line: &str) -> Result<PhoneMsg, WireError> {
    if line.len() > MAX_ART_LINE {
        return Err(WireError::BadLine);
    }
    let v: serde_json::Value = serde_json::from_str(line).map_err(|_| WireError::BadLine)?;
    // The cap depends on the op, so the cheap ops cannot be used to push a big line through.
    let cap = match v["op"].as_str() {
        Some("art") => MAX_ART_LINE,
        Some("now") => MAX_NOW_LINE,
        _ => MAX_LINE,
    };
    if line.len() > cap {
        return Err(WireError::BadLine);
    }
    match v["op"].as_str() {
        Some("now") => {
            let text = |k: &str| v[k].as_str().map(str::to_string).ok_or(WireError::BadLine);
            let status = text("status")?;
            if !matches!(status.as_str(), "Playing" | "Paused" | "Stopped") {
                return Err(WireError::BadLine);
            }
            let ms = |k: &str| v[k].as_i64().filter(|n| *n >= 0).ok_or(WireError::BadLine);
            Ok(PhoneMsg::Now { title: text("title")?, artist: text("artist")?, album: text("album")?, status, pos_ms: ms("pos")?, len_ms: ms("len")? })
        }
        Some("art") => {
            let hash = v["hash"].as_str().ok_or(WireError::BadLine)?;
            if hash.len() > 64 || !hash.bytes().all(|c| c.is_ascii_alphanumeric()) {
                return Err(WireError::BadLine);
            }
            let image = base64_decode(v["b64"].as_str().ok_or(WireError::BadLine)?).ok_or(WireError::BadLine)?;
            if image.len() > MAX_ART {
                return Err(WireError::BadLine);
            }
            Ok(PhoneMsg::Art { hash: hash.to_string(), image })
        }
        Some("hb") => Ok(PhoneMsg::Heartbeat),
        Some("state") => {
            let headphones = match v["headphones"].as_str() {
                Some("phone") => Where::Phone,
                Some("pc") => Where::Pc,
                Some("none") => Where::None,
                _ => return Err(WireError::BadLine),
            };
            let call = v["call"].as_bool().ok_or(WireError::BadLine)?;
            let phone_playing = v["phone_playing"].as_bool().ok_or(WireError::BadLine)?;
            Ok(PhoneMsg::State { headphones, call, phone_playing })
        }
        _ => Err(WireError::BadLine),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    const VECTORS: &str = include_str!("../../scripts/phone/audiolink-vectors.json");

    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    #[test]
    fn every_shared_wire_case_holds() {
        let doc: Value = serde_json::from_str(VECTORS).unwrap();
        let cases = doc["wire_cases"].as_array().unwrap();
        assert!(cases.len() >= 10, "the vector file lost its wire cases");
        for c in cases {
            let name = c["name"].as_str().unwrap();
            let got = decode(&unhex(c["hex"].as_str().unwrap()));
            let consumed = c["consumed"].as_u64().unwrap() as usize;
            match (c["result"].as_str().unwrap(), got) {
                ("audio", Ok(Some((f, n)))) => assert_eq!((f.kind, n), (FrameKind::Audio, consumed), "{name}"),
                ("heartbeat", Ok(Some((f, n)))) => assert_eq!((f.kind, n), (FrameKind::Heartbeat, consumed), "{name}"),
                ("swap_prompt", Ok(Some((f, n)))) => {
                    assert_eq!((f.kind, n), (FrameKind::SwapPrompt, consumed), "{name}");
                    assert_eq!(f.payload.len(), 1, "{name}");
                }
                ("pause_phone", Ok(Some((f, n)))) => assert_eq!((f.kind, n), (FrameKind::PausePhone, consumed), "{name}"),
                ("hello", Ok(Some((f, n)))) => {
                    assert_eq!((f.kind, n), (FrameKind::Hello, consumed), "{name}");
                    assert_eq!(f.payload, HELLO_JSON.as_bytes(), "{name}");
                }
                ("need_more", Ok(None)) => {}
                ("error", Err(_)) => {}
                (want, got) => panic!("{name}: wanted {want}, got {got:?}"),
            }
        }
    }

    #[test]
    fn every_shared_line_case_holds() {
        let doc: Value = serde_json::from_str(VECTORS).unwrap();
        for c in doc["line_cases"].as_array().unwrap() {
            let name = c["name"].as_str().unwrap();
            let got = parse_phone_line(c["line"].as_str().unwrap());
            match (c["result"].as_str().unwrap(), got) {
                ("heartbeat", Ok(PhoneMsg::Heartbeat)) => {}
                ("state", Ok(PhoneMsg::State { headphones, call, phone_playing })) => {
                    let want = match c["headphones"].as_str().unwrap() {
                        "phone" => Where::Phone,
                        "pc" => Where::Pc,
                        _ => Where::None,
                    };
                    assert_eq!(headphones, want, "{name}");
                    assert_eq!(call, c["call"].as_bool().unwrap(), "{name}");
                    assert_eq!(phone_playing, c["phone_playing"].as_bool().unwrap(), "{name}");
                }
                ("now", Ok(PhoneMsg::Now { title, status, pos_ms, len_ms, .. })) => {
                    assert_eq!(title, c["title"].as_str().unwrap(), "{name}");
                    assert_eq!(status, c["status"].as_str().unwrap(), "{name}");
                    assert_eq!(pos_ms, c["pos"].as_i64().unwrap(), "{name}");
                    assert_eq!(len_ms, c["len"].as_i64().unwrap(), "{name}");
                }
                ("art", Ok(PhoneMsg::Art { hash, image })) => {
                    assert_eq!(hash, c["hash"].as_str().unwrap(), "{name}");
                    assert_eq!(image.len(), c["bytes"].as_u64().unwrap() as usize, "{name}");
                }
                ("error", Err(_)) => {}
                (want, got) => panic!("{name}: wanted {want}, got {got:?}"),
            }
        }
    }

    #[test]
    fn base64_matches_the_standard_cases() {
        assert_eq!(base64_decode(""), Some(vec![]));
        assert_eq!(base64_decode("Zg=="), Some(b"f".to_vec()));
        assert_eq!(base64_decode("Zm8="), Some(b"fo".to_vec()));
        assert_eq!(base64_decode("Zm9v"), Some(b"foo".to_vec()));
        assert_eq!(base64_decode("Zm9vYg=="), Some(b"foob".to_vec()));
        assert_eq!(base64_decode("Zm9"), None);
        assert_eq!(base64_decode("Zm9*"), None);
        assert_eq!(base64_decode("Z==="), None);
    }

    #[test]
    fn a_cover_of_exactly_the_limit_passes_and_one_more_byte_does_not() {
        let b64 = |n: usize| {
            const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
            let bytes = vec![0xFFu8; n];
            let mut s = String::new();
            for ch in bytes.chunks(3) {
                let v = ((ch[0] as u32) << 16) | ((*ch.get(1).unwrap_or(&0) as u32) << 8) | *ch.get(2).unwrap_or(&0) as u32;
                for i in 0..4 {
                    if i <= ch.len() { s.push(T[((v >> (18 - 6 * i)) & 63) as usize] as char) } else { s.push('=') }
                }
            }
            s
        };
        let line = |n| format!(r#"{{"op":"art","hash":"h","b64":"{}"}}"#, b64(n));
        assert!(matches!(parse_phone_line(&line(MAX_ART)), Ok(PhoneMsg::Art { .. })));
        assert!(line(MAX_ART).len() <= MAX_ART_LINE);
        assert_eq!(parse_phone_line(&line(MAX_ART + 1)), Err(WireError::BadLine));
    }

    #[test]
    fn only_the_cover_line_may_be_long() {
        let pad = "x".repeat(2000);
        assert_eq!(parse_phone_line(&format!(r#"{{"op":"state","pad":"{pad}"}}"#)), Err(WireError::BadLine));
        assert_eq!(parse_phone_line(&format!(r#"{{"op":"now","title":"{pad}","artist":"","album":"","status":"Playing","pos":0,"len":0}}"#)), Err(WireError::BadLine));
    }

    #[test]
    fn encode_then_decode_gives_the_same_frame() {
        let bytes = encode(FrameKind::Audio, &[9, 8, 7]);
        assert_eq!(bytes, vec![1, 0, 3, 9, 8, 7]);
        assert_eq!(decode(&bytes), Ok(Some((Frame { kind: FrameKind::Audio, payload: vec![9, 8, 7] }, 6))));
    }

    #[test]
    fn an_audio_frame_of_exactly_the_limit_decodes_and_one_more_does_not() {
        let ok = encode(FrameKind::Audio, &vec![0u8; MAX_AUDIO]);
        assert!(matches!(decode(&ok), Ok(Some((_, n))) if n == 3 + MAX_AUDIO));
        let mut over = vec![1u8];
        over.extend_from_slice(&((MAX_AUDIO + 1) as u16).to_be_bytes());
        assert_eq!(decode(&over), Err(WireError::TooLong));
    }

    #[test]
    fn phone_line_rejects_over_512_bytes() {
        let line = format!(r#"{{"op":"hb","pad":"{}"}}"#, "x".repeat(MAX_LINE));
        assert_eq!(parse_phone_line(&line), Err(WireError::BadLine));
    }
}
