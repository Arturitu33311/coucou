# Reproductor de cristal — plan de implementación

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Una vista nueva de la isla, "player", para buscar y poner canciones de Spotify y YT Music, con recientes, DJ y página de artista con cover flow.

**Architecture:** Un módulo Rust nuevo `src-tauri/src/player/` (tipos, historial, OAuth PKCE y Web API de Spotify, elección de dispositivo B/A, YT Music por un ayudante Python local, mpv por IPC) expuesto como comandos Tauri `player_*`. Del lado TS, una vista `player` (`src/views/player.ts`) que reusa el tinte, las barras y el estado de música existentes. Lo que suena se sigue leyendo por MPRIS como hoy (`music.rs`).

**Tech Stack:** Tauri 2, Rust (reqwest 0.12 rustls, tokio, ring, keyring), TypeScript sin framework, Python 3 + ytmusicapi (venv local), mpv + mpv-mpris + yt-dlp, spotifyd.

**Spec:** `docs/superpowers/specs/2026-11-08-reproductor-cristal-design.md`

Todas las rutas son relativas a `windows/` salvo que se diga otra cosa.

## Global Constraints

- Reglas de `CLAUDE.md` raíz: sin telemetría; red solo a servicios configurados; vistas existentes sin cambios de aspecto (en "music" solo se agrega el botón lupa); 0 % CPU con la isla oculta; secretos nunca en disco ni en git.
- Redirect URI exacta: `http://127.0.0.1:43821/callback`. Scopes: `user-read-playback-state user-modify-playback-state user-read-currently-playing user-read-recently-played`.
- Spotify Development Mode: `GET /search` con `limit` ≤ 10 (paginar con `offset`); sin `artists/{id}/top-tracks`; sin `GET /artists` ni `GET /albums` por lotes.
- Client ID: **no va en el repo** (repo público). Campo de ajustes `spotifyClientId`, por defecto `""`; lo escribe el script de preparación en la laptop.
- Refresh token de Spotify: en el llavero, clave interna `spotify-refresh-token` (`secrets::INTERNAL_KEYS`), nunca accesible desde el webview.
- Dispositivo Spotify: B (spotifyd, nombre de dispositivo `"Coucou"`) principal; A (app oficial `com.spotify.Client`, ventana minimizada) de respaldo.
- YT Music y mpv solo en Linux (`#[cfg(target_os = "linux")]`); en Windows la vista oculta la fuente YT Music. No romper `cargo build` de Windows.
- Todo el texto visible con `t()`/`tl()` y su traducción en `src/i18n/extra.json` (los 9 idiomas).
- Vista "player": `VIEW_LAYOUTS.player = { height: 320, botX: 0, botY: null, botDiameter: 0, agentMode: "none" }` (320 = alto máximo del panel; decisión de Claude).
- Verificación de cada tarea: `cd windows && npx tsc --noEmit`, `cargo test -p coucou`, y `~/.nvm/versions/node/v22.23.3/bin/node --import ./tests/setup.mjs --test <archivos>` (Node del sistema es 20).
- Ramificar desde `origin/unify/monorepo`. Commits pequeños, uno por tarea como mínimo.

## Review Focus

1. **Tecleo rápido en la búsqueda:** respuestas viejas que llegan después de una nueva no deben pisar los resultados (número de secuencia por consulta). → Tarea 8, test `staleResultsAreDropped`.
2. **spotifyd ausente o caído:** pasar al respaldo A en ≤ 10 s, sin colgar la isla, y avisar que se usó el respaldo. → Tarea 4, test `falls_back_to_app_when_coucou_device_missing`.
3. **YT Music roto (ytmusicapi o yt-dlp desactualizados):** error claro "YT Music no responde" y la isla sigue respondiendo (timeout del ayudante). → Tarea 5, test `helper_timeout_is_an_error_not_a_hang`.
4. **Token vencido o revocado:** refrescar una vez; si falla, la vista vuelve a mostrar "Conectar Spotify". → Tarea 3, test `unauthorized_after_refresh_reports_login_needed`.
5. **Isla oculta:** sin sondeos; las barras se piden solo con la vista visible; mpv inactivo no se reinicia en bucle. → Tarea 8, test `barsWantedOnlyWhilePlayerVisible`.

