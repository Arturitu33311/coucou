# Clipboard sync (phone ⇄ PC) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

Status: **draft for Alan to decide the four open questions below**; nothing is built. It is the next part of the PC–phone
integration, after the audio relay (`docs/AUDIO-LINK.md`) and its manual headphones swap.

**Goal:** What Alan copies on the phone (above all verification codes) is on the PC's clipboard without pressing anything
in Zorin Connect, and what he copies on the PC can be pasted on the phone.

**Architecture:** One more kind of line on the phone ⇄ PC link that already exists (`remote.rs` on the PC, `AgentRemote`
on the phone, the `--remote` ssh key). Text only, short, never from a password manager, off until turned on. The phone
gets text to send from three places that Android allows (a notification that holds a code, a Quick Settings tile, the
share sheet), because Android 10+ does not let an app read the clipboard in the background. The PC → phone direction
needs no trick: writing the clipboard from the background is allowed.

**Tech Stack:** Rust (`remote.rs`, `clipboard.rs`), Kotlin (`NotificationListenerService` already in the app, a
`TileService`, a share `Activity`), shared JSON vectors checked by `scripts/check-contracts.sh`.

## Why (Alan, 2026-10-14 and 2026-10-27)

"Copy and paste from the phone to the PC… I have to press *send clipboard* in Zorin Connect." The thing copied is mostly
codes. Everything else in the original list (AirDrop, Universal Control, calls…) is a separate project.

## What already exists (verified in the repo and on the devices, 2026-10-31)

- **PC:** `windows/src-tauri/src/clipboard.rs` is the Hub's clipboard history: event-driven (`owner-change`), text only,
  4 000 characters, skips what a password manager marks secret (`x-kde-passwordManagerHint`), memory only, **"never sent
  anywhere"**. This plan deliberately changes that last sentence, behind a new opt-in, for the text and nothing else.
- **The link:** `windows/src-tauri/src/remote.rs` (private socket, phone reaches it by ssh with a key restricted to
  `coucou-hook --remote`), `android/.../coucou/agents/AgentRemote.kt` (reconnects itself, beats, replays). Both already
  carry other JSON lines (`peer`, `tasks_sync`, answers) with a shared test file (`wire-fixtures.json`, vectors).
- **Phone:** a `NotificationListenerService` (`SmartIslandNotificationListenerService`) already runs with access granted.
  No code reads or writes the clipboard (only "copy this line" buttons in the setup screens).
- **Today's workaround:** Zorin Connect / KDE Connect clipboard plugin. On Android 10+ it can only push when the app is in
  front, which is the manual button Alan dislikes. It also **mirrors media** as players on the other device, which is
  what broke the audio relay's pauses: sync must not touch that plugin.

## Open questions for Alan (answer before Task 1)

1. **Which phone → PC triggers?** Proposed: (a) codes found in notifications, automatic; (b) a Quick Settings tile "send
   clipboard to PC", one tap; (c) the share sheet "Send to PC". Not proposed: reading the clipboard in the background
   (Android blocks it; the workarounds, an accessibility service or Shizuku, are fragile and the banking-app notes in
   `android/docs/BANK-APPS.md` argue against adding more of them).
