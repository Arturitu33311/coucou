// What travels between the laptop and the phone for the audio relay, with no I/O.
//
// Laptop → phone: frames `[type: u8][len: u16 big-endian][payload]`.
//   1 audio      PCM s16le, 48000 Hz, 2 channels; at most `MAX_AUDIO` bytes
//   2 heartbeat  no payload
//   3 hello      JSON describing the audio; at most `MAX_HELLO` bytes
//   4 pause      the computer asks the phone to pause its media; no payload
// Phone → laptop: one JSON object per line, at most `MAX_LINE` bytes:
//   {"op":"hb"}
//   {"op":"state","headphones":"phone"|"pc"|"none","call":bool,"phone_playing":bool}
//
// Whatever arrives is checked here before anything is allocated or acted on: a bad length or an unknown
// type is an error and the connection is dropped by the caller. The phone's Kotlin twin is
// `AudioLinkWire.kt`; both read `scripts/phone/audiolink-vectors.json`.

use crate::audiolink_logic::Where;

pub const MAX_AUDIO: usize = 8192;
pub const MAX_HELLO: usize = 256;
pub const MAX_LINE: usize = 512;
/// What the phone is told the audio is.
pub const HELLO_JSON: &str = r#"{"rate":48000,"channels":2,"fmt":"s16le"}"#;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameKind {
    Audio = 1,
    Heartbeat = 2,
    Hello = 3,
    /// The computer asks the phone to pause what it is playing. No payload.
    PausePhone = 4,
}

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
    if line.len() > MAX_LINE {
        return Err(WireError::BadLine);
    }
    let v: serde_json::Value = serde_json::from_str(line).map_err(|_| WireError::BadLine)?;
    match v["op"].as_str() {
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
                ("error", Err(_)) => {}
                (want, got) => panic!("{name}: wanted {want}, got {got:?}"),
            }
        }
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