---

### Task 1: Tipos compartidos e historial local

**Files:**
- Create: `src-tauri/src/player/mod.rs`, `src-tauri/src/player/history.rs`
- Modify: `src-tauri/src/lib.rs` (`mod player;` junto a `mod music;`)

**Interfaces:**
- Produces (en `player/mod.rs`):
  - `#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)] #[serde(rename_all="camelCase")] pub enum Source { Spotify, Ytmusic }`
  - `pub struct Item { pub source: Source, pub kind: Kind, pub id: String, pub title: String, pub subtitle: String, pub art_url: String }` (camelCase)
  - `pub enum Kind { Track, Artist, Album, Context }` (camelCase; `Context` = DJ/lista)
  - `history::path() -> PathBuf` = `settings::local_dir().join("player-history.json")`
  - `history::push(list: &mut Vec<HistoryEntry>, item: Item, at_ms: i64)` — más nuevo primero, sin duplicados por `(source,id)`, máximo `HISTORY_MAX = 50`
  - `history::load() -> Vec<HistoryEntry>`, `history::save(&[HistoryEntry]) -> Result<(), String>` (con `platform::ensure_private_dir`)
  - `pub struct HistoryEntry { pub item: Item, pub at_ms: i64 }`

- [ ] **Step 1: Tests en `history.rs` (`mod tests`)**: `push_puts_newest_first`, `push_dedupes_same_source_and_id` (mismo id en otra fuente NO se deduplica), `push_caps_at_50`, `roundtrip_json` (serializa/deserializa un `Vec<HistoryEntry>` igual).
- [ ] **Step 2:** `cargo test player::history` → FAIL (no compila).
- [ ] **Step 3:** Implementar tipos y funciones.
- [ ] **Step 4:** `cargo test player::history` → PASS.
- [ ] **Step 5:** Commit `feat(player): shared types and local play history`.

### Task 2: OAuth PKCE de Spotify con escucha de loopback

**Files:**
- Create: `src-tauri/src/player/spotify_auth.rs`
- Modify: `src-tauri/src/secrets.rs` (agregar `"spotify-refresh-token"` a `INTERNAL_KEYS`), `src-tauri/src/settings.rs` + `src/core/state.ts` (campo `spotifyClientId: String`, default `""`)

**Interfaces:**
- Produces:
  - `pub const REDIRECT: &str = "http://127.0.0.1:43821/callback";` `pub const SCOPES: &str = "<los 4 scopes>";`
  - `pub fn challenge(verifier: &str) -> String` (S256, base64url sin relleno; `ring::digest`)
  - `pub fn new_verifier() -> String` (64 chars del alfabeto PKCE, `ring::rand::SystemRandom`)
  - `pub fn authorize_url(client_id: &str, challenge: &str, state: &str) -> String`
  - `pub fn parse_callback(request_line: &str, expected_state: &str) -> Result<String /*code*/, String>`
  - `pub async fn login(app: &AppHandle) -> Result<(), String>`: lee `spotifyClientId` (error claro si vacío), abre el navegador, escucha **una** petición en `127.0.0.1:43821` (timeout 180 s), responde una página corta "Listo, ya puedes cerrar esta pestaña", canjea el code en `https://accounts.spotify.com/api/token`, guarda refresh token con `secrets::set_internal`.
  - `pub async fn access_token() -> Result<String, AuthError>` — caché en memoria hasta `expires_in - 60 s`; refresca con el refresh token (y guarda el nuevo si Spotify lo rota).
  - `pub enum AuthError { LoginNeeded, Network(String) }`

