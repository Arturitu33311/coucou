// Spotify login (OAuth authorization code + PKCE) on a loopback port, and the access token the later player
// pieces use. No client secret: the user's own Spotify app Client ID (settings) is enough. The refresh token
// lives in the OS keystore ("spotify-refresh-token"); the short-lived access token only in memory.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use ring::digest::{digest, SHA256};
use ring::rand::{SecureRandom, SystemRandom};
use tauri::AppHandle;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

pub const REDIRECT: &str = "http://127.0.0.1:43821/callback";
pub const SCOPES: &str =
    "user-read-playback-state user-modify-playback-state user-read-currently-playing user-read-recently-played";

const LISTEN_ADDR: &str = "127.0.0.1:43821";
const TOKEN_URL: &str = "https://accounts.spotify.com/api/token";
const AUTHORIZE_URL: &str = "https://accounts.spotify.com/authorize";
const REFRESH_KEY: &str = "spotify-refresh-token";
const LOGIN_TIMEOUT: Duration = Duration::from_secs(180);
const MAX_ANSWER: usize = 64 * 1024;
const USER_AGENT: &str = concat!("Coucou/", env!("CARGO_PKG_VERSION"));

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthError {
    LoginNeeded,
    Network(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::LoginNeeded => write!(f, "Spotify login needed"),
            AuthError::Network(e) => write!(f, "{e}"),
        }
    }
}

// ── Small encoders (no base64 / percent-encoding crate in the tree) ───────────

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

fn b64url(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = (chunk[0] as u32) << 16
            | (*chunk.get(1).unwrap_or(&0) as u32) << 8
            | *chunk.get(2).unwrap_or(&0) as u32;
        for i in 0..=chunk.len() {
            out.push(B64[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    out
}

fn pct_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn pct_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' => match s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
                Some(v) => {
                    out.push(v);
                    i += 2;
                }
                None => out.push(b'%'),
            },
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

// ── PKCE ──────────────────────────────────────────────────────────────────────

/// The S256 code challenge for a verifier.
pub fn challenge(verifier: &str) -> String {
    b64url(digest(&SHA256, verifier.as_bytes()).as_ref())
}

const PKCE_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";

fn random_string(len: usize) -> String {
    let mut bytes = vec![0u8; len];
    SystemRandom::new().fill(&mut bytes).expect("system randomness");
    bytes.iter().map(|b| PKCE_ALPHABET[(*b as usize) % PKCE_ALPHABET.len()] as char).collect()
}

/// A fresh 64-character PKCE verifier (the modulo bias over 66 symbols is negligible for this use).
pub fn new_verifier() -> String {
    random_string(64)
}

pub fn authorize_url(client_id: &str, challenge: &str, state: &str) -> String {
    format!(
        "{AUTHORIZE_URL}?client_id={}&response_type=code&redirect_uri={}&scope={}&code_challenge_method=S256&code_challenge={}&state={}",
        pct_encode(client_id),
        pct_encode(REDIRECT),
        pct_encode(SCOPES),
        pct_encode(challenge),
        pct_encode(state),
    )
}

/// The authorization code in a `GET /callback?...` request line, checking `state` matches.
pub fn parse_callback(request_line: &str, expected_state: &str) -> Result<String, String> {
    let target = request_line.split_whitespace().nth(1).ok_or("empty request")?;
    let query = target.split_once('?').map(|(_, q)| q).unwrap_or("");
    let (mut code, mut state, mut error) = (None, None, None);
    for pair in query.split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        match k {
            "code" => code = Some(pct_decode(v)),
            "state" => state = Some(pct_decode(v)),
            "error" => error = Some(pct_decode(v)),
            _ => {}
        }
    }
    if state.as_deref() != Some(expected_state) {
        return Err("Spotify login: state mismatch".into());
    }
    if let Some(e) = error {
        return Err(format!("Spotify login refused: {e}"));
    }
    code.filter(|c| !c.is_empty()).ok_or_else(|| "Spotify login: no code in the answer".into())
}

// ── Login ─────────────────────────────────────────────────────────────────────

fn http() -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(8))
        .build()
        .map_err(|e| e.to_string())
}

