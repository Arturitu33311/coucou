# Jinx as the phone's assistant — phase 1: two-way text

Date: 2026-10-13 · Status: draft for review · Branch: `fix/tertiary-collapsed`

## Goal

On the S21 Ultra and S22 Ultra, Jinx can be chosen as the *Digital assistant app*
(Settings → Apps → Choose default apps). Long-pressing the side key opens a panel where
Alan writes to Jinx and reads the answer, **in the same conversation he has with Jinx on
Telegram**, and sees what Claude Code sessions and Jinx's pendientes are doing.

Out of scope for phase 1: voice (STT/TTS), "Hey Jinx", reading the screen, acting in
other apps. They come in later phases and must not need a redesign of this one.

## Decisions taken (2026-10-13)

1. **Mirror to Telegram: yes.** What is said at the phone appears in the Telegram chat
   (`📱 …` for Alan's text, `🟠 …` for Jinx's answer), so Telegram never loses context.
2. **The gateway lives on the laptop, temporarily.** The real brain (Hermes, on `server`)
   is down since 2026-10-09. The gateway answers with the fallback Jinx (JINX-CORE +
   synced memory, OpenRouter → local Ollama). It moves to / forwards to Hermes when the
   server is back (see "Open questions").
3. **Each phone is independent (2026-10-13).** The assistant must work on the S21 and
   S22 with no laptop, no other phone and no internet, using an on-device model when
   offline. The laptop gateway is an optional upgrade, never a requirement. See
   "Independent mode".

## What exists today (verified)

| Piece | Where | Used for |
|---|---|---|
| ssh with the phone's own key, one command per connection | `coucou/phone/Ssh.kt`, `PhoneKey.kt`, `PhoneSettings.kt` (`LAPTOP` profile, host key pinned) | pendientes (`JinxClient.kt`), Claude Code stream (`agents/AgentRemote.kt`) |
| Jinx's messages on the island, with Mochi's mood | `coucou/presence/JinxMsgUi.kt` (`mochi:<Mood>:<kind>`) | answers shown when the island is visible |
| Compose outside an Activity | `service/OverlayViewTreeOwners.kt` | optional |
| Telegram relay with brain, history (20 msgs) and logs | `~/bin/jinx-telegram`, `~/jinx-emergencia/telegram/history-<device>.json`, `logs/telegram-*.jsonl` | fallback Jinx |
| Device priority and heartbeat (laptop > s21 > s22) | `jinx-telegram` `BEATS` | who answers Telegram |

Two facts that shape the design:

