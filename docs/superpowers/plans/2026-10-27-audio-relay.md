# Audio relay (audiolink) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** What plays on the Zorin laptop comes out of the Bose connected to the S22, with automatic "last one to start wins" pausing, and a manual way to move the Bose to the PC.

**Architecture:** The phone dials the laptop over SSH with a **second, audio-only restricted key** (`coucou-hook --audio`, which only pipes to a new private Unix socket `coucou-audio.sock`). The laptop moves its default output to a silent virtual sink, captures that sink with `parec`, and streams framed PCM down the channel; the phone plays it with `AudioTrack` as a foreground service. A pure state machine (same function in Rust and Kotlin, pinned by one shared JSON vector file) decides sink, stream and pausing.

**Tech Stack:** Rust (Tauri app `windows/src-tauri`, `coucou-hook`), `pactl`/`parec`/`bluetoothctl`, MPRIS via `zbus` (already a dependency), Kotlin (Android, `sshj` already a dependency), `MediaSessionManager`, `AudioTrack`.

**Spec:** `docs/superpowers/specs/2026-10-27-audio-relay-design.md`

**Naming:** the repo already uses "relay" for the hook relay (`platform::relay_socket_path`). Everything new is called **audiolink** to avoid confusion.

## Global Constraints

- Branch `feat/audio-relay`, from `unify/monorepo`. Never push to `upstream`. Commit after each task; do not push or open a PR unless Alan says so.
- The existing key and `coucou-hook --remote` must behave exactly as before (spec: "Weakening the existing phone link" is an anti-goal).
- Nothing listens on the network; the PC only has a Unix socket (mode `0600`, peer uid checked), like `remote.rs`.
- Raw PCM, **48000 Hz, 2 channels, s16le**, 20 ms chunks (3840 bytes) in v1 (my call: no new codec dependency; ~1.5 Mbit/s; Opus only if the on-device measurement in Task 8 says the bandwidth hurts).
- Audio stream carries only the virtual sink's monitor. Never record a microphone or any other source.
- 0 % CPU when nothing plays or when the setting is off: no polling loops; blocking reads only. Setting is off by default (`audiolink_enabled: false`).
- Phone heartbeat: the PC declares the phone gone after **3 s** without a frame or heartbeat from it. Headphone-drop grace: **5 s**.
- Target phone: S22 only (minSdk of the app is 26; the code may use API 31+ calls behind `Build.VERSION.SDK_INT` guards or just target them since only the S22 matters).
- Android stays GPL-3.0 with the file header already used in `coucou/` files. Swift untouched.
- Secrets never in the repo.
- Verify: `cd windows/hook && cargo test`; `cd windows/src-tauri && cargo test audiolink`; `cd windows && npx tsc --noEmit`; `cd android && ./gradlew :app:testDebugUnitTest --tests '*audiolink*' :app:compileDebugKotlin`; `bash scripts/check-contracts.sh`. If `src-tauri` does not build for lack of GTK/WebKit dev packages, say so and do not claim the Rust tests passed.

## Review Focus

Inputs and conditions the spec implies but no happy path exercises, most likely to bite first. Each has a test in the task that owns it.

1. Coucou on the PC is killed (`kill -9`) while the virtual sink is the default → the laptop stays silent forever. Pinned in Task 4 (watchdog restores on EOF).
2. A Bose blip of 1-2 s (walking away) → the laptop must not start blasting its speakers. Pinned in Task 1 (hold + grace) and Task 6.
3. A corrupt or hostile frame length arriving on the socket (65535, or a type that does not exist) → drop the connection, never allocate or panic. Pinned in Task 2.
4. The phone connects twice (reconnect before the old connection timed out) → the new one replaces the old; never two streams into one `AudioTrack`. Pinned in Task 3.
5. A notification sound, navigation voice or a short sound on the phone → must not pause the PC; only media sessions in `PLAYING` count. Pinned in Task 6.

---

## File Structure