const DONE_PAGE: &str = "<!doctype html><meta charset=utf-8><title>Coucou</title>\
<body style=\"font-family:sans-serif;text-align:center;margin-top:20vh\"><h2>Listo, ya puedes cerrar esta pesta\u{f1}a</h2></body>";

/// Wait for the browser to hit `/callback` and return its request line. Other paths (favicon...) get a 404.
async fn wait_for_callback(listener: TcpListener) -> Result<String, String> {
    loop {
        let (mut sock, _) = listener.accept().await.map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; 8192];
        let n = match tokio::time::timeout(Duration::from_secs(5), sock.read(&mut buf)).await {
            Ok(Ok(n)) => n,
            _ => continue,
        };
        let text = String::from_utf8_lossy(&buf[..n]);
        let line = text.lines().next().unwrap_or("").to_string();
        let is_callback = line.split_whitespace().nth(1).is_some_and(|t| t.split('?').next() == Some("/callback"));
        let reply = if is_callback {
            let body = DONE_PAGE.as_bytes().len();
            format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {body}\r\nConnection: close\r\n\r\n{DONE_PAGE}"
            )
        } else {
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
        };
        let _ = sock.write_all(reply.as_bytes()).await;
        let _ = sock.shutdown().await;
        if is_callback {
            return Ok(line);
        }
    }
}

pub async fn login(_app: &AppHandle) -> Result<(), String> {
    let client_id = crate::settings::load().spotify_client_id.trim().to_string();
    if client_id.is_empty() {
        return Err("Set your Spotify Client ID (spotifyClientId) first.".into());
    }
    let verifier = new_verifier();
    let state = random_string(24);
    // Bind before the browser opens, so the redirect always finds a listener.
    let listener = TcpListener::bind(LISTEN_ADDR)
        .await
        .map_err(|e| format!("Spotify login: port 43821 is busy ({e})"))?;
    crate::platform::open_url(&authorize_url(&client_id, &challenge(&verifier), &state));
    let line = tokio::time::timeout(LOGIN_TIMEOUT, wait_for_callback(listener))
        .await
        .map_err(|_| "Spotify login timed out".to_string())??;
    let code = parse_callback(&line, &state)?;
    let reply = token_request(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", REDIRECT),
        ("client_id", &client_id),
        ("code_verifier", &verifier),
    ])
    .await
    .map_err(|e| e.to_string())?;
    let refresh = reply.refresh_token.ok_or("Spotify gave no refresh token")?;
    crate::secrets::set_internal(REFRESH_KEY, &refresh)?;
    store_access(&reply.access_token, reply.expires_in);
    Ok(())
}

// ── Tokens ────────────────────────────────────────────────────────────────────

struct TokenReply {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: u64,
}

static CACHE: Mutex<Option<(String, Instant)>> = Mutex::new(None);

fn store_access(token: &str, expires_in: u64) {
    let life = Duration::from_secs(expires_in.saturating_sub(60));
    if let Ok(mut c) = CACHE.lock() {
        *c = Some((token.to_string(), Instant::now() + life));
    }
}

async fn token_request(form: &[(&str, &str)]) -> Result<TokenReply, AuthError> {
    let net = |e: String| AuthError::Network(e);
    let response = http()
        .map_err(net)?
        .post(TOKEN_URL)
        .form(form)
        .send()
        .await
        .map_err(|e| net(e.to_string()))?;
    let status = response.status();
    let body = crate::net::read_capped(response, MAX_ANSWER).await.map_err(net)?;
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap_or(serde_json::Value::Null);
    if !status.is_success() {
        if status.as_u16() == 400 && json["error"] == "invalid_grant" {
            return Err(AuthError::LoginNeeded);
        }
        return Err(net(format!("Spotify token: HTTP {}", status.as_u16())));
    }
    Ok(TokenReply {
        access_token: json["access_token"]
            .as_str()
            .ok_or_else(|| net("Spotify token: no access_token".into()))?
            .to_string(),
        refresh_token: json["refresh_token"].as_str().map(str::to_string),
        expires_in: json["expires_in"].as_u64().unwrap_or(3600),
    })
}

