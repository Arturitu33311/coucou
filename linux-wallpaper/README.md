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
