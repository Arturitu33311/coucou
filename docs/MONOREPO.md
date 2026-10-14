# Layout of this repository

| Directory | What it is | License |
|---|---|---|
| `NotchBuddy/` | macOS app (upstream) | MIT |
| `windows/` | Desktop app for Windows and Linux (Tauri + Rust + TypeScript) | MIT |
| `relay/` | Relay (upstream) | MIT |
| `android/` | Android app: the island and Coucou on the phone. Derived from SmartIsland | **GPL-3.0** (`android/LICENSE`) |
| `jinx/` | Jinx's side: laptop assistant (`jinx-assist/`), the VGA panel on the server (`vga-panel/`) | MIT |
| `scripts/check-contracts.sh` | Fails if the PC's and the phone's shared test vectors differ | — |

`android/` keeps the SmartIsland history and licence. It no longer follows or contributes to the SmartIsland upstream.
Everything else tracks Coucou's upstream (`Louis-CFM/coucou`).

## Shared contracts

The PC (Rust) and the phone (Kotlin) must behave the same on two things, and each is pinned by one JSON file kept in both places:

- `windows/scripts/phone/merge-vectors.json` ↔ `android/app/src/test/resources/merge-vectors.json` (tasks: newest change wins)
- `windows/scripts/phone/presence-vectors.json` ↔ `android/app/src/test/resources/presence-vectors.json` (presence arbiter,
  `windows/src-tauri/src/presence.rs` ↔ `android/.../coucou/presence/Arbiter.kt`)

Change both together, then run `scripts/check-contracts.sh`.

## Taking changes from Coucou's upstream

```
git fetch upstream
git merge upstream/main        # or: git log HEAD..upstream/main --oneline  to look first
```

Upstream only touches the directories it knows (`windows/`, `NotchBuddy/`, `relay/`, docs); `android/` and `jinx/` are ours.

## The VGA panel

`jinx/vga-panel/build.py` bundles the PC's own Mochi drawing code (`windows/src/mochi/wardrobe.ts`) into the panel page.
It needs `windows/node_modules` (`npm ci` in `windows/`) or `ESBUILD=/path/to/esbuild`. `deploy.sh` builds, runs the tests
and publishes to the server with backups.