- **Telegram's Bot API cannot send as Alan**, and only one consumer may call
  `getUpdates`. So the phone never talks to Telegram: it talks to a gateway, and the
  gateway mirrors with `sendMessage` (which does not conflict with the relay's polling).
- **The laptop's authorized key is pinned to one forced command**
  (`coucou-hook --remote`, a Rust binary that does not read `SSH_ORIGINAL_COMMAND`).
  A second command needs a dispatcher in front. The phone already sends a command
  name in the ssh exec string (`stream("coucou")`), so the Android side needs no change
  to be dispatched.

## Architecture

```
side key ─▶ JinxVoiceInteractionService ─▶ startAssistantActivity ─▶ JinxAssistActivity (Compose)
                                                                       │ JinxChatClient (ChatApi)
                                      ssh  laptop  "jinx-chat"         ▼
                          authorized_keys: command="coucou-dispatch"  ──▶ jinx-chat
                                                                           ├─ thread (flock'd files)
                                                                           ├─ Brain (shared with relay)
                                                                           ├─ mirror → Telegram sendMessage
                                                                           └─ sessions ← coucou state
```

### Laptop side

**`~/bin/coucou-dispatch`** (new, ~15 lines, forced command for the phone keys):
reads `SSH_ORIGINAL_COMMAND`: `coucou` → `exec coucou-hook --remote` (today's
behaviour, byte-for-byte); `jinx-chat` → `exec ~/bin/jinx-chat`; anything else → exit 2.
No shell is ever given. `authorized_keys` is edited by `jinx-phone-onboard` (it already
owns the phone-key lines); the old lines are backed up first.

**`~/bin/jinx-chat`** (new, Python): one request line on stdin, NDJSON events on stdout.

| op | request | events |
|---|---|---|
| `history` | `{"op":"history","n":40}` | `{"ev":"history","items":[{ts,src,text}]}` |
| `send` | `{"op":"send","text":"…","cid":"<uuid>"}` | `{"ev":"ack"}` `{"ev":"delta","t":"…"}`… `{"ev":"done","mode":"or\|local","text":"…"}` or `{"ev":"error","msg":"…"}` |
| `sessions` | `{"op":"sessions"}` | `{"ev":"sessions","items":[…],"pendientes":[…]}` |

`cid` makes `send` idempotent: a retry after a dropped connection returns the stored
answer instead of asking the model twice.

**Shared thread (`~/jinx-emergencia/telegram/`)**: today `Brain` in `jinx-telegram`
loads its history once at start and writes it after each reply, so two writers would
overwrite each other. Change:

- Extract `Thread` + `Brain` from `jinx-telegram` into `~/bin/jinx_thread.py`
  (importable by both). `Thread` takes an `flock` on `thread.lock`, **re-reads**
  `history-laptop.json` before every reply and writes it atomically (as now).
- New append-only `thread.jsonl` with `{ts, src: "tg"|"s21"|"s22"|"jinx", text}` for
  display. `history-*.json` stays what it is (model context, trimmed to 20).
- `jinx-telegram` is changed only to use `Thread` and to append to `thread.jsonl`;
  its loop, priority, 409 and webhook rules are untouched (its `--self-check` still
  forbids `deleteWebhook`/`setWebhook`/`logOut`).

**Mirror**: after the answer, `sendMessage` to the allowed chat with token/chat from the
vault (same source as the relay). Failure to mirror never fails the answer; it is
retried from `thread.jsonl` on the next request (`mirrored` flag per line). While the
relay is *idle* because the real server is up, the gateway still answers (it is a
separate, explicit entry point) and labels the badge "Jinx de respaldo".

**`sessions`**: reads the live Claude Code sessions and pendientes that the laptop's
Coucou already holds (same data `AgentUi.sessions/pending` shows). If the laptop's
Coucou is not running: `items: []` and the panel says so; it is not an error.

### Android side (`coucou/assist/`, new)

- `JinxVoiceInteractionService`, `JinxVoiceInteractionSessionService` and a stub
  `RecognitionService`, declared in the manifest with
  `res/xml/jinx_voice_interaction.xml` (`supportsAssist="true"`,
  `supportsLaunchVoiceAssistFromKeyguard="false"` in phase 1). The stub recogniser is
  there because the assistant list expects the attribute; **to verify on the emulator**
  that the app is listed without one.
- `JinxVoiceInteractionSession.onShow` → `startAssistantActivity(JinxAssistActivity)`.
  A normal Compose Activity is simpler to build and test than a session window.
- `JinxAssistActivity`: bottom panel with the thread (Alan / Jinx / source badge),
  an input box, Mochi in the mood of the last answer, a "respaldo" badge, and a small
  strip with Claude Code sessions and open pendientes. It works while the phone is
  locked only for reading; sending asks to unlock (`requestDismissKeyguard`).
- `ChatApi` (interface) + `SshChat` (real) + `JinxChatProtocol` (pure parse/format, unit
  tested like `JinxProtocol`). Reuses `Ssh.open(context, PhoneSettings.profile(LAPTOP))`;
  no new key, no new pairing screen.
- When the island is visible, the answer is also posted as a Jinx message
  (`mochi:<Mood>:info`) so Mochi shows it; the mood comes from the same mapper the
  island already uses.
- Laptop unreachable: the app's own `ChatEngine` answers (see "Independent mode"). Only
  if that engine cannot answer either (no internet and no downloaded local model) the
  text waits in the outbox with a visible "pendiente" state. It never fakes an answer.

## Independent mode: each phone has its own Jinx (requested 2026-10-13)

Requirement: on the S21 and S22 the assistant works **by itself**, with no laptop, no
other phone, and no internet. This replaces the earlier idea of using the phones' Termux
as gateways: Termux does not start after a reboot, depends on battery settings and on
sshd, and an in-app engine strictly dominates it.

### The phone is a complete stack

```
JinxAssistActivity ─▶ ChatEngine (in the app, always available)
                        ├─ ThreadStore   app-private file, same {ts,src,text,cid,mirrored} lines
                        ├─ Persona       JINX-CORE.md + MEMORY.md, cached copy (see below)
                        ├─ Brain         1) OpenRouter direct, if there is internet
                        │                2) on-device model (LiteRT-LM), if not / if it fails
                        └─ Mirror        Telegram sendMessage, queued until there is internet
              ▲ optional, when reachable in 3 s:
              └─ Laptop gateway (`jinx-chat`): richer brain (tools, Claude Code sessions,
                 pendientes) and the canonical thread. Never required.
```

**Selection per message:** laptop gateway if it answers the 3 s probe → else the
in-app engine. The panel's badge says which one answered ("laptop", "este teléfono ·
nube", "este teléfono · sin conexión"). The last one that worked is tried first.

### On-device model

- **Runtime: LiteRT-LM (Kotlin API).** Google's MediaPipe LLM Inference API is in
  maintenance-only mode and points to LiteRT-LM for Android. Alternative kept in
  reserve: llama.cpp via a JNI binding (GGUF, CPU), if LiteRT-LM misbehaves on these
  devices.
- **Model: a Gemma 2B-class `.litertlm`** (Gemma-4-E2B is listed at about 2.6 GB;
  Gemma-3 1B is the smaller option). Same family as `gemma4:e2b`, which already
  answers Jinx well on the laptop. Final pick is made by the spike below, not now.
- **Reference numbers** (from Google's page, Samsung S24 Ultra, Gemma-3n-E2B, CPU):
  prefill ~111 tok/s, decode ~16 tok/s. The S21/S22 are older, so **expect slower;
  nothing is promised until measured on these two phones**.
- **Delivery**: the model file is not in the APK (≈2.6 GB). The app downloads it on
  Wi-Fi from a source Alan chooses (Hugging Face or the laptop over Tailscale), checks a
  SHA-256 pinned in the app, and stores it in app-private storage. Until it is there the
  panel says "modelo local no descargado" and offline answers are impossible, not faked.
  Updating the APK with `install -r` does not delete it.
- **Prompt budget**: JINX-CORE.md is 2.2 KB and MEMORY.md 3.5 KB (~1.7k tokens);
  fits a 2B model's context with room for the 20-message thread. The app keeps
  generation short (a max-token cap) because decode speed is the bottleneck.
- **Memory/battery**: the engine is created on first use and released after a few
  minutes idle; one generation at a time; cancelled when the panel closes. No
  background service, no hotword in phase 1.

### Persona and thread without the laptop

- `JINX-CORE.md` and `MEMORY.md` come from the laptop gateway (new ops `core`, whitelisted
  content only, never `USER.md`) and are **cached in app-private storage**. A minimal
  built-in persona (the SOUL-emergencia text, 1.2 KB, no personal data) is bundled so a
  phone that has never met the laptop still answers as Jinx, only knowing less.
- The thread is local. When the laptop is reachable the app sends its unsent lines
  (`op:"merge"`, deduped by `(ts,cid)`) and receives the ones it lacks. Two phones that
  never see each other still each keep a coherent thread; they converge through the
  laptop (or later the server). `history` for the model is always the tail of the local
  thread, so there is nothing else to reconcile.

### Secrets on the phone

- The OpenRouter key and the Telegram token (+ allowed chat id) are needed for the
  independent mode. They are provisioned **once** from the laptop gateway (op
  `provision`, only over the already-pinned, key-authenticated ssh) and stored in the
  Android Keystore-wrapped preferences, never in the APK or in logs.
- Decision for Alan to confirm in review: these phones already hold both secrets in
  Termux `~/.jinx-secrets`, so this adds no new class of exposure, but it does put them
  in the Coucou app too. If he prefers, independent mode can ship **without** the
  Telegram mirror and cloud brain (local model only) and add them later.
- Mirror from a phone follows the existing rule: only when the relay's priority allows
  it (heartbeats); otherwise lines stay `mirrored:false` and go out later, by this phone
  or by the laptop after `merge`.

### Spike first (before building the engine)

Measure, on the real S21 and S22 via `adb install` of a tiny test activity (no taps on
the phones): load time, prefill and decode tok/s, peak RAM, and temperature after 5
turns, for Gemma-4-E2B and Gemma-3 1B on CPU and GPU. Pass criteria for phase 1b:
first token under ~8 s warm, ≥ 5 tok/s decode, no OOM with Coucou's island running.
If a phone fails, the plan switches that phone to the smaller model or to "cloud only,
queue offline" and says so in the badge; it never pretends.

### Phases

- **1a** laptop gateway + app panel + `ChatApi`/laptop target (needs the laptop).
- **1b** independent mode: `ThreadStore`, `Persona` cache, cloud brain, mirror queue,
  `merge`/`core`/`provision` ops. Works with internet and no laptop.
- **1c** on-device model (after the spike): offline answers.

Termux relays keep doing what they did (Telegram fallback relay); the app does not
depend on them.

## Error handling

- Gateway unreachable / host key changed / key refused: reuse `Ssh.explain` messages;
  the panel shows them in plain words and keeps the text in the box.
- Model failure: `Brain` already falls back OpenRouter → local; if both fail the
  gateway returns `error` and nothing is appended to the thread as an answer.
- Two devices send at once: the `flock` serialises; each reply sees the other's turn.
- Telegram down: answer is delivered to the phone, mirror retried later.

## Security

- The phone's key can only run `coucou-dispatch`, which can only run two fixed programs.
  No shell, no arguments from the client reach a command line.
- `jinx-chat` never returns memory files, secrets or `USER.md`; only the thread, which
  is already in Alan's own Telegram chat.
- The vault token is read on the laptop only; it never goes to the phone.
- Text typed at the phone is mirrored to a chat only Alan is allowed in (the relay's
  allowlist applies to the chat id used for `sendMessage`).

## Testing

- Python: `jinx_thread.py` and `jinx-chat` get `--self-check` (history trim, flock
  re-read by two writers, `cid` idempotency, mirror failure does not fail the answer,
  `coucou-dispatch` rejects unknown commands). `jinx-telegram --self-check` keeps
  passing.
- Kotlin: unit tests for `JinxChatProtocol` (events, malformed lines, `error`), outbox
  ordering, mood mapping reuse. No network (fake `ChatApi`).
- Emulator `coucou34`: the app is listed under assistants, the side-key gesture opens
  the panel, a fake gateway answers, dark mode renders. Phones: **no taps are injected**
  (the user is using them); install with `install -r` on both, as usual.
- End to end on the laptop: a message from the panel appears in Telegram with `📱`,
  the answer with `🟠`, and Telegram's next message sees both in `history-laptop.json`.

## Samsung notes

Side key long-press defaults to Bixby. Alan sets *Side key → Press and hold → Digital
assistant* (wording varies by One UI version) and *Default apps → Digital assistant
app → Coucou/Jinx*. Phase 1 ships a short `docs/ASSISTANT.md` with these steps.

## Open questions (do not block phase 1a)

1. **Hermes' API.** The server is down, so how to inject a message into Hermes' own
   Telegram session is unverified (a `:8642` HTTP server was noted in memory, never
   read). When the server returns: read `gateway/` in `hermes-agent`, then add a
   `HermesBackend` behind the same `jinx-chat` protocol. The phone does not change.
2. **Where the gateway ends up**: laptop (temporary) vs `server`. Decided later; the
   protocol is the same.
3. **S22 key**: the S22's Coucou key is not authorized yet (see ADD-A-PHONE.md);
   `jinx-phone-onboard` handles it. Needed for the laptop target and for the one-time
   provisioning (`core`, `provision`) from the S22; independent mode runs without it
   once provisioned.
4. **On-device model choice and speed**: decided by the spike in "Independent mode"
   (Gemma-4-E2B vs Gemma-3 1B, CPU vs GPU, on the real S21 and S22). Nothing about
   speed is promised before that.
5. **Secrets in the app** (OpenRouter key, Telegram token): Alan confirms in review, or
   ships independent mode as local-model-only first (see "Secrets on the phone").
6. **Model download source**: Hugging Face on Wi-Fi vs the laptop over Tailscale; and
   the pinned SHA-256 is filled in at spike time.

## Implementation notes (2026-10-13, supersede the text above where they differ)

1. **No `sessions` op** in `jinx-chat`: Claude Code sessions already reach the phone through the `coucou`
   stream (`AgentRemote`); the panel reads `CoucouRuntime.agentUi`.
2. **No `delta` events in phase 1**: `send` emits `ack` then `done`; streaming arrives with voice in phase 2.
3. **Phone mirroring is unconditional**: `sendMessage` never competes with the relay's `getUpdates`, so the
   heartbeat/priority rule applies only to polling, not to mirroring.
4. **`coucou-dispatch` treats unknown commands as today's behaviour** (`coucou-hook --remote`), and only the
   exact string `jinx-chat` selects the gateway, so no existing client changes.

## Phases after this one (not designed here)

2 voice (STT/TTS, streaming answers) · 3 screen context (`AssistStructure`,
screenshot) · 4 actions (intents, then accessibility with confirmations) ·
5 hotword and Jinx-only extras.
