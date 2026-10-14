# Layout of this repository

| Directory | What it is | License |
|---|---|---|
| `NotchBuddy/` | macOS app (upstream) | MIT |
| `windows/` | Desktop app for Windows and Linux (Tauri + Rust + TypeScript) | MIT |
| `relay/` | Relay (upstream) | MIT |
| `android/` | Android app: the island and Coucou on the phone. Derived from SmartIsland | **GPL-3.0** (`android/LICENSE`) |
| `jinx/` | Jinx's side: laptop assistant (`jinx-assist/`), the VGA panel on the server (`vga-panel/`) | MIT |
| `scripts/check-contracts.sh` | Fails if the PC's and the phone's shared test vectors differ | — |

`android/` keeps the SmartIsland history and licence. It no longer follows or contributes to the SmartIsland upstream.
Everything else tracks Coucou's upstream (`Louis-CFM/coucou`).

`jinx/server-side/` holds the sources of what runs on the home server outside this repo's other folders (Cat OS, the
homelab dashboard's edited files). `docs/history/` keeps the working notes of the Jinx-assistant build.

## Day-to-day

- Desktop: `cd windows && npm ci && npm run tauri dev` (needs the GTK/WebKit dev packages on Linux).
- Android: `cd android && JAVA_HOME=<jdk21> ./gradlew :app:assembleDebug` (needs `android/local.properties` with `sdk.dir`,
  which is machine-specific and ignored by git).
- Jinx tooling: `jinx/jinx-assist/install.sh` (laptop side), `jinx/jinx-assist/deploy-server.sh`, `jinx/vga-panel/deploy.sh`.
- Before sending anything: `bash scripts/check-contracts.sh`.

## Shared contracts

The PC (Rust) and the phone (Kotlin) must behave the same on two things, and each is pinned by one JSON file kept in both places:

- `windows/scripts/phone/merge-vectors.json` ↔ `android/app/src/test/resources/merge-vectors.json` (tasks: newest change wins)
- `windows/scripts/phone/presence-vectors.json` ↔ `android/app/src/test/resources/presence-vectors.json` (presence arbiter,
  `windows/src-tauri/src/presence.rs` ↔ `android/.../coucou/presence/Arbiter.kt`)

Change both together, then run `scripts/check-contracts.sh`.

### Jinx's mood (gateway contract)

Where Jinx's mood comes from, in the order the clients use it (`windows/src-tauri/src/jinx_gateway.rs`, `windows/src/core/jinx-mood.ts`,
`android/.../coucou/assist/JinxMood.kt`; gateway side `jinx/jinx-assist/jinx_chat.py`):

1. `done.mood` (and `mood` on a `history`/`merge` line of hers): one of the 19 names `happy love proud surprised excited
   confused shy sad curious bored scared grumpy smug laughing crying relieved determined flustered sleepy`, or absent.
   Never her background mood: only a mood that came with that reply (the hidden tag `[[mood:<emote>]]`, stripped by the
   gateway before the thread and Telegram; **not enabled**: nothing asks the brain for it yet).
2. Else the text rules pinned by `jinx-mood-vectors.json`.
3. `{"op":"mood"}` → `{"ev":"mood","emote":<emote>|null,"source":"state"|"text","emotion_state":{…}|null}`: her background
   mood now, from `emotion_state` in the server's `~/.hermes/STATE.md` (read over ssh) by `baseline_emote(state, hour)`.
   Dimensions are 0–120; `source:"text"` means the state could not be read.

An unknown name is ignored by every client. Hermes itself returns no mood with its replies (only text).

**Her background mood on her Mochi** (what she feels between messages): the presence server (`mochi_presence.py`, on the
server, where `~/.hermes/STATE.md` is local) answers every heartbeat with `jinx_mood` (`<emote>|null`, read at most once
a minute). The rules live in one module, `jinx/jinx-assist/jinx_baseline.py` (`baseline_emote`, all ten dimensions plus
`emotion_score`), used by the gateway's `op: mood` too; `deploy.sh` copies it next to the server. The PC
(`presence.rs` → `Presence.jinxMood` → `mochi/scene.ts wornEmote`) and the phone (`MochiPresence` → `JinxBackground` →
`MochiReactions.calmOrJinx`, and the chat header) wear it whenever nothing else moves him; null means calm.

### Which device has Mochi (presence contract)

One device at a time: the rules are `presence.rs decide` and `Arbiter.kt` (twins), pinned by `presence-vectors.json`.
The server calls the laptop `pc` (clients send `pc` and read `pc` as `laptop`; the server also accepts `laptop`).
Its `vga` means nobody is in use: the VGA panel has Mochi and no device shows him, unless one of them, screen on, was
touched less than 300 s ago (its answer is up to a heartbeat old). The panel itself draws Mochi only when it is the
active one. `POST /api/mochi/message` answers `watched: false` when the panel has him (maybe nobody in front of it):
whoever sends the notice must then also send it by Telegram.

## Taking changes from Coucou's upstream

```
git remote add upstream https://github.com/Louis-CFM/coucou   # once
git fetch upstream
git merge-tree --write-tree --name-only HEAD upstream/main    # dry run: lists the conflicts, touches nothing
git merge --no-commit upstream/main
```

**`android/` is never taken from upstream.** Since its 0.4.0, upstream has its own Android app (MIT, by another author)
in `android/`, the same directory as ours (GPL-3.0, SmartIsland-based). They are unrelated apps. On every merge:

```
git checkout HEAD -- android/                                   # our side for the files both have
git diff --name-only --diff-filter=A HEAD -- android/ | xargs -r git rm -qf  # drop the files only upstream has
git diff --cached --stat HEAD -- android/                       # must print nothing
```

Upstream's `windows/src-tauri/src/phone_link/` pairs with *its* Android app, not ours (ours goes over ssh, see
`android/COUCOU.md`). It is off by default; keep it rather than delete it, or every merge conflicts on it again.

Last full merge: upstream `1456bd3` (Coucou for Windows/Linux 0.4.0), on 14 Oct 2026. What was decided there —
which side of each overlapping feature stays — is in `docs/MERGE-UPSTREAM-0.4.md`; keep those choices on the next merge.
Upstream's `.github/workflows/android*.yml` and the Android jobs of `phone-link.yml` are left out for the same reason as
`android/`: delete them again if a merge brings them back.

`jinx/` is ours alone. `windows/` has our own features on top of upstream's (Jinx, presence, tasks, the ssh phone link):
expect conflicts there, and run the checks in `CLAUDE.md` plus `scripts/check-contracts.sh` before committing a merge.
`jinx/vga-panel` bundles `windows/src/mochi/wardrobe.ts`: run its tests whenever a merge touches `windows/src/mochi/`.

## The VGA panel

`jinx/vga-panel/build.py` bundles the PC's own Mochi drawing code (`windows/src/mochi/wardrobe.ts`) into the panel page.
It needs `windows/node_modules` (`npm ci` in `windows/`) or `ESBUILD=/path/to/esbuild`. `deploy.sh` builds, runs the tests
and publishes to the server with backups.
