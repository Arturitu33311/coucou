# Jinx as the digital assistant (Samsung One UI)

1. Settings → Apps → Choose default apps → **Digital assistant app** → Device assistance app → *Jinx*.
2. Settings → Advanced features → Side key → Press and hold → **Digital assistant** (wording varies by One UI version).
3. First time only, to make the phone independent of the laptop: on the laptop run
   `touch ~/jinx-emergencia/telegram/ALLOW_PROVISION`, then open the panel and tap **Sincronizar**.
   The flag is deleted on use; the OpenRouter key and the Telegram bot are stored Keystore-wrapped.
4. Without laptop and without internet the panel keeps your text as pending and says so; it never invents
   an answer. An on-device model for that case is phase 1c (see the spike in the plan).

Notes:

- Right after `install -r` the service rebinds; the first assist gesture within a few seconds can land on Google.
- The laptop side lives in `scripts/jinx-assist/` (`install.sh` copies it to `~/bin`). The phone keys in
  `~/.ssh/authorized_keys` must run `~/bin/coucou-dispatch` (it still serves `coucou-hook --remote`).
