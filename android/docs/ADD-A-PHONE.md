# Adding a phone (s21, s22…) to Coucou and Jinx

One phone = one id (`s21`, `s22`). Presence, Mochi and Jinx's fallback relay all key on it.

## Coucou (presence, tasks, Mochi)

1. Install the APK (`scripts/install-phone.sh`), open **Coucou** settings.
2. **This phone**: it picks the id from the model (SM-S90x = `s22`, SM-G99x = `s21`); fix it by hand if wrong.
3. **1 · The phone's key**: copy the public line and, on the laptop:
   `jinx-phone-onboard coucou-key s22 'ssh-rsa AAAA…'`
   (appends it to `~/.ssh/authorized_keys` restricted to `coucou-hook --remote`).
4. The laptop relays each phone's beat to the other, so both arbitrate on the same inputs:
   the device with the screen on that was touched last shows Mochi (8 s margin).

## Jinx fallback (Telegram relay when the server is down)

Priority chain: server > laptop > s21 (`phone`) > s22. A lower device only polls when nothing above
it answers its heartbeat (`:18790` on each device's Tailscale IP).

1. `~/.config/jinx/phones.conf` lists the phones (name, `JINX_DEVICE`, Tailscale IP, sshd port, user).
2. In Termux on the phone: paste the line printed by `jinx-phone-onboard laptop-key`.
3. `jinx-phone-onboard relay s22` installs the relay, `~/jinx-start.sh`, and copies the secrets from
   another already-onboarded phone through the laptop (never displayed).
4. `jinx-sync-celular.timer` (every 10 min) then keeps core/soul/memory in sync, restarts a dead relay
   and pulls `telegram-<device>-*.jsonl` back to the laptop.

`jinx-phone-onboard status` shows what the laptop sees of each phone.