- [ ] **Step 1: Tests**: `challenge_matches_rfc7636_vector` (verifier `dBjftJeZ4CVP-mJ92K9qnyhsNC0mH7bvEbZXJ1qE-rU` → `E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM`); `verifier_is_64_unreserved_chars`; `authorize_url_has_pkce_params` (contiene `code_challenge_method=S256`, `redirect_uri=http%3A%2F%2F127.0.0.1%3A43821%2Fcallback`, `response_type=code`, los scopes codificados); `parse_callback_ok`, `parse_callback_rejects_wrong_state`, `parse_callback_reports_access_denied` (`?error=access_denied`).
- [ ] **Step 2:** `cargo test player::spotify_auth` → FAIL.
- [ ] **Step 3:** Implementar. El canje de token es POST form (`grant_type`, `code`, `redirect_uri`, `client_id`, `code_verifier`); cliente reqwest con timeout 8 s y `net::read_capped`.
- [ ] **Step 4:** Tests → PASS; `npx tsc --noEmit` → OK.
- [ ] **Step 5:** Commit `feat(player): Spotify PKCE login on a loopback port`.

### Task 3: Cliente de la Web API de Spotify

**Files:**
- Create: `src-tauri/src/player/spotify_api.rs`, fixtures `src-tauri/src/player/fixtures/{search,artist,artist_albums,album,devices,recent}.json` (respuestas reales recortadas, formato feb-2026)

**Interfaces:**
- Consumes: `spotify_auth::access_token`, `Item`, `Kind`, `Source`.
- Produces (parsers puros + funciones async que los usan):
  - `pub fn parse_search(json: &Value) -> Vec<Item>` (tracks y artists; `subtitle` = artistas unidos con ", "; `art_url` = imagen más chica ≥ 64 px)
  - `pub fn parse_artist_albums(json) -> Vec<Item>`, `pub fn parse_album_tracks(json, album_art: &str) -> Vec<Item>`, `pub fn parse_devices(json) -> Vec<Device>`, `pub fn parse_recent(json) -> Vec<HistoryEntry>`
  - `pub struct Device { pub id: String, pub name: String, pub kind: String, pub active: bool }`
  - `pub async fn search(q: &str, offset: u32) -> Result<Vec<Item>, ApiError>` (`type=track,artist&limit=10`)
  - `pub async fn artist_tracks(name: &str) -> Result<Vec<Item>, ApiError>` (búsqueda `artist:"<name>"`, sustituto de top-tracks)
  - `pub async fn artist_albums(id) / album_tracks(id) / devices() / recently_played()`
  - `pub async fn play(device_id: &str, target: &PlayTarget) -> Result<(), ApiError>` con `pub enum PlayTarget { Track(String /*uri*/), Context(String /*uri*/) }`
  - `pub async fn pause()`
  - `pub enum ApiError { LoginNeeded, RateLimited { retry_after_s: u64 }, NoDevice, Other(String) }`
- Caché en memoria de las últimas 20 búsquedas por `(q, offset)` durante 60 s (test `search_cache_hits_within_60s`).
- Regla: ante 401, refrescar el token **una** vez y reintentar; si vuelve 401 → `LoginNeeded`. 429 → `RateLimited` con `Retry-After`.

- [ ] **Step 1: Tests** sobre los fixtures: `parse_search_returns_tracks_and_artists`, `parse_search_handles_missing_images`, `parse_artist_albums_ok`, `parse_album_tracks_inherit_album_art`, `parse_devices_ok`, `parse_recent_ok`; y con un servidor falso en `127.0.0.1:0` (como `net.rs:251`): `unauthorized_after_refresh_reports_login_needed`, `rate_limited_reads_retry_after`. Para poder apuntar a un servidor falso, la base URL es un parámetro interno (`api_base()` sobreescribible en tests).
- [ ] **Step 2:** `cargo test player::spotify_api` → FAIL.
- [ ] **Step 3:** Implementar.
- [ ] **Step 4:** → PASS.
- [ ] **Step 5:** Commit `feat(player): Spotify Web API client (search, artist, album, devices, play, recent)`.

### Task 4: Elección de dispositivo Spotify (B spotifyd → A app oficial)

**Files:**
- Create: `src-tauri/src/player/spotify_device.rs`, `../linux/coucou-spotifyd.service` (unidad de usuario; `ExecStart=%h/.local/bin/spotifyd --no-daemon --device-name Coucou --backend pulseaudio --use-mpris true`)

