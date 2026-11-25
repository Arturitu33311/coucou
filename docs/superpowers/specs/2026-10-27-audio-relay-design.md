# Audio relay (PC → phone → headphones)

Status: draft for Alan's approval. Nothing is built.

## Why

Alan uses one pair of Bluetooth headphones (Bose QuietComfort Earbuds II) with both the Zorin laptop and the phone (S22 / S21).
The Bose have no multipoint (checked: Bose's multipoint update went to the QC Ultra line, not the QCE II), so today every switch means
disconnecting from one device and connecting on the other.

The daily pain is exactly that switch. The fix Alan chose: **the headphones stay connected to the phone, always** (the phone is
always within reach). Whatever plays on the PC is sent over the network to the phone, which plays it into the headphones, like
AudioRelay does. Moving the real Bluetooth connection to the PC stays possible, but only on purpose (manual).

## Goals

1. Sound from the PC comes out of the headphones without touching Bluetooth.
2. Only one thing plays at a time: when something starts on one device, what plays on the other is paused, automatically.
3. The PC never plays out loud by surprise: with the relay active, the laptop speakers stay silent.
4. Manual "move the headphones to the PC" (and back), one tap, from the island on the PC or a notification on the phone.
5. Music is the target use. A second or so of delay is acceptable for it.

## Non-goals (this project)

- Clipboard sync, calls, SMS, camera, KVM, Sidecar and the other ideas in Alan's list. Each is its own project.
  Order Alan chose: **relay first**, then the headphones-switch polish, then clipboard (phone → PC, then both ways).
- PC microphone through the headphones. The Bose mic is on the phone; video calls on the PC will not have it. Not solved here.
- Capturing the phone's calls. Android does not allow it.
- Perfect lip-sync for video on the PC. Delay is expected to be roughly 250-400 ms (network + Bluetooth); for video the manual
  move (goal 4) is the answer.
- Any public or LAN-open port. See "Security".

## Anti-goals (a failure even if it "works")

- The PC suddenly blasting sound through its speakers because the phone went out of reach.
- A loop: the phone relays its audio to the PC, which relays it back.
- Pausing something the person is actively using, or flapping between the two devices.
- Weakening the existing phone link (the SSH key that can run exactly one command).
- Doubled CPU use on the PC when nothing is playing (Coucou's rule: 0 % CPU when idle).

## Behaviour

State, per moment: the headphones are connected to **device H** (default: the phone). The other device is **device O**.

| Situation | What happens |
|---|---|
| Something plays on the PC, headphones on the phone, phone reachable | The PC's output is captured, compressed and sent to the phone; the phone plays it into the headphones. PC speakers silent. |
| The phone starts playing (music, video) while the PC relay is playing | The PC player is paused (MPRIS `Pause`, never a toggle). |
| The PC starts playing while the phone is playing | The phone's media is paused (media key `PAUSE`). |
| The phone becomes unreachable (out of range, Tailscale down) | The PC pauses what it plays and does **not** fall back to the speakers. When the phone is back, it stays paused (no auto-resume). |
| Headphones not connected to the phone | The PC plays on its own speakers as it always did (relay not active). |
| Alan taps "headphones to the PC" | The phone disconnects the headphones (or the PC asks for it); the PC connects them (`bluetoothctl connect`); H and O swap. Relay stops; the phone is now the one to be relayed or paused as O. Tapping again swaps back. |

"Active" requires: relay switched on in settings, the phone link on, and the headphones connected to the phone.

## What exists and must be reused

- **PC ↔ phone link:** `windows/src-tauri/src/remote.rs` — a private Unix socket reached from the phone by SSH with a key restricted
  to one command (`coucou-hook --remote`). Nothing listens on the network. The relay must not weaken this.
- **Presence / who is in use:** `presence.rs` ↔ `android/.../coucou/presence/Arbiter.kt`, pinned by `presence-vectors.json`.
  Useful to decide which device the person is actually on; the pause rules above are by "who started playing", not by presence.
- **Prior art on the same hardware:** branch `feat/music-handoff` (not merged into `unify/monorepo`): `handoff.rs`,
  `docs/MUSIC-HANDOFF.md`. It offers the song to the phone and makes the PC release the Bose (`Disconnect` + 20 s block, marker
  file for the block). That is the opposite direction of this design (headphones go to the phone). Its BlueZ handling and the
  `bluetoothctl block/unblock` safety (always lifted) should be shared, not rewritten. Decide at plan time whether to merge it first.
- **PC side tools present:** `wpctl`, `pactl`, `bluetoothctl`, PipeWire; MPRIS for players. `peripherals.rs` already reads the
  Bluetooth headphones' battery via UPower.
- **Phone side:** the Android app (`android/`, GPL-3.0) already has a notification listener, a system-event receiver and a
  Bluetooth screen (`BluetoothExpanded.kt`). It has no audio playback or media-key code.
- `zorin-connect` (GSConnect) is installed on the laptop for the manual clipboard button. This project does not touch it.

## Constraints

- Linux PC with PipeWire; the audio source is a monitor of the output the apps play to (or a virtual sink they are moved to).
- Android: phone-audio capture is not needed (phone plays its own audio). Playing the received stream needs a foreground service
  (`AudioTrack`) and must keep running with the screen off. Target: the S22 only.
- Transport must work over Tailscale (outside home Wi-Fi) as well as on the home LAN.
- Rust on the PC (Tauri app, existing crates; avoid new heavy dependencies), Kotlin on the phone. The codec should be Opus (or an
  equally common one) so neither side writes its own codec.
- Secrets never in the repo. No telemetry. Network only to the user's own paired phone.
- Both sides ship together with a shared contract (a JSON vector file, like `presence-vectors.json`) for the pause/ownership rules,
  verified with `bash scripts/check-contracts.sh`.
- Swift (`NotchBuddy/`) is not touched. Verification: `cd windows && npx tsc --noEmit`, `cd windows/hook && cargo test`,
  `cd android && ./gradlew :app:compileDebugKotlin`.

## Security

- No new open port reachable by anyone else. The audio travels over an authenticated, encrypted channel between the two paired
  devices only (the existing SSH link, or a stream inside it, is the starting assumption). If a separate channel is needed for
  latency, it must be paired by the same key material, listen only on the Tailscale/loopback interface, and be off unless the relay is on.
- The phone may only send "I'm playing / I'm paused / headphones state" and receive audio. It must not gain any new power over
  the PC beyond pausing the PC's player.
- The PC must never record anything but the headphones-bound output (no microphone, no other sources).

## Decided by Alan

- Headphones stay on the phone by default. Moving them to the PC is a manual action.
- Relay carries what plays on the PC to the phone (not the reverse).
- With the relay active the PC is silent on its speakers.
- If the phone cannot be reached, the PC pauses; it does not play out loud.
- Automatic pause of the other device when one starts.
- Build order: relay first.

## Left to the builder (with my suggested defaults, marked as my calls)

- Transport: stream audio inside the existing SSH link vs. a second channel (my guess: start with the SSH tunnel; measure delay;
  only add a second channel if it is unusable).
- Codec and buffer size: Opus at roughly 64-128 kbit/s, small jitter buffer (my guess).
- How the PC output is captured: a monitor of the default sink vs. a dedicated virtual sink (my guess: a virtual sink named
  "Coucou relay" that becomes the default only while the relay is active, so the speakers go quiet and restore on stop).
- How "who started playing" is detected: MPRIS on the PC; `MediaSessionManager`/active playback on Android (my guess: notification
  listener already present gives `MediaSession` access).
- Whether the manual "move headphones" is part of this project or the next one (my guess: include the PC `bluetoothctl connect` and a
  button, since the relay is incomplete without an escape hatch).
- Settings placement and wording (follow the existing Hub / Phone settings style).

## Decided by Alan, second round

- **Phone playback that pauses the PC:** music and video (media sessions) only. Notifications, navigation voice and other short
  sounds do nothing. (Calls and alarms were not discussed: my call is that they pause the relay while they last, and the PC stays
  silent, never falling back to the speakers. Change if wrong.)
- **Target phone:** the S22 only. The S21 is out of scope; no older-Android support work.

## Calls made by me after the builder check (not discussed with Alan; change any of them)

- **Transport and the key:** the existing key and `coucou-hook --remote` stay exactly as they are. The relay gets its own
  restricted command (`coucou-hook --relay`, streams audio only, accepts no input beyond control bytes) on a **second key** made
  for it. Losing the relay key loses the audio and nothing else. The phone connects (it already dials the PC); the PC never
  listens on the network.
- **Who starts it:** the phone pulls. It opens the relay when the headphones are connected to it and the setting is on, and
  reconnects by itself with back-off. The PC streams only while a phone is attached.
- **Never blasting the speakers:** while the relay is active, the PC's default output is a virtual sink ("Coucou relay"). If the
  phone vanishes, the default stays on that silent sink, so nothing can sound, even from apps with no MPRIS (browsers, games);
  MPRIS `Pause` is sent as well where available. The phone is declared gone after 3 s without audio or heartbeat.
- **Crash safety:** a marker file (like the handoff branch's `bt-hold.json`) records the previous default sink; Coucou restores it at
  the next start. A systemd user unit with `ExecStopPost` restores it if the app is killed. Silence after a crash is not acceptable.
- **Phone audio focus:** the relay takes audio focus on the phone. The phone's own media is paused by the pause rule first; focus is
  the safety net so the two never mix loudly if the pause is slow.
- **Headphones state:** the phone reports it (it is the one holding them); the PC cross-checks with BlueZ only for the manual swap
  (so a swap never races a third device).
- **Contract vectors:** one new file, `relay-vectors.json`, for the pure state machine (events in: who started/stopped playing,
  headphones connected/lost, phone gone, swap; actions out: pause PC, pause phone, start/stop relay, set sink). Transport is not in
  the vectors. Add it to `scripts/check-contracts.sh`.
- **Music handoff branch:** not a prerequisite. With the headphones living on the phone, handoff ("release the Bose to the phone")
  is redundant; while the relay is on, handoff stays off. The BlueZ block/unblock helper is shared if both end up merged.
- **Video on the PC:** just plays, delayed. No warning in v1.

## Success

- Alan plays music on the PC, and it comes out of the Bose connected to the S22, without any Bluetooth tap, with no audible
  stutter on the home Wi-Fi.
- Starting a video on the phone pauses the PC; starting music on the PC pauses the phone.
- Walking out of Wi-Fi/Tailscale range pauses the PC and never makes the speakers sound.
- One tap moves the Bose to the PC, and one tap moves them back.
- Idle PC (nothing playing): no measurable CPU from the relay.

## Open measurements (do first, in a spike, before the full plan)

1. End-to-end delay PC → S22 → Bose over home Wi-Fi and over Tailscale (target < 500 ms, ideal ~300).
2. Whether the S22 keeps playing the stream with the screen off and in Doze.
3. Whether the Bose connect to the PC by themselves after the phone releases them (the handoff branch left this untested).