/// A valid access token: the cached one, or a fresh one from the stored refresh token.
pub async fn access_token() -> Result<String, AuthError> {
    if let Ok(c) = CACHE.lock() {
        if let Some((token, until)) = c.as_ref() {
            if Instant::now() < *until {
                return Ok(token.clone());
            }
        }
    }
    let refresh = crate::secrets::get_internal(REFRESH_KEY)
        .filter(|r| !r.is_empty())
        .ok_or(AuthError::LoginNeeded)?;
    let client_id = crate::settings::load().spotify_client_id.trim().to_string();
    if client_id.is_empty() {
        return Err(AuthError::LoginNeeded);
    }
    let reply = token_request(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", &refresh),
        ("client_id", &client_id),
    ])
    .await?;
    if let Some(new) = reply.refresh_token.as_deref().filter(|n| *n != refresh) {
        let _ = crate::secrets::set_internal(REFRESH_KEY, new);
    }
    store_access(&reply.access_token, reply.expires_in);
    Ok(reply.access_token)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn challenge_matches_rfc7636_vector() {
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn verifier_is_64_unreserved_chars() {
        let v = new_verifier();
        assert_eq!(v.len(), 64);
        assert!(v.chars().all(|c| c.is_ascii_alphanumeric() || "-._~".contains(c)));
        assert_ne!(v, new_verifier());
    }

    #[test]
    fn authorize_url_has_pkce_params() {
        let u = authorize_url("cid", "chal", "st");
        assert!(u.starts_with("https://accounts.spotify.com/authorize?"));
        for part in [
            "client_id=cid",
            "response_type=code",
            "code_challenge_method=S256",
            "code_challenge=chal",
            "state=st",
            "redirect_uri=http%3A%2F%2F127.0.0.1%3A43821%2Fcallback",
            "scope=user-read-playback-state%20user-modify-playback-state%20user-read-currently-playing%20user-read-recently-played",
        ] {
            assert!(u.contains(part), "{part} missing from {u}");
        }
    }

    #[test]
    fn parse_callback_ok() {
        let line = "GET /callback?code=AbC%2Fd&state=xyz HTTP/1.1";
        assert_eq!(parse_callback(line, "xyz").unwrap(), "AbC/d");
    }

    #[test]
    fn parse_callback_rejects_wrong_state() {
        assert!(parse_callback("GET /callback?code=a&state=evil HTTP/1.1", "xyz").is_err());
        assert!(parse_callback("GET /callback?code=a HTTP/1.1", "xyz").is_err());
    }

    #[test]
    fn parse_callback_reports_access_denied() {
        let e = parse_callback("GET /callback?error=access_denied&state=xyz HTTP/1.1", "xyz").unwrap_err();
        assert!(e.contains("access_denied"), "{e}");
    }

    #[test]
    fn base64url_has_no_padding() {
        assert_eq!(b64url(b"f"), "Zg");
        assert_eq!(b64url(b"fo"), "Zm8");
        assert_eq!(b64url(b"foo"), "Zm9v");
        assert_eq!(b64url(&[0xfb, 0xff]), "-_8");
    }

    #[test]
    fn percent_round_trip() {
        assert_eq!(pct_encode("a b/c~-._"), "a%20b%2Fc~-._");
        assert_eq!(pct_decode("a%20b%2Fc+d"), "a b/c d");
        assert_eq!(pct_decode("100%"), "100%");
    }
}
