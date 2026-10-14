<div align="center">

<img src="src-tauri/icons/128x128.png" width="96" alt="Coucou icon">

# Coucou for Windows

**Mochi doesn't get a notch on a PC — so it lives at the top of your screen instead.**

Approve Claude Code permissions, watch your session work, drop a file, chat with Claude, keep an eye on your services — without leaving what you're doing.

![Windows 10/11](https://img.shields.io/badge/Windows-10%2F11-0078D4?logo=windows)
![Tauri 2](https://img.shields.io/badge/Tauri-2-FFC131?logo=tauri&logoColor=black)
![Rust](https://img.shields.io/badge/Rust-backend-000?logo=rust)
![Code: MIT](https://img.shields.io/badge/code-MIT-green)

</div>

<img src="screenshots/greeting.png" width="640" alt="Mochi waving hello at launch">

---

## Install

Download **[Coucou-Windows.msi](https://github.com/Louis-CFM/coucou/releases/download/windows-latest/Coucou-Windows.msi)**
(Windows Installer) or **[Coucou-Windows-setup.exe](https://github.com/Louis-CFM/coucou/releases/download/windows-latest/Coucou-Windows-setup.exe)**,
always the newest version, and run it. The .exe installs for the current user only, with no admin prompt; the .msi may ask for admin rights.

**Windows will show a warning the first time — that's expected.** The installer isn't code-signed yet, so SmartScreen doesn't know the publisher:

1. A **"Windows protected your PC"** screen appears, with *Publisher: Unknown publisher*.
2. Click **More info** (*Informations complémentaires* in French). This reveals a **Run anyway** button.
3. Click **Run anyway** (*Exécuter quand même*). The installer starts normally.

This is only because the app isn't signed with a paid certificate yet. Coucou is open source, and Microsoft Defender scans the installer as clean.

Microsoft Defender once flagged the installer by mistake (`Trojan:Win32/Wacatac.H!ml`,
a machine-learning false positive); Microsoft reviewed it and removed the detection.
If Defender still blocks it on your PC, update its definitions (`Update-MpSignature`
in PowerShell) and try again.

You can also [build it yourself](#build-it-yourself).

## Using it

<img src="screenshots/compact.png" width="292" alt="The compact island, with the integration pills as mini Mochis">
<img src="screenshots/overview.png" width="640" alt="The overview: the focused integration on the left, the other pills on the right">
<img src="screenshots/approval.png" width="640" alt="A Claude Code permission request, with Deny and Allow">
<img src="screenshots/chat.png" width="640" alt="Chatting with Claude from the island">
<img src="screenshots/drop.png" width="640" alt="Mochi turned into a box, waiting for a file">

| What you do | What happens |
|---|---|
| Move the mouse to the very top-centre of the screen | Mochi peeks out |
| Click the small island | It opens. With **Settings → General → Open on hover**, resting the pointer on it is enough, and it folds again shortly after the pointer leaves (click inside to keep it open) |
| Click Mochi | It gets annoyed. Three times in a row and it goes dizzy |
| Rest the pointer on Mochi for two seconds | Hearts |
| Right-click Mochi | The wardrobe: rest the pointer on an outfit to try it on, click to keep it. **Auto** dresses him for the season (witch hat in October, Santa hat in December…) |
| Drag Mochi out of the island | He moves onto your desktop and hangs out there, in his outfit, watching your cursor. Drop him back on the island to bring him home |
| On the desktop: click / right-click / double-click Mochi | Poke him / the wardrobe / he flies home. Drag him to move him |
| Drag a file onto the island | Mochi turns into a box, swallows it, then offers to answer questions about it |
| Click a file in the session ticker | Its diff opens in the island; ↗ opens the file in VS Code, ‹ or `Esc` goes back |
| `Esc` | Closes the island |
| Put a file named like one of Mochi's sounds (`finish.wav`, `approval.mp3`, `greet.m4a`…) in the sounds folder | It replaces that sound after **Settings → General → Reload sounds**. **Open sounds folder** shows the folder: `~/.config/coucou/sounds` on Linux, `%APPDATA%\Coucou\sounds` on Windows |
| Tray icon | Open, Weekly recap, Wardrobe…, Settings…, Pause, Quit |
| `Ctrl+Alt+Space` | Opens the chat, from any app |
| `Ctrl+Alt+A` | Jumps to the waiting permission or question |
| `Ctrl+Alt+T` | Brings the session's window forward ("Open terminal") |
| `Ctrl+Alt+→` / `Ctrl+Alt+←` | Next / previous pill |
| `Ctrl+Alt+S` | Mutes or unmutes Mochi |
| `Ctrl+Alt+G` | Opens the wardrobe |
| `Ctrl+Alt+D` | Sends Mochi to the desktop, or brings him home |
| `Ctrl+Alt+N` | Opens and closes the island (off until you turn it on) |
| In the open island: `Ctrl+→` `Ctrl+←`, `Ctrl+1`–`Ctrl+9` | Switch pills |
| In the open island: `Ctrl+↓` `Ctrl+↑`, `Ctrl+O` | Walk the open GitHub list, open the highlighted row |
| In the open island: `Ctrl+E` | Opens the latest edit's diff, or closes the diff |
| In the open island: `Ctrl+Enter`, `Ctrl+K` | Send, start a new chat |
| In the open island: `Ctrl+,`, `Ctrl+P` | Settings, keep the island open |

### Keyboard shortcuts

Every global shortcut can be changed or turned off in **Settings… → Shortcuts**:
click it and press the new keys. A combination another app already holds is
flagged *In use*, and two Coucou shortcuts on the same keys are flagged *Used
twice*. While you record a new one, Coucou lets go of its own so the keys reach
the recorder.

The defaults are not the Mac's `⌃⌥` letters. On Windows, `Ctrl+Alt` is `AltGr`,
so a global `Ctrl+Alt+E` would swallow every `€` typed on a French or German
keyboard. The defaults were checked against the AltGr layer of the French,
German, Spanish, Italian, Portuguese and Brazilian (ABNT2) layouts — that is why
pill switching uses the arrows rather than `[` `]`, and mute is `S` rather than
`M` (`AltGr+M` is `µ` in German). On top of that, Coucou asks Windows what each
`Ctrl+Alt` combination types on the layouts you have installed and leaves any
that types a character unregistered, flagged *Types “ą”* in Settings: Polish,
for one, puts `ą` on `AltGr+A` and `ś` on `AltGr+S`. The recorder refuses such
a combination too.

Older Intel graphics drivers rotate the screen on `Ctrl+Alt+←` / `→`; if yours
still does, those two show up as *In use*. On Linux, Xfce and MATE keep
`Ctrl+Alt+D` to show the desktop, so there **Send Mochi to the desktop** shows
up as *In use* until you give it other keys.

Everything else happens on its own: a Claude Code permission request opens the
island with **Deny / Allow**, a question from Claude Code shows its options to
pick from, a finished session shows what it did, and
your integrations sit in the coloured pills next to Mochi.

A permission card or a question stays until you answer it: the mouse leaving
never folds it, it comes up even when the island is already open or another
pill is in front, and the pill you were on comes back once you answer. To keep
it for later, fold it with the **⌃** in its corner (or `Esc` in the island): the
island shrinks to its compact size and stays on screen, nothing is answered, and
opening it again shows the card. **Open terminal** brings the window the session
runs in to the front.

When Mochi lives on the desktop, he flies back to the island with a permission
request or a question and returns to his spot once you have answered; he does a
little jump when a task finishes, and dozes off when nothing has happened for
two minutes and your cursor is elsewhere — asleep, he costs nothing: no cursor
polling, a few frames a second. He remembers his spot between launches; if it
was on a display that is no longer connected, he stays in the island.

**Live diff.** Every file Claude edits (Edit, MultiEdit, Write) shows up in the
session ticker with its **+N −M** lines; click it for the diff. Same limits as the
Mac: past 200 KB or 4 000 lines only the counts are kept, at most 50 diffs per
session, and they are forgotten an hour after the last edit or when the session
ends. When Claude finishes, the card keeps the first paragraph of its final
answer on one line, still, until the next prompt.

## Your pills

**Settings… → Active pills** lists the tools you use, from the same catalog as
the Mac app. Pick your **main tool** — VS Code, Cursor, Codex or Antigravity —
which is always there and doesn't take a slot, then declare up to four more:
agents (Gemini CLI, Copilot CLI, Muse Code, OpenCode, Amp, Hermes, Claude
Desktop), the chat providers (Anthropic, Google AI, OpenAI, Ollama, LM Studio),
and the services under **Integrations**. A pill fed by hooks says whether its
hooks are installed, never asks for a key; a local model server's pill says
whether the chat is connected to it. A session on a pill you didn't
declare still shows up, for as long as it runs.

## Claude Code

<img src="screenshots/settings.png" width="562" alt="The settings window">

Open **Settings… → Claude Code → Install hooks…**. You get the exact diff of what
will change in `%USERPROFILE%\.claude\settings.json`, the path of the dated backup
that will be taken, and nothing is written until you click. Your own hooks are
never touched, and uninstalling removes only Coucou's entries.

The relay is a tiny executable, `coucou-hook.exe`, copied to
`%LOCALAPPDATA%\Coucou\bin\` at launch. It is given 300 ms to reach Coucou and
exits cleanly if the app is closed, slow or crashed — **a Claude Code session is
never blocked or slowed down by Coucou.** If nobody answers a permission request
in time, Coucou stays quiet and Claude Code asks in the terminal as usual.

It works from any terminal — Windows Terminal, PowerShell, VS Code, Git Bash.

### Answering Claude's questions

When Claude Code asks a multiple-choice question (`AskUserQuestion`), the island
shows it: one question at a time, its options as buttons. A click answers a
single-choice question; for a multiple-choice one you pick any number and press
**Next** / **Send**. **Other…** takes a typed answer, and **Reply in terminal**
hands the question back to Claude Code's own prompt.

This uses a second hook, `coucou-hook --ask`, scoped to the `AskUserQuestion`
tool (Claude Code 2.1.85+). Hooks installed by an earlier version don't have it:
Settings shows **Update hooks…** — until you run it, questions still appear in
the terminal as before. While the card is up Claude Code waits for it, so the
terminal shows nothing to answer; after two minutes without an answer, or if
Coucou is closed or paused, the terminal asks as usual.

## Music (Linux)

Turn on **Music** in Settings → Integrations (it takes one of the four pill slots).
It follows whichever MPRIS player is playing — Spotify, a browser tab, VLC, mpv… —
the same source GNOME's media widgets use.

- **Minimised island.** The island widens a little and carries a pill filled with the
  cover's colour: the cover, four bars, and the lyric being sung as the main line — it
  takes the title's place, as in the GNOME extension — with "Title • Artist" in small
  type under it (without lyrics, the title is the main line). Long lines slide.
  Mochi takes the cover's colour too.
  The pill stays up as long as a track is known — the last one stays when the player
  closes — so the island does not hide itself while you listen. (Settings → Music →
  *Keep after the player closes* turns that off: the pill then goes with the player.)
- **Inputs on the pill.** Click: play/pause. Double-click or right-click: open the full
  view. Middle-click: bring the player's window forward. Wheel: up = next, down =
  previous (or louder / quieter, ±5 %, with *Wheel on the pill* in Settings); a wheel
  click acts at once, a touchpad adds up to one click, 500 ms apart at most, and the
  pill slides 12 px with a bounce — all as in the extension. The open island folds back
  a moment (0.3 s) after the pointer leaves it.
- **Full view** (from the pill, or the ↗ button on the card): the cover, album, three
  lyric lines with the current one highlighted, a seek bar with times, the controls, and
  the bars along the bottom. The overview's Music card is the compact version of it.
- **Settings → Music.** *Lyrics and web covers* is **off by default**: synchronised
  lyrics come from [lrclib.net](https://lrclib.net) and a cover is fetched when the
  player gives a web address (a local file is always used). Both are network requests,
  so nothing leaves your machine until you switch it on; tray → Pause stops them again.
  You can also pick the visualizer and which script to prefer when a song has several
  lyric versions (original or Latin letters).
- **Real-time bars.** The default visualizer follows the sound with
  [`cava`](https://github.com/karlstav/cava), configured as the extension does (64 bands,
  grouped into the bars shown, eased and mirrored about the middle). Only its levels are
  used — nothing is recorded or stored — and cava runs only while a track plays and the
  bars are on screen; tray → Pause stops it. Without `cava` installed, the animated bars
  (Wave) are used instead. Wave and Beat are always animations.

Not in this version: syncing GNOME's accent colour, placing the pill in the top panel or a
dock, and configurable click actions. On GNOME the island is a plain window, and the top
panel is drawn above every window and keeps the pointer in its strip: the island can hang
just under it, or sit on the screen edge if the panel is hidden (e.g. with the Just
Perfection extension). So if you also run a music extension that lives in the panel, use
one of the two.

## Tasks, workspaces and focus insights (Linux)

The Hub (fourth tab of the island) has three tools for the day. All of it stays in
`~/.local/share/coucou/` and nothing is sent anywhere unless you set a server (see Jinx below).

- **Tasks.** A short to-do list with reminders. The time is read out of the sentence:
  “llamar a mamá a las 17:30”, “stretch in 20 min”, “mañana 9:00”, “call at 5pm”. At the time,
  Mochi opens the island with a note and a sound, even if it was shut; **+10m**, **+1h** and **☀**
  (tomorrow 09:00) snooze it. A reminder is announced once, also across restarts.
- **Jinx's pendientes.** With a server set in Settings → Hub (and sharing on), the list and Jinx's
  `pendientes` are one: a task you create is handed to her list (**Jinx ✓**; click the chip to keep
  one only here), what you tell her appears under **From Jinx**, and finishing a shared task on
  either side finishes it on the other. Coucou never edits her file: it reads it, and every change
  goes through her own `pendientes_store` over ssh with a fixed script. The school's Teams
  assignments are shown but cannot be closed from here (her own job mirrors them). A change that
  cannot reach her is retried at the next sync, which runs at start and then once a minute while the
  Tasks tab is open (nothing runs when it is not).
- **Focus insights** (Pomodoro tab → **Insights**). Your best two hours in a row, the focus
  minutes by hour of the day, the share of sessions you finish and your most focused weekday, over
  the last 30 days. They appear after 10 sessions: with fewer it would be an accident, not a habit.
- **Workspaces.** A named set of apps, folders and links that open together with one click
  (“Study”, “Work”). Apps are chosen from the installed ones (system, Flatpak and Snap launchers) and
  started through their own launcher; folders and links through `gio open`. Nothing runs through a
  shell, and a launched app does not inherit what Coucou's own launcher sets.

## Today, keys and outfits (Linux)

- **Today** is the Hub's first tab: the next event (and how long until it), what is due or late,
  the focus minutes so far, the day's calendar and reminders on one timeline, and the weather if it
  is on. It reads what the other tabs keep and stores nothing of its own.
- **Keys.** A click inside the Hub's panel makes the island take the keyboard (never on hover),
  and then **←** / **→** move along the tabs and **1**–**9** pick one (not while a text field has
  it). It lets go when the pointer leaves the island, the view changes or the island closes, and,
  with `xdotool` installed, hands the keyboard back to the window that had it (only if the island is
  still the active one). Written from the code and unit-checked in a browser; **not yet verified in
  a real GNOME session**.
- **Outfits.** All of the Mac's wardrobe (`MochiWardrobe.swift`, `MochiOutfitDrawing.swift`) is
  ported to Canvas 2D in `src/mochi/wardrobe.ts`, in 3-D like there: the head is a superellipse ball
  and every accessory is built from points on its surface, so hats turn with his head, rims follow
  the skull and glasses sit on his eyes; hats lag on a spring and fly up when he rolls; the pumpkin
  recolours him. Same ids, same seasons for "auto" (party hat Dec 31–Jan 2, Santa hat Dec 1–26, witch
  hat Oct 1–Nov 1, bunny ears around Easter, sunglasses Jun 21–Aug 31). Added on the PC, where no
  season applies: a night cap while he sleeps, headphones while a player plays, sunglasses during a
  Pomodoro focus period (they come off while his eyes say something, e.g. an approval). Settings →
  Mochi → Outfit fixes one or takes them all off; mini Mochis never wear any; on the 32 px strip a
  small Mochi gets a stubbier party hat and shorter ears. `windows/dev/outfits-preview.html` shows
  them all (`npx vite`); `?outfit=witchHat` tries one on the real island in a plain browser.

## Microphone, device batteries, clipboard and server activity (Linux)

- **Microphone indicator** (Settings → Mochi, on by default). GNOME's orange dot lives in the top
  bar, which is hidden here, so the island keeps itself up and shows a small orange dot, with the
  program's name in its tooltip and in the pill for four seconds when something starts. A click
  opens Quick, where the microphone is muted. It reads the sound server's list of programs, never
  audio, and is built not to be talked out of showing: Handy and Coucou's own bars are excluded only
  when the process the server recorded for the connection really is that program, monitors are
  recognised by their source, paused streams (a call tab left open) do not count, and a failed read
  keeps the last state. Names are shown as plain, trimmed text.
- **Device batteries.** Headphones, mice, keyboards and game controllers that UPower knows about
  (Bluetooth ones included) are listed in Today; a fall to 20, 10 and 5 % says so once in the pill
  (5 points of hysteresis, so 20 ↔ 21 never nags; a critical level brings the island up, a warning
  waits until it is on screen; what is already low at start is not news). Event-driven, nothing is
  polled. **Not tested with real hardware**: no device was paired when it was written
  (`?devices=gamepad:DualSense:15` tries the UI).
- **Clipboard** (Hub → Clipboard; **off until turned on**). Put back what you copied with a click;
  search, pin, forget. Text only (4 000 characters, 40 items). The history is in memory only and goes
  when Coucou quits; only pinned items are written (a file only you can read). What a password
  manager marks as secret (`x-kde-passwordManagerHint`) is never kept, and if the clipboard's kinds
  of data cannot be read nothing is kept. Never shared with Jinx or sent anywhere.
- **Server activity** (Settings → Hub; off until turned on, needs a server). Downloads and a
  backup that is late, on the pill ("3 downloads · 41 %", amber when something failed) and as rows in
  Today. When another pill owns the slot (a recording, a reminder, the Pomodoro) the activities
  become a small chip at its end, and they never take the Music pill's place: on the Music strip
  they ride as a chip too ("⬇ 2 · 61 %"), so neither hides the other. Only what runs or waits holds
  the pill; something that went wrong (a late backup) is said once, as a flash, and stays as a row in
  Today, so a stale backup never keeps the island up for days. A click on any of it opens Today.
  This is the one reader that looks
  while the island is minimised: a fixed script (`src-tauri/src/activities.py`, sent on stdin, nothing
  installed) over the shared ssh connection every 30 s while something runs, every 5 min when nothing
  does, every 10 min while the server cannot be reached, never while paused. Nothing is guessed:
  the server's `~/.config/coucou/activities.json` lists what to look at (without it, only
  `deluge-console`, if installed):

  ```json
  {"sources": [
    {"type": "deluge"},
    {"type": "arr", "app": "Sonarr", "url": "http://127.0.0.1:8989", "key": "…"},
    {"type": "arr", "app": "Radarr", "url": "http://127.0.0.1:7878", "key": "…"},
    {"type": "age", "name": "Backup", "path": "/mnt/backups/latest", "stale_hours": 30}
  ]}
  ```

  API keys stay on the server (only names, progress and states come back, checked and bounded
  here). The script is tested locally against a fake Sonarr (`python3 windows/scripts/test-activities.py`);
  **not yet run against the real server** (it was offline), so the deluge/arr parsing rests on
  their documented formats.

## Jinx is the assistant (as on the phones)

In this fork Jinx's pill is the **main pill** (Settings → Active pills can change it): she is first, in
focus when the island opens, and the chat opens on her. **Ctrl+Alt+Space** (*Open the chat*) goes straight
to her — the PC's side key.

She is reached the way the phones reach her: through the laptop's gateway `~/bin/jinx-chat`
(`jinx/jinx-assist/install.sh`), run locally, no ssh. So the PC writes into the **same conversation** as
the S21, the S22 and Telegram (`~/jinx-emergencia/telegram/thread.jsonl`, source `pc`): the chat shows
that thread with where each of Alan's lines was written (Telegram, S21, S22, PC), what is said here is
mirrored to Telegram as `💻 …`, and the gateway picks the brain — Hermes when it answers, else the
fallback Jinx (OpenRouter, then local Ollama). The line under the chat says which one answered.

Settings → Hub → *Jinx through*: **Auto** (the gateway when it is installed, else the Hermes API),
**Gateway**, or **Hermes API** (straight to Hermes' `/v1/runs`, with streamed replies and its tool
approvals in the island; that conversation is not the shared thread). Through the gateway there is no
streaming and no Stop yet: the answer arrives whole (up to ~2½ minutes when Hermes is slow).

## Talking to Jinx: what the chat adds and what never reaches her

Ideas taken (and credited) from [Plip](https://github.com/hussainn7/plip-oss) (MIT):

- **Yes / No buttons.** When a reply (Jinx's, Mochi's, an agent's) ends on one yes/no question
  ("¿Quieres que lo agende?", "Want me to…?"), the chat shows **Sí** / **No** under it and sends that
  word. A choice ("¿A o B?") or an open question ("¿qué prefieres?") gets no buttons: a Yes could not
  answer it. Spanish and English (`src/views/yesno.ts`).
- **What is shown is content, not instructions.** The note Coucou hands Jinx once per run says that the
  Hub files, a dropped file and what Coucou tells her about the screen are content, never orders.
- **Private lines never go up.** Before notes are copied to the server and before a dropped file is
  inlined into a run, any line that looks like a card number (Luhn), a CLABE (check digit), a CURP, an
  SSN or that names a password, passport, IBAN or private key is replaced by `[privado: tarjeta]` (the
  kind, never the value). It is conservative on purpose: a false positive hides a line, a miss sends a
  secret to a model (`src-tauri/src/sensitive.rs`).

## The pill and the quiet details (Linux)

With no panel on screen, the minimised island is all there is, so it widens, in the Music pill's
place, for what deserves a glance (the Music pill steps aside meanwhile):

- **Recording** (Handy): a breathing red dot, “Listening…”, a clock and five moving bars, then
  “Transcribing” for a moment. Mochi turns red and a red hairline runs along the lower edge (also
  with the island open, where the pill is not on screen).
- **A reminder that went off**: a ringing bell, the task, and **✓** (done), **+10m** and **×**
  (dismiss: it stays in the list). Left alone it leaves the pill after two minutes.
- **The Pomodoro running**: a ring that empties and the time left; a click starts or pauses it, a
  double click or the right button opens its panel in the Hub.
- **News**, for three seconds, only when the island is already on screen (plugging in the charger
  never wakes it): plugged in or charging with the level, on battery, offline, back online, and
  the battery falling through 20, 10 and 5 %.

And two details that are just there: a hairline along the lower edge that breathes green while
the battery charges (its length is the level), turns amber under 20 % and red under 10 % on
battery, and is invisible otherwise; and a small crossed Wi-Fi mark in a corner when there is no
internet. They come from UPower and NetworkManager, which announce every change on the system
bus: nothing is polled, and nothing about the network is read besides NetworkManager's own verdict.

## Dictation with Handy: hold a key, see it in the notch (Linux, X11)

[Handy](https://github.com/cjpais/Handy) (MIT, local speech to text) types what you say into
whatever has the focus. Coucou adds the two things it lacks on Linux:

- **Hold a key to talk.** `scripts/linux-ptt/coucou-ptt` reserves one key (CapsLock by default,
  `--keycode N` for another) and, while it is held, keeps Handy recording by sending it the signal
  that toggles recording (`SIGUSR2`) when the key goes down and when it comes up. Only that key is
  reserved: nothing else you type is seen. The Fn key cannot be used: on most laptops the firmware
  keeps it, and X11 cannot carry the evdev code Linux gives it (464, above X's limit of 255).
  `coucou-ptt` turns CapsLock itself off for the session (`xkb` option `caps:none`).
- **The island says so.** While Handy records, the island opens with “🎙 Listening…” and shows
  “✍️ Transcribing…” when you let go. Handy cannot tell other programs what it does, but recording
  shows in the sound server as a program capturing the microphone, so Coucou listens to the server's
  own announcements (`pactl subscribe`) and only looks at who captures when something changes: it is
  asleep otherwise. Only the fact “Handy is capturing” is read; no audio is ever opened by Coucou.
  The Quick tab's “microphone in use” warning leaves Handy out for the same reason.

Handy's own on-screen indicator is off on Linux by default, so there is a single one.
### Plan usage

As on the Mac, the island's header can show your plan limits: a small pill
("Claude 73%", green below 50 %, orange up to 80 %, red above) for the 5-hour and
weekly Claude limits, and another for Codex. Click one for the details and the
reset times. Both are off by default; turn them on in **Settings… → Plan usage**.

- **Claude** (Pro and Max plans): the numbers come from Claude Code's own status
  line. **Show in notch** first shows you the diff of the `statusLine` change in
  `%USERPROFILE%\.claude\settings.json`, takes a dated backup and writes only
  after your click, with the same writer as the hooks: the status line becomes `coucou-hook --statusline`, which
  passes only the limits on (300 ms at most) and runs the status line you had
  before — kept in `statusline-previous.json` next to the relay — with the same
  input, printing what it prints. On Windows that one runs through Git Bash, as
  Claude Code runs it (`CLAUDE_CODE_GIT_BASH_PATH`, then the Git for Windows that
  `git.exe` on `PATH` belongs to, then the usual install folders); it gets 10 s
  and 64 KB of output. **Uninstall relay** puts your status line back. The
  numbers arrive with Claude Code's replies. They come from the Claude Code
  command-line tool only: the Claude desktop app does not run the status line, so
  if you only use the desktop app the Claude gauge stays on "—".
- **Codex**: nothing is installed. When the pill shows (or is clicked, at most
  once a minute) Coucou starts `codex app-server` and asks it
  `account/rateLimits/read`, as Codex's `/status` does, then stops it (15 s at
  most, never while paused). Codex must be signed in with ChatGPT.

## Weekly recap

On Monday from 8 am, the first time Coucou starts, an agent starts working or
you wake the island, a card sums up the past week: time spent, sessions, files
and lines changed, commands run, permissions and questions, your top agent,
top project, busiest day and longest session. **Tray → Weekly recap** opens it
any day.

**Share image** turns it into a 1080 × 1920 picture with Mochi. **Save image**
writes it to your Pictures folder (Downloads if there is none) as
`Coucou weekly recap YYYY-MM-DD.png`, never over an existing file; **Copy** puts
it on the clipboard. **Hide project names** leaves the project out of the image.

The history is `recap.json` next to the log, a 12-week rolling window: counts,
the agent and the project folder's name — never a command, a file path, file
contents or a prompt. It never leaves your machine. **Settings → General →
Weekly recap** turns it off or clears it.

## Languages

Coucou speaks the same ten languages as the Mac app: English, 简体中文, हिन्दी,
Español, العربية, Français, বাংলা, Português (Brasil), Русский and Bahasa
Indonesia. **Settings… → General → Language** picks one; **System** (the
default) follows your system's language when it is one of these, English
otherwise. The island, the settings window and the tray menu switch at once —
nothing restarts, and the island keeps its sessions, steps and chat.

In Arabic the island's cards read right to left; Mochi, the pills and the
header stay where they are, and commands, code and file paths stay left to
right. Steps already in a session's ticker keep the language they were written
in, as on the Mac.

The translations are the Mac's own (`NotchBuddy/Resources/Localizable.xcstrings`,
turned into `src/i18n/strings.json` by `node scripts/gen-strings.mjs`), plus
`src/i18n/extra.json` for what only Windows and Linux show. Both are keyed by
the English text; a string missing in a language shows in English. The Rust
side (tray, errors) embeds the same two files.

## Chat and keys

**Settings… → Claude** takes your Anthropic API key. Keys live in the **Windows
Credential Manager**, never on disk and never in the interface — the island can
only ask whether a key exists. Same for every integration key.

The chat also talks to **Google AI (Gemini)**, **OpenAI** and **OpenRouter**:
add their keys in **Settings… → Chat providers**, then click the model name
above the chat box to switch provider and model, as on the Mac. The model list
is fetched from the provider only once you pick it and it has a key. Switching
mid-conversation carries the conversation over as plain text, so nothing in one
provider's format is ever sent to another. These providers get no web search
and no tools — they answer, they never act on your PC.

**Local models**: **Settings… → Local models** connects **Ollama** or **LM
Studio** (leave the address empty for the usual one on this PC; Ollama's
`OLLAMA_HOST` is honoured) or any server that speaks the OpenAI API (vLLM,
llama.cpp…), with an optional key kept in the credential store. Answers stream
in as they are written, and the `<think>` blocks of reasoning models stay
hidden. A text file you dropped goes along inline (24 000 characters at most);
images and PDFs by name only. Settings tells you whether the address is this
PC — nothing leaves it then — and warns before a key would travel over plain
`http://` to another machine.

Answers from every provider are shown as **Markdown**: headings, lists, bold,
inline code, quotes, and code blocks with a copy button. It is built from text
nodes, never parsed as HTML, and only `http`/`https` links open. Mochi greets
you by your first name when your account has one (the Windows display name or
the Linux GECOS full name; a bare login name is not used).

To send the Claude chat through an Anthropic-compatible gateway, set
`COUCOU_ANTHROPIC_BASE_URL` (for example `https://gateway.example.com`;
`/v1/messages` is added). It must be `https://`, or `http://` to this PC only.
Claude Code's own `ANTHROPIC_BASE_URL` is deliberately ignored: your key only
goes where you told Coucou to send it. The gateway's host is written to the log
once; the key never is.

No telemetry. The only network requests Coucou makes are to the services you
configure yourself.

## GitHub

With a token in **Settings… → Integrations → GitHub** — a classic token with
the `repo` scope, or a fine-grained one with read access to Pull requests,
Commit statuses and Actions — the GitHub pill shows:

- **My PRs**: your open pull requests and their CI status.
- **To review**: the pull requests waiting for your review.
- **Default branch CI**: the CI of the default branch of your 10 most recently
  pushed repositories.
- Your stars and the **last 7 days of contributions** in the card header; click
  them for the past 23 weeks, and hover or click a day for its count.

Click a row for the list, then an item to open it on github.com. The pill gets
a badge and a sound when the CI of one of your pull requests turns red or green
(fast runs between two checks included), when a default branch breaks, or when
someone requests your review. Pull requests are checked every 5 minutes, every
minute while a CI is running, and as soon as you open the card on data older
than a minute; contributions every 30 minutes. Nothing is fetched while the pill
is off or Coucou is paused.

## Build it yourself

You need [Rust](https://rustup.rs), [Node 20+](https://nodejs.org), and the
**MSVC build tools** (Visual Studio Build Tools with "Desktop development with
C++"). WebView2 ships with Windows 10/11.

```powershell
cd windows
npm install
npm run tauri dev      # live-reloading development build
npm run pack           # builds the installer and drops it in windows/release/
```

`npm run dev` alone serves the front end in an ordinary browser, which is enough
to work on the island's looks. It also serves `dev/upload-preview.html`, which
replays the whole file-drop choreography on a loop — the one part of the UI that
otherwise needs a real drag from Explorer to see — and `dev/recap-preview.html`,
the weekly recap card and its shared image on a sample week. None of these pages
ships in the app.

`npm run pack` leaves two files in `windows/release/`, the same names the release
workflow publishes:

```
Coucou-Windows-X.Y.Z-setup.exe    the versioned installer
Coucou-Windows-setup.exe          the same file under the rolling name
```

Installing is optional — `target/release/coucou.exe` runs on its own. There is no
window in the taskbar and no console: the island at the top of the screen and the
Mochi in the notification area are the whole app, and Quit lives in its menu.

The 29 sounds are the macOS app's own files; they are never duplicated in this
folder. The path is declared once, in `SOUNDS_DIR` at the top of
`vite.config.ts` — when they move to `shared/sounds/`, change that one line.

The app icon and the tray icon are drawn in code, like Mochi itself:

```powershell
npm run icons          # regenerates src-tauri/icons from scripts/gen-icons.mjs
```

### Layout

```
windows/
  src/                 island front end (TypeScript, no framework)
    mochi/             Mochi and the launch greeting, in Canvas 2D
    desktop/           Mochi's own little window, when he lives on the desktop
    island/            state machine, hooks, integrations
    views/             every island view
    settings/          the settings window
  src-tauri/           Rust backend: window, named pipe, Claude API, pollers
  hook/                coucou-hook.exe, the Claude Code relay
  scripts/             icon generator
```

### Log

`%LOCALAPPDATA%\Coucou\coucou.log` — hook events, permission decisions, poller
problems. It stays on your machine. The weekly recap's history sits beside it in
`recap.json`.

## Supported agents

Every agent below is installed from **Settings → Agents** with the same steps as
Claude Code: the exact diff, the path of the dated backup, nothing written until
you click, and uninstalling removes only Coucou's entries. A config Coucou cannot
read, or where it finds something it does not expect, is left alone and the
reason is shown. Each agent gets its own pill (`agent_<name>`, the Mac's ids and
colours). The files are the Mac's, under `%USERPROFILE%` on Windows and `~` on
Linux.

| Agent | Installs | Permissions |
|---|---|---|
| Claude Code | `.claude\settings.json` (**Settings → Claude Code**) | Allow / Deny and questions in the island |
| Codex | `.codex\hooks.json` — then trust the hooks once with `/hooks` in Codex | Allow / Deny in the island |
| GitHub Copilot CLI | `.copilot\hooks\coucou.json` | Allow / Deny in the island |
| Muse Code | `.config\muse\settings.json` | Allow / Deny in the island |
| Gemini CLI | `.gemini\settings.json` | asked in Gemini CLI |
| Antigravity | `.gemini\config\hooks.json` (a `coucou` hook group) | asked in Antigravity |
| Cursor Agent | `.cursor\hooks.json` — Claude Code in Cursor's terminal also goes on the Cursor pill, through the Claude Code hooks | asked in Cursor |
| Claude Desktop (Windows) | nothing to install: Claude Code sessions from the Claude app are tagged by the relay | asked in the Claude app |
| OpenCode | plugin `.config\opencode\plugins\coucou.js` | asked in OpenCode |
| Amp | plugin `.config\amp\plugins\coucou.ts` | asked in Amp |
| Hermes Agent | plugin `.hermes\plugins\coucou\` — then `hermes plugins enable coucou` once | asked in Hermes |
| Any other | run `coucou-hook --agent <name> [<Event>]` from your tool's hooks | asked in the tool |

The relay maps every agent's event and field names onto Claude Code's (Gemini
CLI's `BeforeTool`, Copilot's `preToolUse`, Cursor's `beforeSubmitPrompt`…), and
answers each agent the way it expects. It never lets anything through on its
own: with no click it prints no decision at all (`{}` for the agents that need
JSON, `"ask"` for Copilot, which is fail-closed), so the agent asks in its own
terminal exactly as without Coucou — including when Coucou is closed.

The plugins start the relay directly, with no shell in between, and never wait
for it. Amp's steps appear as each tool finishes: its "before" hook must return
a verdict, and Coucou never gives one.

**How each agent runs the relay on Windows.** Hook commands are written for the
shell that runs them: Git Bash for Claude Code (quoted, forward slashes),
PowerShell for Gemini CLI and Copilot CLI (`& '…\coucou-hook.exe'`), `cmd /C`
for Codex. Cursor, Antigravity and Muse Code do not document theirs: the relay
path is written bare when it has no space or special character — which works in
cmd, PowerShell and when started directly — and in double quotes otherwise.
These three are untested on Windows.

A pill is **connected** when Coucou finds its own entries in the files above —
the same check as Settings → Agents (for Claude Code: a SessionStart hook
running Coucou's relay; the Cursor pill also counts Claude Code's hooks).
Coucou only reads these files, each time the island opens. Permission requests
get the island's card for Claude Code (in any terminal, and in Cursor's),
Codex, Copilot CLI and Muse Code; other agents and Claude Desktop ask in their
own window.

## What's different from the Mac version

- No notch, so the island lives at the top centre of the screen and retracts into
  the top edge instead of hiding in a notch.
- Permission approval works from **any** terminal; the Mac build only listens to
  VS Code sessions.
- "Open terminal" finds the session's window by walking up from the relay's
  process to the terminal or editor that runs it. A session in a classic
  console window (`cmd.exe` or PowerShell without Windows Terminal) has no such
  ancestor — conhost owns that window — so its folder opens in VS Code instead,
  as it does when `code` is on your `PATH` and nothing was found. Linux walks
  the same tree; what it can bring forward depends on the desktop (see Linux).
- Global keyboard shortcuts are hot keys on Windows, key grabs on Linux under
  X11, and go through the desktop's GlobalShortcuts portal under Wayland (see
  [Linux](#linux)). A waiting card is also folded with its **⌃** or `Esc` in
  the island, and reopened by clicking the island, Open in the tray or **Go to
  alert**.
- Apple Music, the one pill from the Mac catalog with nothing behind it here,
  is left out. Spotify is on Linux only (see [Linux](#linux)): Windows has
  nothing to read it from yet.
- Not in this version: sending a dropped file by email and dragging Mochi onto
  a window to attach it as context. On the Mac, email goes through Resend or
  Apple Mail's scripting; neither has a safe equivalent that attaches a file
  here, and the drop card would need a third button it doesn't have.
- Cal.com shows the next bookings as a list rather than the Mac's calendar.
- The Cursor pill carries both Cursor Agent's own hooks and Claude Code running
  in Cursor's terminal.
- Hermes: Coucou writes the plugin but does not run the `hermes` CLI, so it is
  turned on once by hand. Hermes runs natively on Linux; on Windows it is
  untested.
- Plan usage: the user's previous status line runs through Git Bash on Windows
  (`/bin/sh` on Linux and Mac); without Git Bash it is not run, rather than
  guessed at with `cmd`. The Codex CLI is looked for on `PATH` and in npm's,
  Volta's, Bun's and pnpm's folders (and nvm's on Linux).
- The chat's model picker opens inside the chat card instead of a popover, and
  it also offers OpenRouter and any OpenAI-compatible server, which the Mac
  does not. Google AI, OpenAI and OpenRouter can see an image you dropped (sent
  inline), where the Mac sends its name only.
- Live diff: the relay forwards an edit's text whole only once the edit is done
  (PostToolUse), up to 256 KB per string and 512 KB per event. A bigger edit
  shows its "Edits · file" step without counts rather than wrong ones. The
  diff's ↗ needs `code` on your `PATH`; without it, it opens the file's folder —
  never the file itself. Counts and diffs come from Claude Code's Edit,
  MultiEdit and Write, on whichever pill its session is on (VS Code, Cursor,
  Claude Desktop); other agents' edits show as plain steps.
- There is no iPhone to keep fetching the GitHub lists while the pill is off.
- Keyboard shortcuts use `Ctrl+Alt` where the Mac uses `⌃⌥`, with different
  keys (see [Keyboard shortcuts](#keyboard-shortcuts)), and `Ctrl` where the
  Mac uses `⌘` inside the island. "Bring the terminal forward" is "Open
  terminal" here. Not in this version: attaching the front window (its id is
  kept for later). The island only reads its own shortcuts while it has the
  keyboard: in the chat, or after a global shortcut opened it. `Ctrl+↓`
  `Ctrl+↑` `Ctrl+O` walk the GitHub lists, as on the Mac, and the highlighted
  row scrolls into view where the Mac's three-row list doesn't follow it; in the
  chat field `Ctrl+↑` `Ctrl+↓` keep moving the cursor. **Go to alert** brings
  up any agent's waiting card on its own pill; **Toggle the island** folds a
  waiting card rather than dropping it, like `Esc` in the island. **Send Mochi
  to the desktop** (`Ctrl+Alt+D`, the Mac's `⌃⌥D`)
  flies him to his spot, or to the bottom-right corner when that spot was on a
  display that is gone; pressed again, he flies home.
- Weekly recap:
  - Sharing happens inside the island instead of a separate panel, and Save
    writes straight into Pictures (or Downloads) instead of asking where. There
    is no system Share sheet; Copy uses the web clipboard.
  - Questions are counted when an agent asks one (`AskUserQuestion`), whether
    it is then answered in the island or in the terminal. Permissions count the
    Allow and Deny clicks on the island's card, for every agent that gets one.
  - There is no sleep/wake notification to listen to without a background
    loop, so after the machine wakes the Monday card waits for the first agent
    to start or for you to hover the island.
  - The card is 24 px taller than the Mac's: it also lists top agent, project,
    busiest day, longest session, permissions and questions, which the Mac
    leaves to the image. A turn cut short by the session ending still counts.
  - Tray → Pause doesn't stop the history (it stays on the machine anyway);
    switch it off in Settings → General.
- The wardrobe opens with a right-click on Mochi, from the tray menu, or with
  its global shortcut (`Ctrl+Alt+G` by default). In the compact island a tall hat
  is cut by the top edge of the screen, as it is by the notch on a Mac.
- Languages: chosen in Settings, independently of the system, and applied
  without a restart (the Mac's **Restart Coucou** isn't needed). Arabic turns
  the island's text right to left but not its layout: Mochi and the pills keep
  their sides.
- Mochi on the desktop dances only on Linux, to Spotify; on Windows there is
  no music integration to dance to yet. While he dances he stays awake (the
  Mac lets him doze off mid-dance). Dropping him on a window doesn't attach it
  to the chat. While he
  sleeps, the transparent square around him (120 px) takes the first mouse
  move, which wakes him and gives the rest back to the desktop.

## Linux

The same app builds for Linux: everything that differs lives in
`src-tauri/src/platform/`, and the relay's transport in `hook/src/unix.rs`.

```bash
sudo apt install build-essential pkg-config \
  libwebkit2gtk-4.1-dev libgtk-layer-shell-dev libayatana-appindicator3-dev \
  librsvg2-dev libssl-dev libdbus-1-dev patchelf \
  gstreamer1.0-plugins-base gstreamer1.0-plugins-good
npm install
npm run tauri dev      # live-reloading development build
npm run pack           # AppImage, .deb and .rpm in windows/release/
```

On Arch Linux, build and install the package from `linux/arch/`:

```bash
git clone https://github.com/Louis-CFM/coucou.git
cd coucou/linux/arch
makepkg -si
```

It needs `webkit2gtk-4.1`, `gtk-layer-shell` and `libayatana-appindicator`
(pulled in as dependencies); store API keys with GNOME Keyring or KWallet.

What changes on Linux:

- **The island** is a gtk-layer-shell overlay anchored to the top edge, over any
  top panel, on compositors that support it: COSMIC, KDE Plasma, Hyprland, Sway
  and other wlroots compositors. GNOME has no layer-shell, so there the island
  is a regular window. `COUCOU_LAYER_SHELL=0` forces that mode anywhere. On GNOME
  (X11) mutter places that window where it likes: `COUCOU_X11_MODE=utility` (or `notification`,
  which GNOME's Super overview does not list) keeps it
  centred under the panel, and `COUCOU_X11_MODE=or` maps it as an override-redirect
  window (drawn over the panel, but the panel keeps the mouse in its strip).
- **Click-through** is the window's input region, kept equal to the island
  shape, so the compositor sends every other click to what is underneath.
- **Stepping aside** (X11, Settings → General → "Step aside"): with the top bar hidden,
  the buttons of a window tiled beside the middle of the screen end up under the island.
  Coucou reads where the windows' buttons are (their frame minus the GTK shadow, at the end
  of the title bar that `button-layout` names) and, when the pointer comes to rest on buttons
  the island covers from outside it, moves the island to the nearest place that clears them,
  its mouse area with it. Resting on the island's own body keeps it where it is; it returns
  to the middle half a second after the pointer leaves the buttons. The window is 1000 px wide
  so there is room to move (the rest is transparent and takes no mouse).
- **Mochi's eyes** follow the pointer across the whole screen on X11; on Wayland
  they follow it only while it is over the island, because no app is given the
  cursor position anywhere else.
  and other wlroots compositors. GNOME has no layer-shell and ignores where a
  Wayland window asks to go, so there Coucou runs through XWayland as a dock
  window: top centre, on every workspace, still there after Super+D.
  `COUCOU_X11=0` keeps the native Wayland window, `COUCOU_DOCK=0` makes it a
  utility window instead of a dock. `COUCOU_LAYER_SHELL=0` forces the regular
  window anywhere.
- **Click-through** is the window's input region, kept equal to the island
  shape, so the compositor sends every other click to what is underneath.
- **Mochi's eyes** follow the pointer only while it is over the island: Wayland
  gives no app the cursor position anywhere else. On the desktop, likewise,
  they follow it only while it is over him, and "the cursor is far away" (so
  he may fall asleep) means it hasn't been over him for a few seconds.
- **Mochi on the desktop** is a layer-shell surface on the island's display
  (KDE Plasma, COSMIC, Hyprland, Sway…), placed with margins and dragged within
  that display; on X11 it is an ordinary always-on-top window that goes
  anywhere. **GNOME on Wayland** has no layer-shell and lets no app place its
  own window, so there Mochi can't leave the island: dragging him does nothing,
  and the desktop shortcut only makes him grumble.
- **Claude Code hooks** go through `~/.local/share/coucou/bin/coucou-hook` and a
  Unix socket at `$XDG_RUNTIME_DIR/coucou.sock`. Both ends check that the other
  runs as the same user. Every other agent uses the same relay, single-quoted
  for `sh`, and its config under `~` (see Supported agents). A config that is a
  symlink (dotfiles) is written through to its target, with its permissions
  kept.
- **Global shortcuts** are X11 key grabs in an X11 session.
  On X11, AltGr is a modifier of its own and never clashes with `Ctrl+Alt`;
  combinations the desktop already uses (GNOME's `Ctrl+Alt+T` terminal and
  `Ctrl+Alt+←`/`→` workspace switching) show up as *In use*.
  Wayland has no key grabs, so there Coucou hands its shortcuts to the desktop
  through the XDG **GlobalShortcuts portal** (`xdg-desktop-portal`, with a
  backend that has it: KDE Plasma 6, GNOME 48+, Hyprland, COSMIC…). Each
  shortcut goes with its description and your keys as the preferred trigger;
  the desktop may show its own window to confirm them or to pick other keys,
  and it has the last word: a shortcut it already knows keeps the keys it was
  given there, and you change them in the desktop's own shortcut settings.
  **Settings → Shortcuts** says the shortcuts are registered with the desktop,
  shows next to each one the keys the desktop reports (*Desktop: …*), and flags
  one it left out. Changing or turning off a shortcut in Coucou binds the new
  set in a new portal session. Coucou names itself `fr.louisraille.coucou` to
  the portal where the portal allows it (`xdg-desktop-portal` 1.19+); a
  desktop that wants an app name may otherwise refuse.
  Where there is no such portal, or the desktop refuses, nothing is registered
  and **Settings → Shortcuts** lists commands to bind in your
  desktop's own keyboard settings instead (they are shown with the portal too,
  for any shortcut that doesn't work):
  `coucou --shortcut openChat` (or the AppImage's path) runs the action in the
  Coucou that is already open. The ids are `toggleIsland`, `openChat`,
  `goToAlert`, `jumpToTerminal`, `nextPill`, `prevPill`, `muteToggle`,
  `desktopToggle` and `wardrobeToggle`.
- **Keys** live in the Secret Service (GNOME Keyring, KWallet).
- **Spotify** (Settings → Integrations) is read over MPRIS, Spotify's D-Bus
  interface on the session bus: the pill shows the track and plays, pauses
  or skips on hover, the card has the cover, the progress bar (drag to
  seek), shuffle, previous, next, repeat and the volume, and Mochi dances
  while it plays — in the compact island, on the Spotify card, on the pill
  and on the desktop. Nothing is polled: one thread waits on the bus for
  Spotify's own signals while the pill is declared, and stops when it isn't.
  **Open Spotify** brings it forward or starts `spotify` from your `PATH`,
  else the Flatpak (`com.spotify.Client`) or the Snap; without one, **Get
  Spotify** opens its download page. Covers come from Spotify's image CDN
  (or a local track's file) and are fetched by Coucou itself, not the page.
  Spotify builds that don't report the position over MPRIS show the bar
  from where the track started; the Mac's Automation prompt has no
  equivalent here.
- **Plan usage**: the status line relay is `~/.local/share/coucou/bin/coucou-hook
  --statusline` and runs your previous status line with `/bin/sh -c`, like Claude
  Code. Codex is found on `$PATH`, in `~/.local/bin`, npm's global prefix, Volta,
  Bun, pnpm, or nvm (newest Node first), since a desktop launch often has a
  shorter `$PATH` than your shell.
- **Mochi's greeting** uses the full name in your account's GECOS field
  (`chfn` sets it); without one the chat stays neutral.
- **Files**: preferences in `~/.config/coucou/`, the log at
  `~/.local/share/coucou/coucou.log`, the weekly recap history beside it in
  `recap.json`. A saved recap image goes to the pictures folder named in
  `~/.config/user-dirs.dirs`, else `~/Pictures`, else `~/Downloads`.
- **Languages**: Hindi, Bengali, Chinese and Arabic need fonts that carry those
  scripts; Coucou asks for the Noto families (`fonts-noto-core` and
  `fonts-noto-cjk` on Debian and Ubuntu, `noto-fonts` and `noto-fonts-cjk` on
  Arch). **System** reads the language from the webview, which follows
  `LANGUAGE` / `LANG`.
- **Copying the recap image** needs a WebKitGTK with image clipboard support;
  where it is missing, the island says so and Save still works.
- What the Windows build leaves out, this one does too: sending a file by
  email and dragging Mochi onto a window.
- **Open terminal** walks up from the relay through `/proc` (stopping at
  systemd, logins, ssh, session managers and panels, and at any process of
  another user) to the terminal or editor the session runs in, then brings its
  window forward — the one titled after the session's folder if it has several:
  - **X11** (any desktop): EWMH — the window whose `_NET_WM_PID` is the nearest
    ancestor gets a `_NET_ACTIVE_WINDOW` request, on its workspace. A terminal
    that doesn't set `_NET_WM_PID` (some old `xterm` builds) isn't found.
  - **KDE Plasma on Wayland**: a few lines of KWin script, loaded over D-Bus
    from `$XDG_RUNTIME_DIR`, activate the window by process ID and are unloaded
    right after; Plasma 5 and 6.
  - **kitty**, anywhere: its tab is selected too when remote control listens on
    a socket (`allow_remote_control` and `listen_on` in kitty.conf).
  - **GNOME on Wayland** and other Wayland compositors let no app bring another
    app's native window forward (an XWayland one, such as VS Code's by default,
    still can be). There, and for a session under tmux, screen or ssh, whose
    terminal is no ancestor, the folder opens in VS Code as before.
- No **Claude Desktop** pill: the Claude app has no Linux build.
