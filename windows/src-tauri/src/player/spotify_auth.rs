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

use crate::i18n::{t, tf};

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

pub(crate) fn pct_encode(s: &str) -> String {
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

fn random_string(len: usize) -> Result<String, String> {
    let mut bytes = vec![0u8; len];
    SystemRandom::new().fill(&mut bytes).map_err(|_| "no system randomness".to_string())?;
    Ok(bytes.iter().map(|b| PKCE_ALPHABET[(*b as usize) % PKCE_ALPHABET.len()] as char).collect())
}

/// A fresh 64-character PKCE verifier (the modulo bias over 66 symbols is negligible for this use).
pub fn new_verifier() -> Result<String, String> {
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
        return Err(tf("Spotify sign-in was refused: {error}", &[("error", &e)]));
    }
    code.filter(|c| !c.is_empty()).ok_or_else(|| t("Spotify sign-in gave no code"))
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

const FAIL_PAGE: &str = "<!doctype html><meta charset=utf-8><title>Coucou</title>\
<body style=\"font-family:sans-serif;text-align:center;margin-top:20vh\"><h2>No se pudo iniciar sesi\u{f3}n con Spotify</h2></body>";
const NEUTRAL_PAGE: &str = "<!doctype html><meta charset=utf-8><title>Coucou</title><body></body>";

fn html_reply(status: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// Serve the loopback port until a `/callback` request carrying the right `state` arrives; return the code, or
/// the error Spotify reported. Other paths (favicon...) get a 404 and a wrong-state request a neutral page, and
/// both keep waiting, so a stray request cannot end the login.
async fn wait_for_callback(listener: TcpListener, state: &str) -> Result<String, String> {
    loop {
        let (mut sock, _) = listener.accept().await.map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; 8192];
        let n = match tokio::time::timeout(Duration::from_secs(5), sock.read(&mut buf)).await {
            Ok(Ok(n)) => n,
            _ => continue,
        };
        let text = String::from_utf8_lossy(&buf[..n]);
        let line = text.lines().next().unwrap_or("");
        let is_callback = line.split_whitespace().nth(1).is_some_and(|t| t.split('?').next() == Some("/callback"));
        let mut outcome = None;
        let reply = if !is_callback {
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string()
        } else {
            match parse_callback(line, state) {
                Ok(code) => {
                    outcome = Some(Ok(code));
                    html_reply("200 OK", DONE_PAGE)
                }
                Err(e) if e.contains("state mismatch") => html_reply("400 Bad Request", NEUTRAL_PAGE),
                Err(e) => {
                    outcome = Some(Err(e));
                    html_reply("200 OK", FAIL_PAGE)
                }
            }
        };
        let _ = sock.write_all(reply.as_bytes()).await;
        let _ = sock.shutdown().await;
        if let Some(done) = outcome {
            return done;
        }
    }
}

pub async fn login(_app: &AppHandle) -> Result<(), String> {
    let client_id = crate::settings::load().spotify_client_id.trim().to_string();
    if client_id.is_empty() {
        return Err(t("Set your Spotify Client ID (spotifyClientId) first."));
    }
    let verifier = new_verifier()?;
    let state = random_string(24)?;
    // Bind before the browser opens, so the redirect always finds a listener.
    let listener = TcpListener::bind(LISTEN_ADDR).await.map_err(|e| {
        let port = LISTEN_ADDR.rsplit(':').next().unwrap_or(LISTEN_ADDR);
        tf("Spotify sign-in: port {port} is busy ({error})", &[("port", port), ("error", &e.to_string())])
    })?;
    crate::platform::open_url(&authorize_url(&client_id, &challenge(&verifier), &state));
    let code = tokio::time::timeout(LOGIN_TIMEOUT, wait_for_callback(listener, &state))
        .await
        .map_err(|_| t("Spotify sign-in timed out"))??;
    let reply = token_request(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", REDIRECT),
        ("client_id", &client_id),
        ("code_verifier", &verifier),
    ])
    .await
    .map_err(|e| e.to_string())?;
    let refresh = reply.refresh_token.ok_or_else(|| t("Spotify gave no refresh token"))?;
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

static REFRESH_LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();

/// Drops the cached access token so the next `access_token()` refreshes. For a 401: the API rejected a token
/// we still thought was valid.
pub fn invalidate_cached_token() {
    if let Ok(mut c) = CACHE.lock() {
        *c = None;
    }
}

fn cached() -> Option<String> {
    let c = CACHE.lock().ok()?;
    let (token, until) = c.as_ref()?;
    (Instant::now() < *until).then(|| token.clone())
}

/// The cached token, or the one `refresh` fetches. Single-flight: concurrent callers wait on one refresh and
/// then find its result in the cache.
async fn token_or_refresh<F, Fut>(refresh: F) -> Result<String, AuthError>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(String, u64), AuthError>>,
{
    if let Some(t) = cached() {
        return Ok(t);
    }
    let _flight = REFRESH_LOCK.get_or_init(|| tokio::sync::Mutex::new(())).lock().await;
    if let Some(t) = cached() {
        return Ok(t);
    }
    let (token, expires_in) = refresh().await?;
    store_access(&token, expires_in);
    Ok(token)
}

async fn refresh_from_keystore() -> Result<(String, u64), AuthError> {
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
        if let Err(e) = crate::secrets::set_internal(REFRESH_KEY, new) {
            // Never the token itself. The access token still works; the next login may be needed sooner.
            eprintln!("spotify: could not store the rotated refresh token: {e}");
        }
    }
    Ok((reply.access_token, reply.expires_in))
}

