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

### Where things run (server first, the laptop as fallback)

| Piece | Main (server) | Fallback (laptop) | Clients' rule |
|---|---|---|---|
| Jinx's gateway `jinx-chat` + shared thread + Telegram mirror | `~/bin/jinx-chat` (`deploy-gateway-server.sh`); phones reach it through `server-dispatch` (forced command of their key) | `~/bin/jinx-chat` (`install.sh`) | `Failover` (ChatApi.kt) / `jinx_gateway.rs`: server first; unreachable → laptop, server skipped 2 min; server without the gateway → laptop, skipped 10 min; a `send` that broke after reaching the server is never re-sent to the laptop |
| Lines the laptop answered while the server was down | — | `jinx-chat --sync-to-server` (op merge both ways; run it on a timer) | — |
| Presence + her background mood | `mochi_presence.py` (`deploy.sh`) | devices decide among themselves (`decide`/`Arbiter`) | presence contract below |
| Pendientes | `coucou_pendientes.py` (`deploy-server.sh`) | — | — |
| Hermes | server | — | the gateway finds it locally on the server (`default_hermes_host`), over ssh from the laptop |

The laptop finds the server as the ssh host `server` (`~/.ssh/config`; `COUCOU_JINX_SERVER` overrides it, `""` turns it off).

### Where Jinx's replies show up (island first, Telegram only when nobody is looking)

The gateway sends each reply to the presence server (`source: jinx-chat`). When a device is in use
(`watched: true`), the reply shows on that device's island and does not go to Telegram; both lines are marked
handled, so they are never sent there later. When nobody is looking (`vga`), or there is no presence (the
laptop's fallback gateway), it goes to Telegram as before. The device whose Jinx chat is open does not repeat
the reply on its island. The laptop reads the presence inbox only while it is the device in use
(`presence.rs inbox_round`), as the phone does.

Telegram gets the lines as written, without any mark of who or where. The only mark is `⚠️`, in front of a
reply that a backup (OpenRouter, a local model) gave instead of Hermes. The thread keeps that in `via`.

### Jinx asking for permission (approvals)

Hermes asks before a command it flags as dangerous only when someone can answer. Its plain session chat
(`/api/sessions/{id}/chat`, what the gateway used) has nobody, so Hermes applied `approvals.unattended_mode`
(deny by default) and Jinx silently could not run those commands from the phones or the PC. Now:

1. The bridge (`jinx_hermes_chat.py`, on the server) sends through `/api/sessions/{id}/chat/stream`. An
   `approval.request` is written to `~/.hermes/state/coucou/approvals.json` and announced on the device in use
   (presence, kind `alert`). The turn then waits for Alan (Hermes gives 300 s). A Hermes without the stream
   endpoint falls back to the old `/chat`.
2. Gateway ops `approvals` (the list) and `approve` (`run_id`, `request_id`, `choice`) pass through to the bridge.
   It answers only what is in that file, with a choice Hermes offered (`POST /v1/runs/{run_id}/approval`).
3. Phone: Jinx's tab shows the card while a message waits (every 3 s) and while the tab is open (every 10 s).
   Allowing (once / this session) asks for the fingerprint or the screen lock (`ConfirmActivity`, the same as
   Claude Code's permissions on the island); denying does not. "Always" is never offered from the phone. A
   command the server had to cut (2000 characters) cannot be allowed there.
4. PC: while a message waits, the island polls the same list and uses the Hermes API's card; a click answers it.
   A question answered on the phone loses its card on the PC.
5. Telegram keeps Hermes's own approval prompt there.

Timeouts follow: a `send` may last ~490 s with a question (the bridge cuts at 80 s without one); the gateway
waits 500 s and the phone and PC 520 s.

The fingerprint is checked on the phone only: the server trusts the phone's ssh key, as it does for everything else.

### Jinx runs the island (the server's order queue)

Jinx (Hermes) lives on the server and cannot reach the PC, so the presence server keeps her orders in a queue and the
PC asks for them (a long request the server answers the moment an order arrives; nothing is pushed in, the laptop may be
off). One closed list of what she may ask, **checked three times** (her tool, the server, the PC) because what she reads
is not to be trusted:

| | where |
|---|---|
| The list and its rules (Python) | `jinx/jinx-assist/island_commands.py` |
| The same rules (Rust) | `windows/src-tauri/src/island_cmd.rs`; both held to `island-vectors.json` (`scripts/check-contracts.sh`) |
| The queue: `POST /api/island/command`, `GET /api/island/poll`, `POST`/`GET /api/island/result` | `jinx/vga-panel/server/mochi_presence.py` (in memory; an order the PC does not take in 90 s expires; 30 a minute) |
| Her tool and its skill | `jinx/jinx-assist/jinx_island.py`, `island/SKILL.md` → `~/.hermes/skills/devops/coucou-island/` |
| The actions | « OK Coucou »'s `voice::run_tool` (music, pills, open_app): one layer, two ways in; Mochi's mood and the Hub on the page (`island/island-cmd.ts`) |

Actions: `state` (what the island shows, now playing included), `music` (play/pause, next, previous, volume, search),
`pills` (list, add, remove, set main), `mochi` (a mood for 1–8 s), `hub` (open a tool), `open_app`, `close_app`.
Never in the list: approving or denying an agent's permission, sending mail, settings, keys, addresses, files, commands.
`close_app` asks the window to close (`wmctrl -ic`: the app asks to save) and only for the apps in Settings → Jinx and
Hub → *Apps Jinx may close* (whole names; Coucou itself never). Settings → *Jinx runs the island* turns all of it off.
Deploy from the laptop: `jinx/jinx-assist/deploy-island.sh` (the tool, and the presence server with the queue).
The PC needs `wmctrl` to close apps (`sudo apt install wmctrl`). Only the PC executes orders; the phones do not.

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

Last full merge: upstream `98a767c` (Coucou for Windows/Linux 0.5.0, Mac 0.5.1), on 19 Oct 2026; before it `1456bd3` (0.4.0).
What was decided — which side of each overlapping feature stays — is in `docs/MERGE-UPSTREAM-0.4.md` and
`docs/MERGE-UPSTREAM-0.5.md`; keep those choices on the next merge.
Upstream's `.github/workflows/android*.yml` and the Android jobs of `phone-link.yml` are left out for the same reason as
`android/`: delete them again if a merge brings them back.

`jinx/` is ours alone. `windows/` has our own features on top of upstream's (Jinx, presence, tasks, the ssh phone link):
expect conflicts there, and run the checks in `CLAUDE.md` plus `scripts/check-contracts.sh` before committing a merge.
`jinx/vga-panel` bundles `windows/src/mochi/wardrobe.ts`: run its tests whenever a merge touches `windows/src/mochi/`.

## The VGA panel

`jinx/vga-panel/build.py` bundles the PC's own Mochi drawing code (`windows/src/mochi/wardrobe.ts`) into the panel page.
It needs `windows/node_modules` (`npm ci` in `windows/`) or `ESBUILD=/path/to/esbuild`. `deploy.sh` builds, runs the tests
and publishes to the server with backups.
