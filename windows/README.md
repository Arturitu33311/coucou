<div align="center">

<img src="src-tauri/icons/128x128.png" width="96" alt="Coucou icon">

# Coucou for Windows

**Mochi doesn't get a notch on a PC — so it lives at the top of your screen instead.**

Approve Claude Code permissions, watch your session work, drop a file, chat with Claude, keep an eye on your services — without leaving what you're doing.

![Windows 10/11](https://img.shields.io/badge/Windows-10%2F11-0078D4?logo=windows)
![Tauri 2](https://img.shields.io/badge/Tauri-2-FFC131?logo=tauri&logoColor=black)
![Rust](https://img.shields.io/badge/Rust-backend-000?logo=rust)
![License: MIT](https://img.shields.io/badge/license-MIT-green)

</div>

<img src="screenshots/greeting.png" width="640" alt="Mochi waving hello at launch">

---

## Install

The downloadable installer is **temporarily unavailable**. Microsoft Defender
wrongly flags the unsigned installer as malware (`Trojan:Win32/Wacatac.H!ml`, a
machine-learning false positive). A report is under review at Microsoft, and the
installer will be published again once it is cleared and code-signed.

Until then, [build it yourself](#build-it-yourself): it takes a few minutes and
installs for the current user only — no admin prompt.

## Using it

<img src="screenshots/compact.png" width="292" alt="The compact island, with the integration pills as mini Mochis">
<img src="screenshots/overview.png" width="640" alt="The overview: the focused integration on the left, the other pills on the right">
<img src="screenshots/approval.png" width="640" alt="A Claude Code permission request, with Deny and Allow">
<img src="screenshots/chat.png" width="640" alt="Chatting with Claude from the island">
<img src="screenshots/drop.png" width="640" alt="Mochi turned into a box, waiting for a file">

| What you do | What happens |
|---|---|
| Move the mouse to the very top-centre of the screen | Mochi peeks out |
| Click the small island | It opens |
| Click Mochi | It gets annoyed. Three times in a row and it goes dizzy |
| Rest the pointer on Mochi for two seconds | Hearts |
| Drag a file onto the island | Mochi turns into a box, swallows it, then offers to answer questions about it |
| `Esc` | Closes the island |
| Tray icon | Open, Settings…, Pause, Quit |

Everything else happens on its own: a Claude Code permission request opens the
island with **Deny / Allow**, a finished session shows what it did, and
your integrations sit in the coloured pills next to Mochi.

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

## Chat and keys

**Settings… → Claude** takes your Anthropic API key. Keys live in the **Windows
Credential Manager**, never on disk and never in the interface — the island can
only ask whether a key exists. Same for every integration key.

No telemetry. The only network requests Coucou makes are to the services you
configure yourself.

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
otherwise needs a real drag from Explorer to see. Neither page ships in the app.

`npm run pack` leaves two files in `windows/release/`, the same names the release
workflow publishes:

```
Coucou-Windows-X.Y.Z-setup.exe    the versioned installer
Coucou-Windows-setup.exe          the same file under the rolling name
```

Installing is optional — `target/release/coucou.exe` runs on its own. There is no
window in the taskbar and no console: the island at the top of the screen and the
Mochi in the notification area are the whole app, and Quit lives in its menu.

The 28 sounds are the macOS app's own files; they are never duplicated in this
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
    island/            state machine, hooks, integrations
    views/             every island view
    settings/          the settings window
  src-tauri/           Rust backend: window, named pipe, Claude API, pollers
  hook/                coucou-hook.exe, the Claude Code relay
  scripts/             icon generator
```

### Log

`%LOCALAPPDATA%\Coucou\coucou.log` — hook events, permission decisions, poller
problems. It stays on your machine.

## What's different from the Mac version

- No notch, so the island lives at the top centre of the screen and retracts into
  the top edge instead of hiding in a notch.
- Permission approval works from **any** terminal; the Mac build only listens to
  VS Code sessions.
- Not in this version: sending a file by email, dragging Mochi onto a window to
  attach it as context, and jumping to a specific terminal window — "Open
  terminal" opens the working folder in VS Code when `code` is on your `PATH`.
- Cal.com shows the next bookings as a list rather than the Mac's calendar.

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
- **Claude Code hooks** go through `~/.local/share/coucou/bin/coucou-hook` and a
  Unix socket at `$XDG_RUNTIME_DIR/coucou.sock`. Both ends check that the other
  runs as the same user.
- **Keys** live in the Secret Service (GNOME Keyring, KWallet).
- **Files**: preferences in `~/.config/coucou/`, the log at
  `~/.local/share/coucou/coucou.log`.
- What the Windows build leaves out, this one does too: sending a file by
  email, dragging Mochi onto a window, and jumping to a specific terminal
  window — "Open terminal" opens the folder in VS Code.
