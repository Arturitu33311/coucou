# Coucou on this SmartIsland fork

What was added on top of SmartIsland (branch `coucou`). Handoff and the sync model live with the PC app:
`windows/scripts/phone/README.md` and `docs/HANDOFF-2026-10-09.md` in Coucou's `linux-ask-user-question` worktree.

* `coucou/` — tasks with reminders (Mochi's desk), parser, store, exact alarms.
* `coucou/phone/` — the phone link: RSA key kept with the Android Keystore, host-key pinning, restricted ssh to
  the server (Jinx's pendientes gateway) and to the laptop (`coucou-hook --remote`). `Ssh.installProvider()` swaps
  Android's stripped "BC" for the real BouncyCastle once, at app start, atomically — do not touch the providers
  elsewhere: the platform's TLS needs "BC" (it holds the default BKS key store).
* `coucou/agents/` — Claude Code on the island (sessions, permissions, questions) and the task meeting
  (`tasks_sync`) over the same laptop link.
* `coucou/presence/` — Jinx's texts on the phone (written by other models; see the risks in the handoff).
* Island: modes ordered by `IslandMode.priority`; Coucou's own cards (`isCoucou`) never open Smart Island and are
  never torn down as "an app's notifications".

Tasks: each device works alone; the newest change wins when they meet (`TaskLogic.merge`, `reconcile`).
`app/src/test/resources/merge-vectors.json` must stay identical to the PC's `windows/scripts/phone/merge-vectors.json`.

Debugging tips: a crashed accessibility service shows in `dumpsys accessibility` ("Crashed services");
the crash itself is in `dumpsys dropbox --print data_app_crash` (a `logcat -c` erases the rest).

## How the island behaves (decisions worth not undoing)

**Whose face is Mochi's.** The Mochi in the secondary bubble (and in the pill when his desk is the only card) is
**Jinx's**: he takes the mood of her own card (`mochi:<Mood>:<kind>`, `MochiReactions.jinxMood`) while she speaks,
and otherwise only what is the room's (overdue, news, music, battery, hello, night). He never shows Claude Code's
work, rate limit or failures: that is the agent card's (`AgentUi.kt`), which wears its own face and colours.

**The window.** The touchable-region hook (`OnComputeInternalInsetsListener`) is blocked on the phones in use
(S21, S22; logcat says "Touchable region reflection unsupported"). Without it the island window cannot take touches
only on the pill. So in that mode it is split in two (`SmartIslandOverlayService.syncTouchProxy`): the island
window is **always full width** and draws everything, with `FLAG_NOT_TOUCHABLE` while closed; a pill-sized window
(`touchProxy`) takes the touches and forwards them, offset, to the island's view. Never resize it sideways: for a
frame or two the system shows the old buffer at the new origin, which was a visible jump, then a blink (the earlier
"ghost picture" workaround). `scripts/island-motion-check.sh` records an open/fold and fails if the island
vanishes or swings past where it rests.

**Closing is critically damped** (`sizeSpec` in `IslandOverlayView`): any bounce there reads as the island swaying.
The bubble keeps its bounce only when opening.

**Size of his art.** The pill and the bubble clip whatever falls outside the box he is drawn in, so at island size
his outfit uses its full fit and what floats above his head is drawn smaller, inside, shrinking him only while it
shows (`GlyphFit`, covered by `GlyphFitTest`). Do not "make hats bigger" without running that test.

**Edge contrast.** In a dark system with a dark pill the pill gets a 0.75 dp rim of light, strongest on top
(`EdgeContrast`; setting Auto / On / Off in Positions). The fill stays the user's colour.

**The desk.** Tick a task: it fills, folds away, and is done, with *Undo* for 4.5 s. Swipe a row right: +10 min.
Hold a row: reword it (a time in the sentence moves the reminder). While typing, a chip says how the time was
understood. The count rolls when it changes. The reminder card offers +10 min, +1 h, Tomorrow 9:00, Done.

**Language.** Everything Mochi or the agent card says is in `values/` (English) and `values-es/`; add a language by
adding the file, not by editing code. The agent's *subject* (a path, a command) is shown as the laptop sent it.

**Rest.** His gestures rest while the screen is off and with the system's animations off.

## Phones

The build is installed on both phones with `scripts/install-phone.sh <ip:port>` (never uninstall: it would lose the
Keystore key and the phone link): S21 and S22, over Tailscale wireless debugging. **Never a plain `adb install`**:
the script records Play Store as the installer, and without it banking apps (Nu) refuse to open beside the island
(docs/BANK-APPS.md). Ajustes in the app warns when it was installed the wrong way.
