# The phone link: what travels, and the tests that pin it

The phone runs `ssh laptop coucou-hook --remote`; the laptop's Coucou answers on a private socket
(`remote.rs`). One JSON object per line. Phone to laptop uses `op`, laptop to phone uses `t`.

## State and tasks (this folder's fixtures pin the shapes)

| Direction | Line | Meaning |
|---|---|---|
| phone → laptop | `{op:"peer", device:"s21", idle, screen_on, server_ok, server_active, music, news}` | Who is using the phone, whether it reaches the presence server (and what it said), what it hears and sees. State only. Sent every 5 s and at once when music/news change or the phone is touched. |
| laptop → phone | `{t:"peer", device:"laptop", …, active}` | The same, for the laptop, as the answer to a beat and pushed on changes. |
| phone → laptop | `{op:"tasks_sync", tasks:[…]}` | The phone's whole list (tombstones included). Merged: the newest change wins. |
| laptop → phone | `{t:"tasks", tasks:[…]}` | The laptop's whole list, as the answer and whenever it changes. |

## Which device shows Mochi and Coucou (`presence.rs` ⇄ `Arbiter.kt`)

1. The presence server decides only when every device we can see reaches it and agrees on the answer,
   and the answer is one of ours (`s21`, `laptop`).
2. Otherwise the devices that hear each other decide: the one with a screen on that was touched most
   recently (8 s margin so a stray touch does not flip it).
3. A device that hears nobody and is not told by the server shows everything.

Peers expire after 15 s without a beat, on a clock (not on the connection closing). Only the painting
follows the answer: a permission request keeps its timeout and its answer while out of sight.
The server is optional: `presence-url` and `presence-key` in the keyring turn its heartbeat on.

## Tests

- `presence-vectors.json`: cases both `presence.rs` and the phone's `ArbiterTest` must satisfy.
- `merge-vectors.json`: cases both `tasks.rs` and the phone's `LwwSyncTest` must satisfy.
- `wire-fixtures.json`: one line of each kind, as each side writes it; each side reads the other's.
- `e2e-link.py`: a fake phone against the real binary (private X server and session bus).
  `COUCOU_BIN=target/debug/coucou python3 scripts/phone/e2e-link.py`
