# Audio relay ("Phone headphones")

The Bluetooth headphones (Bose QC Earbuds II, no multipoint) stay connected to the phone. What plays on the Linux laptop is
sent to the phone over the network and comes out of the headphones, so the headphones never have to be disconnected and
reconnected. Whichever device starts playing pauses the other. Moving the real Bluetooth connection to the laptop stays
possible, by hand.

Design: `docs/superpowers/specs/2026-10-27-audio-relay-design.md`. Plan: `docs/superpowers/plans/2026-10-27-audio-relay.md`.
Linux laptop + Android phone only (the S22). The phone must be able to reach the laptop over ssh (Tailscale name or LAN).

## How it works

- The phone dials the laptop over ssh with a **second key, audio-only**. That key may run exactly one command,
  `coucou-hook --audio`, which is a plain pipe to `$XDG_RUNTIME_DIR/coucou-audio.sock`. The Claude Code key and its
  `--remote` socket are untouched and cannot reach the audio socket (and the reverse).
- While the relay is on, the laptop's default output is a silent virtual sink (`coucou_audiolink`). Everything plays into it,
  `parec` reads its monitor, and 20 ms chunks of raw PCM (48 kHz, stereo, 16 bit, about 1.5 Mbit/s) go to the phone.
- The decisions (when to stream, when to stay silent, what to pause) are made by one small state machine
  (`audiolink_logic.rs`; `AudioLinkLogic.kt` is its twin). Both are pinned by `windows/scripts/phone/audiolink-vectors.json`
  (`bash scripts/check-contracts.sh`).

| Situation | What happens |
|---|---|
| Headphones on the phone, link up | Laptop sound goes to the phone; laptop speakers silent |
| Phone starts playing music/video | The laptop's players are paused (MPRIS `Pause`) |
| Laptop starts playing | The laptop asks the phone to pause its media |
| Link lost, or a phone call | Laptop goes silent and pauses; **never** falls back to the speakers; no auto-resume |
| Headphones leave the phone (blip) | Silent for 5 s; if they come back the stream resumes, otherwise the speakers are given back |
| Headphones not on the phone, or setting off | Laptop plays on its own speakers as always |

Notification sounds and navigation voice on the phone do **not** pause the laptop: only media sessions in `PLAYING` count.

## Set up

1. Laptop: Settings → Phone → **Phone headphones** on. Restart is not needed. (The `coucou-hook` copy in
   `~/.local/share/coucou/bin/` is refreshed when Coucou starts.)
2. Phone (Coucou → Phone link): pair the laptop as for Claude Code (step 3), then in **5 · Laptop audio through the headphones**
   tap *Make the audio key*, copy the line and paste it as a **second line** in `~/.ssh/authorized_keys` on the laptop
   (it starts with `restrict,command="…/coucou-hook --audio"`). Turn the switch on.
3. Connect the headphones to the phone. The status in that card says "connected to the laptop" when the link is up.

## Moving the headphones by hand

Settings → Phone → **Headphones**: *To this computer* / *Back to the phone*. Android does not let an app connect or
disconnect Bluetooth audio by itself, so the phone shows a notification that opens Bluetooth settings: one tap. The laptop
keeps trying to connect the headphones for a minute and does not give its speakers back during that time.

## If something goes wrong

- Laptop has no sound after a crash: Coucou restores it at the next start. By hand:
  `pactl set-default-sink <your real sink>` (see `pactl list short sinks`) and
  `pactl unload-module module-null-sink`. The marker with the real sink's name is `~/.local/share/coucou/audiolink-sink.json`.
  A small watchdog (`coucou-hook --audio-watchdog`) also restores it on its own the moment Coucou disappears.
- Log lines start with `audiolink:` in `~/.local/share/coucou/coucou.log`.
- Revoke the audio: delete the second line from `authorized_keys`; nothing else loses access.

## Test it (needs you, the S22, the Bose and the laptop)

Install the PC build and the phone build (`scripts/install-phone.sh`, never a plain `install -r`).

1. Bose connected to the S22. Play music on the laptop → it comes out of the Bose; laptop speakers silent.
2. **Delay.** Play a metronome on the laptop and tap a stopwatch on the phone at every audible click, 20 taps. Record the mean
   offset on home Wi-Fi and over Tailscale. Target under 500 ms, ideally ~300.
3. Screen off for 10 minutes while playing: no stutter, the service stays alive.
4. Start a video on the phone → the laptop pauses. Start music on the laptop → the phone pauses. A notification sound does
   nothing. A phone call → laptop silent; after the call the laptop is **not** resumed.
5. Turn off Wi-Fi/Tailscale on the phone: within about 3 s the laptop pauses and never plays on its speakers.
   Then `kill -9` Coucou on the laptop: within about 2 s `pactl get-default-sink` is your real sink again.
6. *To this computer* / *Back to the phone*: does the laptop connect the Bose right after the phone lets go? (This was never
   tested on this hardware.)
7. The ignored test `cargo test live_sink -- --ignored` (in `windows/src-tauri`) does the sink round trip on the real PipeWire;
   it refuses to run while anything is playing.

### Results

Not measured yet. Fill in: delay on Wi-Fi ___ ms, over Tailscale ___ ms; screen-off stutter yes/no; Bose reconnect to the
laptop after the phone lets go yes/no. If the delay is over 500 ms or it stutters on Tailscale, the first fix is Opus
(raw PCM is the v1 choice so as not to add a codec dependency).