**Interfaces:**
- Consumes: `spotify_api::{devices, Device}`.
- Produces:
  - `pub fn choose(devices: &[Device]) -> Choice` con `pub enum Choice { Coucou(String), App(String), None }`: prefiere el dispositivo de nombre `"Coucou"`; si no, uno de tipo `Computer` que no sea `"Coucou"`.
  - `pub async fn ensure_device() -> Result<(String /*device_id*/, bool /*fallback_used*/), ApiError>`:
    1. `devices()` → si `choose` da `Coucou` → listo.
    2. `systemctl --user start coucou-spotifyd.service`, sondear `devices()` cada 500 ms hasta 6 s.
    3. Respaldo A: `flatpak run com.spotify.Client` en segundo plano; minimizar su ventana en cuanto aparezca (`xdotool search --sync --classname spotify windowminimize`, timeout 8 s; si no hay xdotool, seguir sin minimizar); sondear hasta 10 s. Devuelve `fallback_used = true`.
    4. Nada → `ApiError::NoDevice`.
- Linux solo; en Windows `ensure_device` usa solo `devices()` + `choose`.

- [ ] **Step 1: Tests** de `choose`: `prefers_coucou_device`, `falls_back_to_app_when_coucou_device_missing`, `none_when_no_computer`. Test de `ensure_device` con funciones inyectadas (trait o closures para `devices`, `start_service`, `launch_app`): `starts_service_then_finds_coucou`, `uses_app_after_service_timeout` (sin esperas reales: reloj inyectado).
- [ ] **Step 2:** → FAIL. **Step 3:** Implementar. **Step 4:** → PASS.
- [ ] **Step 5:** Commit `feat(player): pick spotifyd first, official app as fallback`.

### Task 5: YT Music — ayudante Python local

**Files:**
- Create: `src-tauri/src/player/ytmusic.py` (embebido con `include_str!`), `src-tauri/src/player/ytmusic.rs`

**Interfaces:**
- Produces:
  - Ayudante: `python3 -I - <op> <arg>` con `op ∈ {search, artist, album}`; imprime JSON `[{source:"ytmusic", kind, id, title, subtitle, artUrl}]` o `{"error": "..."}`. Usa `ytmusicapi.YTMusic()` sin sesión. Intérprete: `settings::local_dir().join("ytm-venv/bin/python3")`.
  - `pub fn parse_helper_output(stdout: &str) -> Result<Vec<Item>, String>`
  - `pub async fn search(q) / artist(id) / album(id) -> Result<Vec<Item>, String>` — subproceso con timeout 8 s (`tokio::time::timeout` + kill); error claro si el venv no existe ("YT Music no está preparado: corre scripts/setup-player.sh").
- `#[cfg(target_os = "linux")]`.

- [ ] **Step 1: Tests**: `parse_ok`, `parse_error_object`, `parse_garbage_is_error`, `helper_timeout_is_an_error_not_a_hang` (ejecuta un "intérprete" falso que duerme; el intérprete es parámetro interno).
- [ ] **Step 2:** → FAIL. **Step 3:** Implementar. **Step 4:** → PASS.
- [ ] **Step 5:** Commit `feat(player): YT Music search and artist pages through a local helper`.

### Task 6: mpv por IPC

**Files:**
- Create: `src-tauri/src/player/mpv.rs`

**Interfaces:**
- Produces:
  - `pub fn socket_path() -> PathBuf` (`$XDG_RUNTIME_DIR/coucou-mpv.sock`)
  - `pub fn command_json(args: &[&str]) -> String` (`{"command":[...]}\n`)
  - `pub async fn play_ytmusic(video_id: &str) -> Result<(), String>`: si no hay mpv vivo en el socket, lanzar `mpv --no-video --idle=yes --input-ipc-server=<sock> --ytdl-format=bestaudio --script-opts=ytdl_hook-ytdl_path=<~/.local/bin/yt-dlp>`; luego `loadfile https://music.youtube.com/watch?v=<id> replace`.
  - `pub async fn pause() -> Result<(), String>` (`set_property pause yes`; sin mpv vivo → Ok)
