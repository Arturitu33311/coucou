# Music handoff

You are listening on the computer, you pick up the phone and stay on it: after a while the computer offers the song
(or video) to the phone, and lets go of the Bluetooth headphones so the phone can take them.

Needs Settings → Phone on (the link to the phone). Off by default. Settings → Hub → **Music handoff**: *Off* / *Ask on the phone* / *Auto*, and how long (30 s – 5 min,
default 90 s; a video needs twice as long).

## What happens

1. The presence clock (`presence.rs`) says a phone has been the device in use for the whole dwell, that phone was touched
   in the last 45 s, and this computer was left alone (`handoff::due`).
2. Something is **Playing** in a player we know how to hand over, and it plays through a **connected Bluetooth audio
   device** (speakers stay where they are). `handoff::classify` decides what the phone is asked to open:
   - Sonora (Spotify): the track id comes from Sonora's own history (`plays` table, newest row with that title and first
     artist), so the phone opens `https://open.spotify.com/track/<id>`; if it can't be pinned, a Spotify search link.
   - A browser playing YouTube (`xesam:url`): `https://www.youtube.com/watch?v=<id>&t=<second>s`, the same second.
   - Anything else: nothing is offered.
3. The computer sends the phone a card (`{"t":"handoff",…}`, addressed to that one phone). It shows on the island
   ("Continuar en el celular — JESUS · Fuerza Regida · 2:31"). **Nothing is done to the music yet.**
4. The phone taps it. The only thing the phone can say is `{"op":"handoff_accept","id":…}`; the computer accepts it only if
   it is the offer that is waiting, for that phone, once, before it expires (2 min), and the person is still on that phone.
5. The computer pauses that player (MPRIS `Pause`, never a toggle), then `Disconnect`s the headphones over BlueZ and blocks
   them for 20 s so they don't reconnect here, and tells the phone (`handoff_done`). If letting go fails, it resumes playing.
6. The phone waits for the Bluetooth audio route. When it is there it opens the link (Spotify can't seek by link: the card
   shows where you were; YouTube opens at the same second). If the route never comes (25 s) it opens Bluetooth settings.
   It never opens the content before the headphones are on the phone, so nothing plays out loud by surprise.

*Auto* skips the accept: once the card is out, the computer pauses and lets go by itself. The phone still waits for your tap
to open the content.

## Safety

- Only https addresses of `open.spotify.com`, `youtube.com`/`youtu.be` ever leave the computer, and the phone checks them
  again (`HandoffRules.safe`). A lost phone can do nothing but accept an offer that is waiting.
- The Bluetooth block is always lifted: by a 20 s timer, and at the next start if the app died first (marker
  `~/.local/share/coucou/bt-hold.json`). If the Bose ever seem unreachable from the PC:
  `bluetoothctl unblock AC:BF:71:A1:21:71`.
- No multipoint on the Bose QC Earbuds II: only one device holds them. That is why the computer has to let go.

## Not done yet (phase 2+)

- Seeking in Spotify, or playing straight away: needs Spotify Connect through the Web API (OAuth, Premium).
- Phone → computer (the computer can `bluetoothctl connect` the headphones by itself; the phone side has to say what plays).
- Taking the headphones on the phone by itself: Android doesn't let an app connect a Bluetooth audio device silently.
  Whether the Bose jump to the phone after the computer lets go is **untested** (see below).
- Local-file players, and anything but YouTube for video.

## Test it (needs you and the Bose)

Build and install the PC and phone versions first (the phone with `scripts/install-phone.sh`, never a plain `install -r`).

1. Settings → Hub → Music handoff → *Ask on the phone*, dwell 30 s. Restart Coucou.
2. Bose on, connected to the PC (`bluetoothctl info AC:BF:71:A1:21:71 | grep Connected`). Play something in Sonora.
3. Leave the PC alone and use the S22 for 30+ s. A card should appear on the island. (`~/.local/share/coucou/coucou.log`,
   lines `handoff:`.)
4. Tap it. Expect: music pauses on the PC, the Bose disconnect (`Connected: no`), within ~25 s either the Bose are on the S22
   and Spotify opens at the track, or Bluetooth settings open on the S22.
5. **The thing to learn:** do the Bose connect to the S22 by themselves? If not, what does it take (take them out of the case,
   a button)? Then check the PC can use them again after: `bluetoothctl info … | grep -E "Blocked|Paired"` should say
   `Blocked: no`, `Paired: yes`.
6. Repeat with a YouTube video in Firefox/Chrome and the phone for 60 s (twice the dwell).