PC (`windows/`):
- `src-tauri/src/audiolink_logic.rs` (new): pure state machine. No I/O.
- `src-tauri/src/audiolink_wire.rs` (new): pure frame encode/decode.
- `src-tauri/src/audiolink_sink.rs` (new): virtual sink, capture process, marker file, watchdog spawn. Linux only.
- `src-tauri/src/audiolink.rs` (new): the socket server, connection handling, glue between logic, sink, MPRIS and the phone.
- `src-tauri/src/music.rs` (modify): `pause_all_playing()` and a hook that reports when "something plays" changes.
- `src-tauri/src/{lib.rs,settings.rs}`, `src/core/state.ts`, `src/settings/main.ts` (modify): setting + module wiring.
- `hook/src/{main.rs,unix.rs}` (modify): `--audio` and `--audio-watchdog`.
- `scripts/phone/audiolink-vectors.json` (new): shared vectors.

Phone (`android/app/src/main/java/com/agupta07505/smartisland/coucou/`):
- `audiolink/AudioLinkLogic.kt`, `audiolink/AudioLinkWire.kt` (new): same pure code as the PC.
- `audiolink/AudioLinkService.kt` (new): foreground service, SSH loop, `AudioTrack`, audio focus.
- `audiolink/Watchers.kt` (new): headphones (A2DP), phone media sessions, calls.
- `phone/PhoneKey.kt`, `phone/Ssh.kt`, `phone/PhoneSetupActivity.kt` (modify): second key.
- `AndroidManifest.xml` (modify): service + permission.
- Tests under `android/app/src/test/java/.../coucou/audiolink/`, vectors in `android/app/src/test/resources/audiolink-vectors.json`.

Shared: `scripts/check-contracts.sh` (modify), `docs/AUDIO-LINK.md` (new).

---

### Task 1: State machine and shared vectors

**Files:**
- Create: `windows/src-tauri/src/audiolink_logic.rs`, `android/app/src/main/java/com/agupta07505/smartisland/coucou/audiolink/AudioLinkLogic.kt`, `windows/scripts/phone/audiolink-vectors.json`, `android/app/src/test/resources/audiolink-vectors.json` (byte-identical copy)
- Test: in-module `#[cfg(test)]` in `audiolink_logic.rs`; `android/app/src/test/java/com/agupta07505/smartisland/coucou/audiolink/AudioLinkLogicTest.kt`
- Modify: `scripts/check-contracts.sh` (add `check windows/scripts/phone/audiolink-vectors.json android/app/src/test/resources/audiolink-vectors.json`), `windows/src-tauri/src/lib.rs` (`mod audiolink_logic;`)