- mpv se publica por MPRIS con el complemento del sistema (`mpv-mpris`), así lo ve `music.rs` sin cambios.

- [ ] **Step 1: Tests**: `command_json_format`, `play_sends_loadfile_to_socket` (servidor Unix falso en un tmpdir), `pause_without_mpv_is_ok`.
- [ ] **Step 2:** → FAIL. **Step 3:** Implementar. **Step 4:** → PASS.
- [ ] **Step 5:** Commit `feat(player): drive one background mpv over its IPC socket`.

### Task 7: Comandos Tauri `player_*`

**Files:**
- Modify: `src-tauri/src/player/mod.rs`, `src-tauri/src/lib.rs` (registrar en `generate_handler!`), `src/core/bridge.ts`

**Interfaces:**
- Consumes: Tareas 1–6.
- Produces (Rust → TS, camelCase):
  - `player_search(source: String /*"spotify"|"ytmusic"|"both"*/, query, offset: u32, seq: u32) -> SearchResult { seq, items: Vec<Item>, errors: Vec<SourceError> }` con `SourceError { source, message, loginNeeded: bool }`
  - `player_artist(source, id, name) -> ArtistPage { tracks: Vec<Item>, albums: Vec<Item> }`
  - `player_album(source, id) -> Vec<Item>`
  - `player_play(item: Item) -> PlayResult { fallbackUsed: bool }` — pausa la otra fuente antes (`mpv::pause` o `spotify_api::pause`), reproduce, `history::push` + `save`.
  - `player_recents() -> Vec<Item>` — historial local + `recently_played` de Spotify (si hay sesión), mezclados por fecha, dedupe por `(source,id)`, máx. 20.
  - `player_dj() -> PlayResult` — `play(Context("spotify:playlist:37i9dQZF1EYkqdzj48dyYq"))`; si Spotify lo rechaza, `xdg-open spotify:playlist:37i9dQZF1EYkqdzj48dyYq` y `fallbackUsed=true`. (La URI del DJ se confirma en la Tarea 11; si cambia, solo cambia esta constante `DJ_URI`.)
  - `player_spotify_login() -> Result<(), String>`, `player_spotify_connected() -> bool`
  - `pub fn merge_recents(local: Vec<HistoryEntry>, remote: Vec<HistoryEntry>, max: usize) -> Vec<Item>` (puro)
  - "both" en búsqueda: ambas fuentes en paralelo (`tokio::join!`), una que falla no tumba la otra (va a `errors`).
  - Bridge TS: `playerSearch`, `playerArtist`, `playerAlbum`, `playerPlay`, `playerRecents`, `playerDj`, `playerSpotifyLogin`, `playerSpotifyConnected` con los mismos nombres de campos; tipos TS `PlayerItem`, `SearchResult`, `ArtistPage`, `PlayResult` en `src/core/player.ts`.
- En Windows: `source` "ytmusic" devuelve error "YT Music solo en Linux"; "both" = solo Spotify.

- [ ] **Step 1: Tests**: `merge_recents_orders_by_time_and_dedupes`, `merge_recents_caps`, `both_keeps_spotify_when_ytmusic_fails` (fuentes inyectadas).
- [ ] **Step 2:** → FAIL. **Step 3:** Implementar. **Step 4:** `cargo test` + `npx tsc --noEmit` → PASS.
- [ ] **Step 5:** Commit `feat(player): Tauri commands for search, play, recents, DJ and login`.

### Task 8: Vista "player": encabezado, búsqueda y resultados

**Files:**
- Create: `src/views/player.ts`, `src/player.css` (importado en `src/main.ts`), `tests/player.test.mjs`
- Modify: `src/core/layout.ts` (`"player"` en `IslandViewName` y `VIEW_LAYOUTS`), `src/views/views.ts` (`map.set("player", buildPlayerView(actions))`), `src/island/island.ts` (`wantsKeys` incluye `"player"`; la lista de vistas que piden barras en ~:1642 incluye `"player"`), `src/core/state.ts` (ajuste `playerSource: "spotify"|"ytmusic"|"both"`, default `"both"`, y su espejo en `settings.rs`), `src/i18n/extra.json`