2. **Should the PC → phone direction be automatic** (everything copied on the PC lands in the phone's clipboard) **or on
   demand** (a button on the island)? Automatic overwrites the phone's clipboard while Alan may be pasting something.
   Proposed default: on demand, automatic available as a switch.
3. **Reuse the `--remote` key or make a third one?** Proposed: reuse. The phone can already answer Claude Code permission
   requests over it, which is far more than "put text in the clipboard", and a new key means another line pasted in
   `authorized_keys` for each device. If Alan prefers the audio relay's style (one tiny key per feature), Task 2 gains
   a hook command and a second socket.
4. **What does the PC do with an incoming text?** Proposed: put it in the clipboard and show a short note on the island
   ("From the phone: 482913"), so a text can never replace the clipboard unnoticed.

## Global Constraints

- Branch `feat/clipboard-sync` from `unify/monorepo`. Never push to `upstream`. Install the phone build only with
  `android/scripts/install-phone.sh` and never `adb uninstall`.
- Off by default on both sides; each direction has its own switch.
- Text only. At most 4 000 characters (the same limit as `clipboard.rs`). Control characters other than newline and tab
  are dropped. Empty or whitespace-only text is never sent.
- Anything a password manager marks secret on the PC is never sent. On the phone, a text from a notification is only a
  code if the pure extractor says so (Task 3); nothing else is read from notifications.
- The text is never logged, written to disk, or sent anywhere but the paired device. Only its length and direction may
  appear in logs.
- One shared vector file, `clip-vectors.json`, in `windows/scripts/phone/` and `android/app/src/test/resources/`; change
  both, then `bash scripts/check-contracts.sh`.
- No polling: the PC reacts to `owner-change`, the phone to notifications, tile and share.
- Do not change how Zorin Connect behaves; it stays installed.
- Verify: `cd windows/hook && cargo test`; `cd windows/src-tauri && cargo test`; `cd windows && npx tsc --noEmit`;
  `cd android && ./gradlew :app:testDebugUnitTest :app:compileDebugKotlin`.

## Review Focus

1. A copy loop: the PC sets the phone's clipboard, the phone sees "a change" and sends it back, forever. Pinned in Task 1.
2. A password copied on the PC from a manager that sets the hint must never leave the PC (and a long secret-looking text
   without the hint is the user's call: the switch is off by default). Pinned in Task 4.
3. A hostile or buggy phone line: huge text, binary, control characters, a flood of lines. Pinned in Task 1 and Task 2.
4. A notification that merely contains numbers (a delivery time, a price, a phone number) must not be sent as a code.
   Pinned in Task 3.
5. The clipboard replaced while Alan is pasting: an incoming text must be visible (Task 2) and, on the phone, Android's own
   "pasted from" toast must still appear (it does for `setPrimaryClip`; nothing in Task 6 may hide it).

## File Structure

- Create `windows/src-tauri/src/clip_sync.rs` (pure rules: limits, sanitising, origin/loop guard) and
  `android/.../coucou/clip/ClipSync.kt` (the same rules).
- Create `android/.../coucou/clip/CodeExtractor.kt`, `ClipTile.kt` (TileService), `SendToPcActivity.kt` (share target).
- Modify `remote.rs` (new op and line), `clipboard.rs` (publish a copy, set a text without echo), `AgentRemote.kt` /
  `RemoteProtocol` (send and receive), `SmartIslandNotificationListenerService.kt` (hand notifications to the extractor),
  `AndroidManifest.xml`, the settings screens on both sides, `scripts/check-contracts.sh`.
- Create `windows/scripts/phone/clip-vectors.json` and its copy under the Android test resources; `docs/CLIPBOARD-SYNC.md`.

---

### Task 1: The rules and the shared vectors

**Files:** Create `clip_sync.rs`, `ClipSync.kt`, both copies of `clip-vectors.json`; Test: in-module Rust tests,
`ClipSyncTest.kt`; Modify `scripts/check-contracts.sh`, `lib.rs` (`mod clip_sync;`).

**Interfaces:**
- Produces (both languages): `fn clean(text: &str) -> Option<String>` (drops control characters except `\n` and `\t`,
  trims, returns `None` when empty or longer than 4 000 characters after cleaning); `fn fingerprint(text: &str) -> u64`
  (stable hash of the cleaned text, same value in both languages); `struct LoopGuard` with
  `fn should_send(&mut self, text: &str, now_ms: u64) -> bool` and `fn note_received(&mut self, text: &str, now_ms: u64)`.
  The guard says no to a text whose fingerprint was received from the other device in the last 5 000 ms and to the same
  text sent twice in a row.
- Wire lines (JSON, one per line): phone → PC `{"op":"clip","text":"…","at":<unix ms>}`; PC → phone
  `{"t":"clip","text":"…","at":<unix ms>}`.
- Vector file: `{"clean_cases":[{"name","in","out"|null}], "fingerprint_cases":[{"text","hash"}], "guard_cases":[{"name","steps":[{"do":"received"|"send","text","at","expect":bool}]}]}`.

- [ ] **Step 1: Write the vectors and both failing tests.** Cases: `keeps_a_code` ("482913" → "482913"), `trims`,
  `drops_control_characters` (bell, escape sequences), `keeps_newline_and_tab`, `empty_is_none`, `whitespace_is_none`,
  `exactly_4000_ok`, `4001_is_none`, a non-BMP emoji survives; fingerprint values computed once by the Rust side and pinned
  in the file; guard: `an_echo_of_what_just_arrived_is_not_sent` (received "x" at 0, send "x" at 1 000 → false),
  `after_five_seconds_it_is_a_new_copy` (… at 5 001 → true), `the_same_text_twice_in_a_row_is_sent_once`.
- [ ] **Step 2: Run them to see them fail.** `cd windows/src-tauri && cargo test clip_sync`; `cd android && ./gradlew :app:testDebugUnitTest --tests "*ClipSyncTest*"`.
- [ ] **Step 3: Implement the three functions and the guard** in both languages (hash: FNV-1a 64 over the UTF-8 bytes).
- [ ] **Step 4: Run both and `bash scripts/check-contracts.sh`.** Expected: PASS, `ok: clip-vectors.json`.
- [ ] **Step 5: Commit** `clip: shared rules and vectors`.

### Task 2: The PC receives a text from the phone

**Files:** Modify `remote.rs`, `clipboard.rs`, `settings.rs`, `windows/src/core/state.ts`, `windows/src/settings/main.ts`
(+ the settings round-trip fixtures in `settings.rs` tests, which list every field).

**Interfaces:** Consumes Task 1's `clean` and `LoopGuard`. Produces `pub fn clipboard::set_from_phone(app, text) `
(sets the GTK clipboard, marks the owner so `owner-change` does not publish it back, tells the island "From the phone"),
and a setting `clipSyncFromPhone: bool` (default false).

- [ ] **Step 1: Failing tests** in `remote.rs`'s test module: `a_clip_line_from_the_phone_is_cleaned_and_bounded` (control
  characters dropped, 5 000 characters refused, `at` ignored for safety), `a_clip_line_is_ignored_when_the_switch_is_off`,
  `a_flood_of_clip_lines_is_rate_limited` (more than 5 in one second → the rest dropped), `a_clip_line_is_never_logged_with_its_text`.
- [ ] **Step 2: Run to see them fail.** `cd windows/src-tauri && cargo test remote::`.
- [ ] **Step 3: Implement `parse_client_line` handling of `op: "clip"` and `set_from_phone`.**
- [ ] **Step 4: Run the suite, `npx tsc --noEmit`.** Expected: PASS.
- [ ] **Step 5: Commit** `clip: the PC takes a text from the phone`.

### Task 3: Finding codes in notifications (phone)

**Files:** Create `CodeExtractor.kt`; Test `CodeExtractorTest.kt`.

**Interfaces:** Produces `fun extractCode(title: String?, text: String?, packageName: String?): String?`: returns the code
when the notification looks like a verification message, else `null`. A code is 4–8 digits (or 3+3 / 4+4 with a space or
hyphen, returned without it) that sits within 40 characters of a keyword (`código`, `codigo`, `code`, `verification`,
`verificación`, `OTP`, `PIN`, `clave`, `token`, `contraseña de un solo uso`) or in a notification from a messaging/SMS
package. Never returns a number that is a time (`10:30`), a price (`$1,200`), a year, or part of a longer number.

- [ ] **Step 1: Failing tests** (messages in Spanish and English; the spec's exact values go in): `bank_code_es`
  ("Tu código de verificación es 482913" → "482913"), `code_with_space` ("Your code: 123 456" → "123456"),
  `delivery_time_is_not_a_code` ("Llegamos a las 10:30, pedido 4821" → null), `price_is_not_a_code`, `phone_number_is_not_a_code`
  ("Llama al 5512345678" → null), `year_is_not_a_code`, `two_numbers_picks_the_one_by_the_keyword`, `no_text_is_null`.
- [ ] **Step 2: Run to see them fail.** `./gradlew :app:testDebugUnitTest --tests "*CodeExtractorTest*"`.
- [ ] **Step 3: Implement `extractCode`** (regex over the cleaned text; keyword proximity as above).
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `clip: find verification codes in notifications`.

### Task 4: The PC sends what is copied there (opt-in)

**Files:** Modify `clipboard.rs`, `remote.rs`, `settings.rs`, `state.ts`, `main.ts`, settings fixtures.

**Interfaces:** Consumes Task 1. Produces setting `clipSyncToPhone: bool` (default false) and `publish_clip(text)` on the
remote bus (`{"t":"clip",…}`), called from the existing capture path **after** the password-manager check and `clean`.

- [ ] **Step 1: Failing tests:** `a_secret_marked_by_a_password_manager_is_never_published` (the capture path with the hint
  set publishes nothing), `a_text_that_came_from_the_phone_is_not_published_back`, `nothing_is_published_when_the_switch_is_off`,
  `only_the_cleaned_text_is_published`.
- [ ] **Step 2: Run to see them fail.** **Step 3: Implement.** **Step 4: Run the suite.** **Step 5: Commit** `clip: the PC offers its clipboard to the phone`.

### Task 5: The phone sends (codes, tile, share sheet)

**Files:** Create `ClipTile.kt`, `SendToPcActivity.kt`; Modify `SmartIslandNotificationListenerService.kt`, `AgentRemote.kt`
(`sendClip(text)`), `AndroidManifest.xml`, `PhoneSettings.kt`, `PhoneSetupActivity.kt`, strings (`values`, `-en`, `-es`).

- [ ] **Step 1: Failing tests:** `a_code_is_sent_once_when_the_same_notification_updates` (a notification re-posted with the
  same text sends one line, the guard from Task 1), `nothing_is_sent_while_the_switch_is_off`, `a_clip_line_is_the_exact_json_the_PC_reads`
  (the encoder against `clip-vectors.json`).
- [ ] **Step 2: Run to see them fail.** **Step 3: Implement** the three triggers: the listener calls `extractCode` and sends;
  the tile opens a transparent activity that reads the clipboard (allowed when it is in front) and sends; the share target
  sends the shared text. Each shows a one-line confirmation ("Sent to the PC") and none stores the text.
- [ ] **Step 4: Run `:app:testDebugUnitTest :app:compileDebugKotlin` and `:app:assembleDebug`; install with `install-phone.sh`.**
- [ ] **Step 5: Commit** `clip: the phone sends codes, the tile and the share sheet`.

### Task 6: The phone receives (and the on-demand button)

**Files:** Modify `AgentRemote.kt`, `PhoneSettings.kt`, `PhoneSetupActivity.kt`; PC: a button on the island / Settings
("Send my clipboard to the phone") calling Task 4's `publish_clip` once.

- [ ] **Step 1: Failing tests:** `an_incoming_line_is_cleaned_before_it_reaches_the_clipboard`, `an_echo_of_what_the_phone_just_sent_is_ignored`,
  `nothing_is_set_while_the_switch_is_off`.
- [ ] **Step 2–4:** implement `setPrimaryClip` with a label (no sensitive flag set by us), the guard, the on-demand button
  and, if Alan chooses it, the automatic switch. **Step 5: Commit** `clip: the phone takes the PC's clipboard`.

### Task 7: Docs and the on-device pass (with Alan)

**Files:** Create `docs/CLIPBOARD-SYNC.md` (what it does, the switches, the privacy limits above, recovery: turn the
switches off), update `docs/AUDIO-LINK.md`'s "also see".

- [ ] **Step 1: On the S22 and the laptop:** copy a code from a real SMS; copy text on the phone and use the tile; share a
  paragraph; copy on the PC and press the button; copy a password in the password manager and check nothing leaves the PC;
  copy the same text on both sides and check there is no loop. Record results in the doc.
- [ ] **Step 2: Commit** `clip: docs and first on-device results`.

## Self-review

- **Spec coverage:** the goal's two directions → Tasks 2/5 and 4/6; "codes" → Task 3; the privacy limits → Global Constraints
  and Tasks 2 and 4; loop and flood risks → Review Focus items 1 and 3.
- **Not here (YAGNI):** images and files (a different channel; AirDrop-style transfer is its own project), a synced history
  with SQLCipher/CRDT (the PC already has a history; sync would need its own design), Handoff of URLs, SMS mirroring.
- **Risk to check first:** whether the tile's transparent activity really gets clipboard access on Android 16 on the S22
  (Android's rule is "the app that has focus"); if not, Task 5's tile becomes a notification action. Spike it in the first
  hour of Task 5, before writing the rest.