**Interfaces:**
- Produces (Rust and Kotlin, same names in each language's case): `enum Mode { Off, Relay, Hold }`; `enum Where { Phone, Pc, None }`; `enum Event { Enabled(bool), Headphones(Where), Link(bool), Call(bool), PhoneMedia(bool), PcMedia(bool), GraceExpired }`; `enum Action { SinkVirtual, SinkRestore, RelayStart, RelayStop, PausePc, PausePhone }`; `struct State` (fields: `mode, enabled, headphones, link_up, call, phone_playing, pc_playing`, all defaulting to Off/false/None); `fn step(state: &State, ev: Event) -> (State, Vec<Action>)`. Kotlin: `AudioLinkLogic.step(state, ev): Pair<State, List<Action>>`.
- JSON vector format: `{"state_cases":[{"name":str,"steps":[{"event":{"t":"enabled","on":bool}|{"t":"headphones","at":"phone"|"pc"|"none"}|{"t":"link","up":bool}|{"t":"call","on":bool}|{"t":"phone_media","playing":bool}|{"t":"pc_media","playing":bool}|{"t":"grace_expired"},"mode":"off"|"relay"|"hold","actions":[snake_case action names]}]}]}`. `mode` and `actions` are what must hold **after** the step. The Rust and Kotlin tests each load the file and run every case from `State::default()`.

Rules (these are the decisions; the vectors pin them). `armed = enabled && headphones == Phone`; `ready = armed && link_up && !call`.
- Off → Relay when `ready`: `[SinkVirtual, RelayStart]`. Off → Hold when `armed && !ready`: `[SinkVirtual, PausePc]`.
- Relay → Off when `!enabled`: `[RelayStop, SinkRestore]`. Relay → Off when `headphones == Pc` (a swap): `[RelayStop, SinkRestore]`. Relay → Hold when `!ready` for any other reason (link down, call, headphones None): `[RelayStop, PausePc]`.
- Hold → Off when `!enabled` or `headphones == Pc`: `[SinkRestore]`. Hold → Off on `GraceExpired` while `headphones != Phone`: `[SinkRestore]`. Hold → Relay when `ready`: `[RelayStart]` (the PC player is **not** resumed).
- Pausing, evaluated after the mode change and only adding actions: `PhoneMedia(true)` while Relay and `pc_playing` → `PausePc`; `PcMedia(true)` while Relay and `phone_playing` → `PausePhone`; `PcMedia(true)` while Hold → `PausePc`. In Off nothing is coordinated.

- [ ] **Step 1: Write the vectors and both failing tests.** Cases (names verbatim): `starts_relaying` (enabled→[], link up→[], headphones phone→`[sink_virtual,relay_start]`, mode relay); `phone_video_pauses_pc` (start, `pc_media true`→[], `phone_media true`→`[pause_pc]`); `pc_music_pauses_phone` (start, `phone_media true`→[], `pc_media true`→`[pause_phone]`); `link_lost_holds_silent` (start, `link down`→`[relay_stop,pause_pc]` hold, `link up`→`[relay_start]` relay); `call_holds` (start, `call on`→`[relay_stop,pause_pc]`, `call off`→`[relay_start]`); `headphones_blip` (start, `headphones none`→`[relay_stop,pause_pc]` hold, `headphones phone`→`[relay_start]` relay); `headphones_gone_for_good` (start, `headphones none`→hold, `grace_expired`→`[sink_restore]` off); `swap_to_pc` (start, `headphones pc`→`[relay_stop,sink_restore]` off); `disabled_stops` (start, `enabled false`→`[relay_stop,sink_restore]` off); `held_pc_start_is_paused` (start, `link down`, `pc_media true`→`[pause_pc]`); `off_does_not_coordinate` (`phone_media true`, `pc_media true`→[] mode off).
- [ ] **Step 2: Run to verify it fails.** `cd windows/src-tauri && cargo test audiolink_logic` and `cd android && ./gradlew :app:testDebugUnitTest --tests '*AudioLinkLogicTest*'`. Expected: FAIL (not defined).
- [ ] **Step 3: Implement `step` in both languages** following the rules above. Rust loads the JSON with `serde_json` via `include_str!("../../scripts/phone/audiolink-vectors.json")`; Kotlin reads the test resource.
- [ ] **Step 4: Run both test commands plus `bash scripts/check-contracts.sh`.** Expected: PASS, `ok: audiolink-vectors.json`.
- [ ] **Step 5: Commit** `audiolink: shared state machine and vectors`.

---

### Task 2: Wire framing

**Files:**
- Create: `windows/src-tauri/src/audiolink_wire.rs`, `.../coucou/audiolink/AudioLinkWire.kt`
- Modify: both vector files (add a `"wire_cases"` array; keep them byte-identical), `lib.rs` (`mod audiolink_wire;`)
- Test: in-module Rust tests; `AudioLinkWireTest.kt`

**Interfaces:**
- Produces: PC→phone frame = `[type: u8][len: u16 big-endian][payload]`. Types: `1` audio (len ≤ 8192), `2` heartbeat (len 0), `3` hello (payload JSON `{"rate":48000,"channels":2,"fmt":"s16le"}`, len ≤ 256). Phone→PC: one JSON object per line, ≤ 512 bytes: `{"op":"hb"}`, `{"op":"state","headphones":"phone"|"pc"|"none","call":bool,"phone_playing":bool}`.
- Rust: `pub fn encode(kind: FrameKind, payload: &[u8]) -> Vec<u8>`; `pub fn decode(buf: &[u8]) -> Result<Option<(Frame, usize)>, WireError>` (`Ok(None)` = need more bytes; `usize` = bytes consumed); `pub fn parse_phone_line(line: &str) -> Result<PhoneMsg, WireError>`. Kotlin mirrors: `AudioLinkWire.encode/decode/parsePhoneLine`-equivalents for the direction the phone uses (it decodes frames, encodes lines).
- Vector `wire_cases`: `{"name","hex","result":"audio"|"heartbeat"|"hello"|"need_more"|"error","consumed":n}`. Required entries: `audio_two_bytes` hex `0100020102` → audio, consumed 5; `heartbeat` `020000` → heartbeat, 3; `split_header` `0100` → need_more; `too_long` `01FFFF` → error; `unknown_type` `090000` → error; `heartbeat_with_payload` `0200010 0`→ write as `020001ff` → error; `hello_ok` with a 52-byte JSON payload.

- [ ] **Step 1: Write the failing tests** that run every `wire_cases` entry in both languages, plus (Rust) `phone_line_rejects_over_512_bytes_and_unknown_op`.
- [ ] **Step 2: Run to verify they fail** (same commands as Task 1).
- [ ] **Step 3: Implement** `encode`/`decode`/`parse_phone_line` (Rust) and the Kotlin counterparts. Review-Focus 3: length above the type's maximum is an error before any allocation.
- [ ] **Step 4: Run tests and `check-contracts.sh`.** Expected: PASS.
- [ ] **Step 5: Commit** `audiolink: wire framing`.

---

### Task 3: PC socket, hook pipe and settings

**Files:**
- Create: `windows/src-tauri/src/audiolink.rs`
- Modify: `windows/hook/src/main.rs`, `windows/hook/src/unix.rs`, `windows/src-tauri/src/{lib.rs,settings.rs}`, `windows/src/core/state.ts`, `windows/src/settings/main.ts`
- Test: `windows/hook/tests/audio_pipe.rs`; in-module tests in `audiolink.rs` for the connection registry

**Interfaces:**
- Consumes: Task 2's `decode`/`encode`/`parse_phone_line`.
- Produces: `coucou-hook --audio` pipes stdin/stdout to `$XDG_RUNTIME_DIR/coucou-audio.sock` exactly as `--remote` does for `coucou-remote.sock` (refactor `run_remote` into `fn pipe_to(socket_name: &str) -> !`; `--remote` behaviour must stay byte-identical; when the app is closed or the setting is off it prints `{"t":"offline"}` and exits). `pub fn audiolink::sync_enabled(app: &AppHandle, on: bool)` (Linux real, other platforms a no-op), `Settings.audiolink_enabled: bool` (default false, serde name `audiolinkEnabled` like its neighbours — copy the neighbour's attribute style), `pub fn audiolink::socket_path() -> Option<PathBuf>`.
- Server: `UnixListener` mode `0600`, peer uid check, same pattern as `remote.rs::serve`. One active connection: a new accepted connection **replaces** the old one (Review-Focus 4). On accept it sends a `hello` frame, then reads phone lines; every received line resets the 3 s liveness timer; no line for 3 s closes the connection and emits `Event::Link(false)`. `Event::Link(true)` on accept. A `state` line emits `Headphones`, `Call` and `PhoneMedia` events.

- [ ] **Step 1: Write failing tests.** Hook: `audio_pipe_forwards_both_ways` (spawn `env!("CARGO_BIN_EXE_coucou-hook") --audio` with `XDG_RUNTIME_DIR` set to a temp dir prepared the way `private_runtime_dir()` requires, mode 0700; a listener thread on `coucou-audio.sock` echoes what it reads; assert the echo comes out of stdout) and `audio_pipe_offline_when_no_socket` (stdout is exactly `{"t":"offline"}\n`, exit 0) and `remote_pipe_unchanged` (same two checks for `--remote` against `coucou-remote.sock`). App: `second_connection_replaces_first` (registry type `Conn` slot: inserting a new id returns the previous one to be closed) and `silent_connection_times_out` using tokio's paused clock.
- [ ] **Step 2: Run to verify they fail.** `cd windows/hook && cargo test` ; `cd windows/src-tauri && cargo test audiolink::`.
- [ ] **Step 3: Implement** the hook refactor (`--audio`), the server in `audiolink.rs`, `audiolink_enabled` in settings (default false), the toggle "Phone headphones (audio relay)" under Settings → Phone in `src/settings/main.ts`, and the `sync_enabled` calls next to the two `remote::sync_enabled` calls in `lib.rs`. Events go into a single `std::sync::mpsc` channel consumed by one thread that owns the Task 1 `State` (this thread is where Tasks 4 and 5 attach actions).
- [ ] **Step 4: Run the tests above and `cd windows && npx tsc --noEmit`.** Expected: PASS, no type errors.
- [ ] **Step 5: Commit** `audiolink: audio-only hook pipe, socket and setting`.

---

### Task 4: Virtual sink, capture and crash-safe restore (PC)

**Files:**
- Create: `windows/src-tauri/src/audiolink_sink.rs`
- Modify: `windows/hook/src/{main.rs,unix.rs}` (`--audio-watchdog`), `windows/src-tauri/src/audiolink.rs` (execute `SinkVirtual`, `SinkRestore`, `RelayStart`, `RelayStop`), `lib.rs` (`mod audiolink_sink;`)
- Test: in-module tests for the pure parts; `windows/hook/tests/audio_watchdog.rs`

**Interfaces:**
- Produces: `pub fn sink_virtual() -> Result<(), String>` (reads and saves the current default sink name into the marker `~/.local/share/coucou/audiolink-sink.json` as `{"previous":"<name>","module":<id>}` **before** loading `pactl load-module module-null-sink sink_name=coucou_audiolink`, then `pactl set-default-sink coucou_audiolink` and moves every existing sink input to it with `pactl move-sink-input`); `pub fn sink_restore() -> Result<(), String>` (sets the default back to `previous`, moves inputs back, unloads the module, deletes the marker; safe to call twice); `pub fn restore_if_stale()` (called at app start: if the marker exists, run `sink_restore`); `pub fn capture_start() -> Result<CaptureHandle, String>` spawning `parec --device=coucou_audiolink.monitor --format=s16le --rate=48000 --channels=2 --latency-msec=20` with a `read_chunk(&mut self, buf: &mut [u8; 3840]) -> io::Result<bool>`; `capture_stop(handle)`; pure `fn parse_default_sink(pactl_output: &str) -> Option<String>` and `fn marker_json(previous: &str, module: u32) -> String` / `fn parse_marker(&str) -> Option<(String, u32)>`.
- `coucou-hook --audio-watchdog`: reads stdin until EOF, then performs the same restore from the marker using `pactl` (the hook cannot link the app crate, so it re-implements the 4 `pactl` calls; keep them in one small function). The app spawns it when it enters Relay/Hold with the virtual sink and keeps its stdin pipe open; app death closes the pipe → restore (Review-Focus 1). The app closes the pipe itself on `sink_restore`.

- [ ] **Step 1: Write failing tests.** `parse_default_sink_reads_pactl_get_default_sink`, `marker_round_trips`, `parse_marker_rejects_garbage`; hook test `watchdog_restores_on_eof` using a fake `pactl` earlier in `PATH` (a shell script that appends its arguments to a log file) and a marker pointing at `previous = "alsa_output.test"`; assert the log has `set-default-sink alsa_output.test` and `unload-module` after stdin is closed, and that nothing runs while stdin stays open.
- [ ] **Step 2: Run to verify they fail.** `cargo test audiolink_sink` ; `cd windows/hook && cargo test audio_watchdog`.
- [ ] **Step 3: Implement** the functions above; wire actions in `audiolink.rs`: `SinkVirtual` → `sink_virtual()` + spawn watchdog; `SinkRestore` → `sink_restore()` + close watchdog stdin; `RelayStart` → start a capture thread that reads chunks and writes `audio` frames to the current connection (blocking read; no polling); `RelayStop` → `capture_stop`. Call `restore_if_stale()` from `audiolink::sync_enabled` before serving. A `heartbeat` frame is sent every second while connected.
- [ ] **Step 4: Run the tests. Then, on Alan's laptop only, run the manual check:** enable the setting, connect a fake client (`nc -U $XDG_RUNTIME_DIR/coucou-audio.sock`), `pactl get-default-sink` says `coucou_audiolink`; `kill -9` Coucou; within 2 s `pactl get-default-sink` is the original again. Expected: PASS. If this machine's Bluetooth/Pipewire cannot be touched in this session, state that this check was not run.
- [ ] **Step 5: Commit** `audiolink: virtual sink, capture and crash-safe restore`.

---

### Task 5: PC pause and playing detection

**Files:**
- Modify: `windows/src-tauri/src/music.rs`, `windows/src-tauri/src/audiolink.rs`
- Test: in-module tests in `music.rs` (pure helper) and `audiolink.rs`

**Interfaces:**
- Produces in `music.rs::mpris`: `pub fn pause_all_playing() -> usize` (MPRIS `Pause`, never a toggle, to **every** player whose `PlaybackStatus` is `Playing`; returns how many were paused), and a notification: `pub fn set_playing_listener(f: fn(bool))` called from the existing resync loop whenever "any player is Playing" changes. The listener must also work when the Music pill is off in the UI: the MPRIS thread currently starts only when `ENABLED` — add a second reason to run (the audiolink setting), with `sync_enabled` taking either flag.
- `audiolink.rs` maps `PausePc` → `pause_all_playing()`, and the listener → `Event::PcMedia`.
- Pure helper `fn any_playing(statuses: &[&str]) -> bool`.

- [ ] **Step 1: Write failing tests:** `any_playing_is_true_only_for_playing` (`["Paused","Stopped"]` false; `["Paused","Playing"]` true; `[]` false) and `playing_listener_fires_only_on_change` (feed a sequence `[false,false,true,true,false]` into the debouncing helper, expect calls `[true,false]`).
- [ ] **Step 2: Run to verify they fail.** `cargo test music:: audiolink::`.
- [ ] **Step 3: Implement** both functions and the wiring; `PausePc` is idempotent.
- [ ] **Step 4: Run tests.** Expected: PASS. Check with `cargo test` that existing `music::` tests still pass.
- [ ] **Step 5: Commit** `audiolink: pause every MPRIS player and report when something plays`.

---

### Task 6: Phone detectors and pausing

**Files:**
- Create: `.../coucou/audiolink/Watchers.kt`
- Modify: `AndroidManifest.xml` (none yet; the watchers use the existing notification listener)
- Test: `.../audiolink/WatchersLogicTest.kt`

**Interfaces:**
- Produces: `interface PhoneSignals { fun onHeadphones(at: Where); fun onCall(on: Boolean); fun onPhoneMedia(playing: Boolean) }` and `class Watchers(context: Context, signals: PhoneSignals) { fun start(); fun stop(); fun pausePhoneMedia() }`. Pure helpers (JVM-testable): `fun headphonesWhere(types: List<Int>): Where` (`Phone` if any is `AudioDeviceInfo.TYPE_BLUETOOTH_A2DP`, else `None`; the phone never reports `Pc`), `fun isCall(audioMode: Int): Boolean` (`MODE_IN_CALL`, `MODE_IN_COMMUNICATION`, `MODE_RINGTONE` → true), `fun mediaPlaying(states: List<Int>): Boolean` (any `PlaybackState.STATE_PLAYING`).
- Real watchers: `AudioManager.registerAudioDeviceCallback` (A2DP output), `AudioManager.addOnModeChangedListener` (API 31+), `MediaSessionManager.addOnActiveSessionsChangedListener` with the component of `SmartIslandNotificationListenerService` plus a `MediaController.Callback` per session; `pausePhoneMedia()` calls `transportControls.pause()` on every playing session. If notification access is not granted, `onPhoneMedia` never fires and the service shows "needs notification access" (Review-Focus 5: notification sounds are not media sessions, so they cannot pause the PC).

- [ ] **Step 1: Write failing tests:** `a2dp_present_is_phone`, `no_a2dp_is_none`, `wired_only_is_none`, `call_modes`, `media_playing_needs_state_playing` (`STATE_PAUSED`,`STATE_BUFFERING` → false).
- [ ] **Step 2: Run to verify they fail.** `cd android && ./gradlew :app:testDebugUnitTest --tests '*WatchersLogicTest*'`.
- [ ] **Step 3: Implement** the helpers and the Android-side watchers.
- [ ] **Step 4: Run tests and `:app:compileDebugKotlin`.** Expected: PASS.
- [ ] **Step 5: Commit** `audiolink: phone-side detectors`.

---

### Task 7: Phone service, second key and playback

**Files:**
- Create: `.../audiolink/AudioLinkService.kt`
- Modify: `.../phone/PhoneKey.kt`, `.../phone/Ssh.kt`, `.../phone/PhoneSetupActivity.kt`, `AndroidManifest.xml`, values `strings.xml` (`values`, `values-en`, `values-es`)
- Test: extend `PhoneKeyTest.kt`; `AudioLinkPlaybackTest.kt` for the pure jitter policy

**Interfaces:**
- Consumes: Tasks 1, 2, 6.
- Produces: `PhoneKey` gains a `name` parameter on `exists/create/publicLine/load` (default `"phone"` keeps current file names; `"audio"` uses `coucou-audio-key.enc` / `.pub`; the Keystore wrap key is shared). `Ssh.open(context, profile, connectTimeoutMs, keyName = "phone")`. `PhoneSetupActivity` shows a second line: `restrict,command="/home/$user/.local/share/coucou/bin/coucou-hook --audio" <audio key line>` and a "Copy" button. `AudioLinkService` (foreground, `foregroundServiceType="mediaPlayback"`, permission `FOREGROUND_SERVICE_MEDIA_PLAYBACK`): while headphones are on the phone and the setting is on, opens `Ssh` with the **laptop profile** and `keyName = "audio"`, runs the `coucou-hook --audio` stream, decodes frames, plays PCM with `AudioTrack` (`PERFORMANCE_MODE_LOW_LATENCY`, `USAGE_MEDIA`, 48000 Hz stereo 16-bit), takes audio focus (`AUDIOFOCUS_GAIN`), sends a heartbeat line every second and a `state` line whenever a watcher changes, reconnects with growing waits (reuse the back-off style of `AgentRemote.loop`), and runs Task 1's `step` with the same events to learn `PausePhone`.
- Pure jitter policy: `fun playbackPlan(queuedMs: Int): PlaybackAction` with `Drop` when `queuedMs > 200`, `Wait` when `queuedMs < 20` at start (pre-buffer to 60 ms before the first write), else `Play`.

- [ ] **Step 1: Write failing tests:** `PhoneKeyTest.audio_key_is_separate_from_phone_key` (creating `"audio"` does not change `publicLine(ctx,"phone")` and the two lines differ — use whatever JVM-testable seam the existing `PhoneKeyTest` already uses) and `playback_plan_thresholds`.
- [ ] **Step 2: Run to verify they fail.** `./gradlew :app:testDebugUnitTest --tests '*PhoneKeyTest*' --tests '*AudioLinkPlaybackTest*'`.
- [ ] **Step 3: Implement** the key parameter, the setup line, the service and the manifest entries. A new `hello` or a repeated connect must stop the previous `AudioTrack` before starting another.
- [ ] **Step 4: Run the tests and `:app:compileDebugKotlin`.** Expected: PASS.
- [ ] **Step 5: Commit** `audiolink: phone service, audio key and playback`.

---

### Task 8: First full run on the S22 (acceptance gate)

**Files:** `docs/AUDIO-LINK.md` (new: what it does, setup of the second key line on the laptop, the manual test below, recovery commands: `pactl set-default-sink <name>` and `pactl unload-module module-null-sink`).

This task changes no code unless a step fails. It is the spec's "open measurements". Run only with Alan present (Bose, S22, Zorin laptop).

- [ ] **Step 1:** Install the PC build and the phone build (`scripts/install-phone.sh`, never a plain `install -r` — see `docs/MUSIC-HANDOFF.md` on the handoff branch for why). Paste the second key line into `~/.ssh/authorized_keys` on the laptop. Turn the setting on.
- [ ] **Step 2:** Bose connected to the S22. Play music on the laptop. Expected: it comes out of the Bose; the laptop speakers are silent.
- [ ] **Step 3: Measure delay.** Clap in front of a laptop video of a clapperboard, record phone video of both (screen + Bose clap sound via the phone mic is not possible) — simpler: play a metronome on the laptop and tap a stopwatch app on the phone at each audible click; record 20 taps; report the mean offset on home Wi-Fi and over Tailscale. Target < 500 ms (ideal ~300).
- [ ] **Step 4:** Screen off for 10 minutes while playing. Expected: no stutter, service alive.
- [ ] **Step 5:** Start a video on the phone → the laptop pauses. Start music on the laptop → the phone pauses. A notification sound → nothing. A call → laptop silent; after the call the laptop is not resumed.
- [ ] **Step 6:** Turn off Tailscale/Wi-Fi on the phone. Expected: within ~3 s the laptop pauses and **never** plays on its speakers. `kill -9 coucou` → default sink restored within 2 s.
- [ ] **Step 7:** Write the results (delay numbers, any failure) into `docs/AUDIO-LINK.md` and commit `audiolink: docs and first on-device measurements`. If delay > 500 ms or the stream stutters on Tailscale, stop and report to Alan before starting Task 9 (Opus becomes the first fix).

---

### Task 9: Manual swap of the headphones

**Files:** Modify `windows/src-tauri/src/audiolink.rs`, `windows/src-tauri/src/audiolink_sink.rs` (BlueZ helper), `windows/src/settings/main.ts` (or the island control already used by Hub — put one button where the Music pill/Hub keeps device actions; follow the neighbouring view), `.../audiolink/AudioLinkService.kt`.
Test: in-module tests for the pure parts.

**Facts that shape this task (do not skip):** Android does not let an app silently disconnect or connect a Bluetooth audio device (the handoff branch hit the same wall), so the phone side of a swap is a **prompt**, not an action. The PC side can do `bluetoothctl connect|disconnect <MAC>`. Whether the Bose accept a connection from the PC right after the phone lets go was never tested.

**Interfaces:**
- Produces: `fn bluez_connect(mac: &str) -> Result<(), String>`, `fn bluez_disconnect(mac: &str) -> Result<(), String>`, `fn find_headphones_mac() -> Option<String>` (first device in `bluetoothctl devices Connected`/`Paired` whose `Icon` is `audio-headphones` or `audio-headset`; pure parser `parse_bluetoothctl_info(&str) -> Option<(String, bool)>` = (icon-kind ok, connected)). New op in the PC→phone direction as a JSON line `{"t":"swap_to_pc"}`; phone→PC `{"op":"swap_done"}` is not needed — the PC learns via `Headphones(Pc)` when the phone reports `None` and `bluez_connect` succeeds.
- Behaviour: "Headphones to PC": PC sends `swap_to_pc`; the phone raises a heads-up notification "Release the headphones" whose action opens `Settings.ACTION_BLUETOOTH_SETTINGS`; the PC retries `bluez_connect` every 3 s for 60 s. When it connects, emit `Event::Headphones(Pc)` (state machine goes Off, speakers restored). "Headphones to phone": the PC runs `bluez_disconnect`, emits `Headphones(None)`, and the phone prompts "Connect the headphones"; when the phone reports `Phone`, the relay resumes.

- [ ] **Step 1: Write failing tests:** `parse_bluetoothctl_info_reads_icon_and_connected` (two canned `bluetoothctl info` outputs, one headset connected, one a mouse).
- [ ] **Step 2: Run to verify it fails.** `cargo test audiolink_sink::`.
- [ ] **Step 3: Implement** the helpers, the PC button/handler, and the phone prompt. Measurement #3: record in `docs/AUDIO-LINK.md` whether the PC connected by itself after the phone released.
- [ ] **Step 4: Run tests; then the on-device swap both ways with Alan.** Expected: PASS and the doc updated.
- [ ] **Step 5: Commit** `audiolink: manual headphones swap`.

---

## Self-review

- **Spec coverage:** goals 1–5 → Tasks 3, 4, 7 (stream), 1+5+6 (pausing), 4 (silent speakers), 9 (swap), 8 (delay). Crash/silence → Task 4. Phone call/alarm hold → Task 1/6. Second key, phone pulls, 3 s heartbeat → Tasks 3 and 7. Vectors → Tasks 1 and 2. Handoff branch: left alone, as decided (not a prerequisite).
- **Gap I decided, not in the spec:** spec says "alarms pause the relay". `AudioManager` modes do not cover alarms; only calls (`MODE_IN_CALL`, `MODE_RINGTONE`, `MODE_IN_COMMUNICATION`) hold the relay. Alarms play over the Bose mixed with the relay. Flagged for Alan in the hand-off.
- **Types:** `Where`, `Event`, `Action`, `State`, `step` names are identical in Tasks 1, 3, 6, 7. `capture_start`/`CaptureHandle` defined in Task 4, used only there and in Task 3's thread.
- **Proportion:** the plan is about 2.5x the spec's length; the extra is Interfaces blocks and the vector case list, which are decisions, not code.