**Interfaces:**
- Consumes: Bridge de la Tarea 7; `musicTint`, `liveBars`, `State.music` existentes.
- Produces:
  - `export function buildPlayerView(actions: ViewActions): PlayerHost` con `PlayerHost extends ViewHost { tick(nowMs): void; openArtist(item: PlayerItem): void }`
  - Funciones puras exportadas para tests: `nextIndex(current: number, len: number, key: "ArrowUp"|"ArrowDown"): number`; `class QueryGate { next(): number; isCurrent(seq: number): boolean }`; `DEBOUNCE_MS = 250`.
- Contenido: encabezado con portada pequeña, título/artista, play/pausa/siguiente (vía `Bridge.musicControl`), tinte `musicTint` como acento y barras `liveBars`; campo de búsqueda con foco al abrir; selector de 3 estados (recordado en `playerSource`); lista de resultados navegable con flechas, Enter pone (`playerPlay`) o abre artista; errores por fuente en una línea discreta; si `loginNeeded`, botón "Conectar Spotify" (`playerSpotifyLogin`); aviso discreto si `fallbackUsed`; Esc → `actions.setView("music")` si llegó desde ahí, si no `actions.collapse()`; sin interacción durante `PLAYER_IDLE_MS = 30000` (y el puntero fuera) → `actions.collapse()`, con test `idleCollapsesAfter30s` (reloj falso). Paginación: al llegar al final de la lista, pedir `offset += 10`.
- En Windows (`navigator.userAgent.includes("Windows")`) el selector no muestra YT Music.

- [ ] **Step 1: Tests** (`tests/player.test.mjs`): `nextIndexWraps`, `staleResultsAreDropped` (QueryGate), `barsWantedOnlyWhilePlayerVisible` (con `tests/fakedom.mjs`: vista visible → `setBarsWanted(true)`; oculta → false), `selectorHidesYtMusicOnWindows`.
- [ ] **Step 2:** `node --import ./tests/setup.mjs --test tests/player.test.mjs` (Node 22) → FAIL.
- [ ] **Step 3:** Implementar; textos en `extra.json` (9 idiomas).
- [ ] **Step 4:** tests de player + `tests/i18n.test.mjs` + `npx tsc --noEmit` → PASS.
- [ ] **Step 5:** Commit `feat(player): glass player view with search`.

### Task 9: Recientes, DJ y página de artista con cover flow

**Files:**
- Create: `src/views/coverflow.ts`
- Modify: `src/views/player.ts`, `src/player.css`, `tests/player.test.mjs`, `src/i18n/extra.json`

**Interfaces:**
- Consumes: `playerRecents`, `playerDj`, `playerArtist`, `playerAlbum`.
- Produces:
  - `export function coverflowTransform(i: number, center: number): { x: number; scale: number; rotateY: number; z: number; opacity: number }` — centro `scale 1, rotateY 0`; vecinos `±1`: `scale 0.8, rotateY ∓45`; más lejos `scale 0.65`, `opacity` baja hasta 0 a partir de `|i-center| > 4`.
  - `export function buildCoverflow(items: PlayerItem[], onPick: (it) => void): { el; move(delta: number): void; center(): number }` — rueda/flechas izq/der mueven; clic o Enter en la central → `onPick`.
- Sin consulta escrita: fila "DJ" + lista de recientes (`playerRecents`, al abrir la vista, sin sondeo).
- Artista: nombre, cover flow de álbumes (al elegir uno → sus canciones en la lista), y "Canciones" (tracks de `playerArtist`). Atrás con Esc/Backspace vuelve a los resultados.

- [ ] **Step 1: Tests**: `coverflowCenterIsFlat`, `coverflowNeighboursTurnInward`, `coverflowFarItemsFadeOut`, `moveClampsAtEnds`.
- [ ] **Step 2:** → FAIL. **Step 3:** Implementar. **Step 4:** → PASS + tsc + i18n.
- [ ] **Step 5:** Commit `feat(player): recents, DJ and artist page with cover flow`.

