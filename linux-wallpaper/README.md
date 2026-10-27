# Coucou Linux wallpaper

The desktop version of the phone's "Coucou home" and "Coucou car" wallpapers: a live wallpaper page for Linux (GNOME,
or anything that can show a web page as the wallpaper, such as Hidamari's *Webpage* source).

What it shows
- slow lights and motes in the colours of the cover that is playing (the hour's colours when nothing plays), eased from
  song to song
- a big 12 h clock, the cover overlapping it, title and artist
- the line being sung, filling in as it goes (synced lyrics; long lines wrap and shrink instead of being cut)
- a small status gauge top right: battery arc and number, Wi-Fi (or wired) symbol, signal dots

How it works: `server.py` listens on 127.0.0.1 only and reads what is playing from any MPRIS player through `playerctl`,
the battery from `/sys/class/power_supply`, and the Wi-Fi level from `nmcli`. `wallpaper.html` draws it all on a canvas
at about 30 fps and pauses when hidden. Standard library only; nothing leaves the machine except, if you turn it on, the
lyrics lookup.

## The car layout (default when there is a car image)
Like the phone's lock screen: a big 12 h clock **behind** a car (the roof hides the foot of the digits, for depth), the car's
reflection on a glass floor, and a frosted-glass music card on the right (cover, title, artist, lyrics). There are no
sensors on a PC to tilt the picture, so it moves on its own:
- **Visualizer** on the floor in front of the car: what the PC is playing, as 32 bars (bass in the middle). The bass also
  swells the floor's glow and the lights, makes the car brighten and the cover breathe.
- **Slow drift**: every layer sways by a different amount (the nearer, the more), which reads as parallax.
- **A soft light slides over the car** every few seconds.
- Colours follow the cover (the hour's colours when nothing plays), eased from song to song; the car takes a little of
  them too.

The car image is **not** in the repo (photos have their own licences). Put a cut-out at
`~/.local/share/coucou-wallpaper/car/car_cut.png`, with its credit in `credits.txt` next to it. `tools/cut_car.py` makes
the cut-out from a photo (GrabCut, needs `cv2`; `--bg x0,y0,x1,y1` marks areas that are surely not the car). It works best
on a car whose colour differs from the background: a glossy black car confuses it. The author's example is a red 2021+
Cadillac Escalade (CC BY-SA 4.0, Ethan Llamas, Wikimedia Commons; credit in that folder's `credits.txt`). Without the file,
or with `?mode=classic`, the plain layout (clock, cover and text side by side) is used.

The composition is deliberately light and a little off-axis: the car is sized by its height as well as its width and leans
slightly, the clock and the card are turned a few degrees and the cover a bit more, and the lyrics live in a clipped region
under the artist so they can never overlap it.

The visualizer reads the default **output's monitor** with `parec` (a speaker's monitor, never a microphone), in memory, and
only while the page asks for it. It needs `numpy`; without it (or without `parec`) there is simply no visualizer.

## Try it
```
python3 server.py                 # http://127.0.0.1:8765/
python3 server.py --lyrics        # also look up synced lyrics (see below)
```
Open `http://127.0.0.1:8765/?demo=1` for made-up data (no player needed). `&still=1` skips the easing, for screenshots.

## Privacy
`--lyrics` is off by default. With it, the artist, title and album of each song are sent once to
[lrclib.net](https://lrclib.net), a free community lyrics database (no account, no key), and the answer is cached in
`~/.cache/coucou-wallpaper`. The cover is fetched from the URL your player itself published for it.

## Use it as the wallpaper
With Hidamari (Flatpak), choose the *Webpage* source and give it `http://127.0.0.1:8765/`. Keep `server.py` running:
`coucou-wallpaper.service` is a systemd user unit for that (edit the path in `ExecStart`, then
`systemctl --user enable --now coucou-wallpaper`).

Needs: `python3`, `playerctl`, and `nmcli` (NetworkManager) for the Wi-Fi level. Without a battery (a desktop PC) the
gauge shows only the connection.