/// A valid access token: the cached one, or a fresh one from the stored refresh token.
pub async fn access_token() -> Result<String, AuthError> {
    token_or_refresh(refresh_from_keystore).await
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The token cache is a process-wide static: tests that touch it run one at a time.
    static CACHE_TESTS: Mutex<()> = Mutex::new(());

    #[test]
    fn challenge_matches_rfc7636_vector() {
        assert_eq!(
            challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn verifier_is_64_unreserved_chars() {
        let v = new_verifier().unwrap();
        assert_eq!(v.len(), 64);
        assert!(v.chars().all(|c| c.is_ascii_alphanumeric() || "-._~".contains(c)));
        assert_ne!(v, new_verifier().unwrap());
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

    async fn get(addr: std::net::SocketAddr, path: &str) -> String {
        let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
        s.write_all(format!("GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes()).await.unwrap();
        let mut out = String::new();
        s.read_to_string(&mut out).await.unwrap();
        out
    }

    #[tokio::test]
    async fn a_wrong_state_request_does_not_end_the_login() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let waiter = tokio::spawn(async move { wait_for_callback(listener, "good").await });
        assert!(get(addr, "/favicon.ico").await.contains("404"));
        let bad = get(addr, "/callback?code=evil&state=bad").await;
        assert!(!bad.contains("Listo"), "{bad}");
        assert!(!waiter.is_finished());
        let ok = get(addr, "/callback?code=real&state=good").await;
        assert!(ok.contains("Listo, ya puedes cerrar esta pesta"), "{ok}");
        assert_eq!(waiter.await.unwrap().unwrap(), "real");
    }

    #[tokio::test]
    async fn a_refusal_gets_the_failure_page_not_the_success_one() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let waiter = tokio::spawn(async move { wait_for_callback(listener, "good").await });
        let page = get(addr, "/callback?error=access_denied&state=good").await;
        assert!(!page.contains("Listo"), "{page}");
        assert!(waiter.await.unwrap().unwrap_err().contains("access_denied"));
    }

    #[tokio::test]
    async fn concurrent_callers_share_one_refresh() {
        let _serial = CACHE_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        *CACHE.lock().unwrap() = None;
        let calls = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for _ in 0..8 {
            let calls = calls.clone();
            tasks.push(tokio::spawn(token_or_refresh(move || async move {
                calls.fetch_add(1, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(30)).await;
                Ok(("tok".to_string(), 3600))
            })));
        }
        for t in tasks {
            assert_eq!(t.await.unwrap().unwrap(), "tok");
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        *CACHE.lock().unwrap() = None;
    }

    #[test]
    fn invalidating_the_cache_forces_the_next_call_to_refresh() {
        let _serial = CACHE_TESTS.lock().unwrap_or_else(|e| e.into_inner());
        store_access("old", 3600);
        assert_eq!(cached().as_deref(), Some("old"));
        invalidate_cached_token();
        assert_eq!(cached(), None);
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