### Task 10: Entradas — lupa, atajo `openPlayer` y `--player`

**Files:**
- Modify: `src/views/music.ts` (solo un botón lupa en la vista completa → `actions.setView("player")`; nada más cambia), `src-tauri/src/shortcuts.rs` (`action("openPlayer", "Super+Alt+M", true, true)` en `ACTIONS`, `description`, actualizar `MAC_IDS` y el test `only_the_island_toggle_is_off_by_default`), `src/core/shortcuts.ts` (`ShortcutId`, `SHORTCUTS`, `SHORTCUT_TEXT`), `src/island/shortcuts.ts` (`case "openPlayer"`: `host.alert("player"); host.takeKeyboard()`), `src-tauri/src/lib.rs` (en el callback single-instance, `--player` → `app.emit_to(island::WINDOW_LABEL, "tray", "player")`), `src/main.ts` (`case "player"` → abrir en "player" con teclado), `tests/shortcuts.test.mjs`, `src/i18n/extra.json`
- Combinación por defecto `Super+Alt+M` (decisión de Claude; comprobar antes que no choque con `gsettings list-recursively org.gnome.desktop.wm.keybindings` / Pop Shell en la laptop y cambiarla si choca).

- [ ] **Step 1: Tests**: en `tests/shortcuts.test.mjs`, `openPlayer` presente en ambas tablas y activado por defecto; en Rust, `from_args` reconoce `--shortcut openPlayer`.
- [ ] **Step 2:** → FAIL. **Step 3:** Implementar. **Step 4:** `cargo test shortcuts` + node tests + tsc → PASS.
- [ ] **Step 5:** Commit `feat(player): magnifier in the music view, openPlayer shortcut and --player`.

### Task 11: Preparación de la laptop, prueba real y retiro de Sonora (con Alan)

**Files:**
- Create: `../scripts/setup-player.sh`, sección "Reproductor" en `README.md` de `windows/`
- Check: `src-tauri/src/handoff.rs:211-226` (`sonora_spotify_id` lee la base de Sonora)

- [ ] **Step 1:** `setup-player.sh` (idempotente, sin sudo salvo lo que diga): `yt-dlp` binario oficial en `~/.local/bin` (`yt-dlp -U` luego funciona); venv `~/.local/share/coucou/ytm-venv` con `ytmusicapi`; `spotifyd` binario oficial (variante con MPRIS) en `~/.local/bin` + unidad `coucou-spotifyd.service` en `~/.config/systemd/user/`; verifica `mpv-mpris` y `xdotool` e imprime `sudo apt install mpv-mpris xdotool` si faltan (lo corre Alan); escribe `spotifyClientId` en `~/.config/coucou/settings.json` si se pasa `--client-id <id>` (sin tocar otros campos). Instrucción para la sesión única de spotifyd (OAuth en el navegador) según su documentación.
- [ ] **Step 2:** Build release (ver `~/estado/proyectos/coucou.md` "Trampas") e instalar en la laptop.
- [ ] **Step 3 (Alan):** Conectar Spotify desde la vista; buscar y poner una canción de cada fuente; probar DJ, recientes, artista y cover flow; cerrar spotifyd a mano y confirmar el respaldo A. Anotar si el DJ funcionó por API, por la app, o no.
- [ ] **Step 4:** Revisar `handoff.rs`: si depende de Sonora para algo que Alan usa, que no falle sin Sonora (ya devuelve None sin la base; confirmarlo con un test o leyendo).
- [ ] **Step 5 (con OK explícito de Alan):** copiar `~/.var/app/io.github.nolight132.sonora/` a `~/backups/sonora-2026-11/` y `flatpak uninstall --user io.github.nolight132.sonora`.
- [ ] **Step 6:** Integrar en `unify/monorepo` (rama estable verificada), borrar la rama, actualizar `~/estado/proyectos/coucou.md`. Commit `docs(player): setup script and README`.
