# Coucou — guide for AI coding agents

Coucou is a native macOS app (`NotchBuddy/`); `windows/` is the Tauri version for Windows and Linux. Mochi, a small animated character living in the MacBook notch, shows AI coding agent sessions (Claude Code, Gemini CLI, Antigravity and more) and a few integrations, and lets the user approve, answer, chat and drop files from the notch.

## Where things are
- `NotchBuddy/Sources/App/` — Mac-only Swift code. `NotchBuddy/Sources/CoucouKit/` — code shared with the iPhone app (Mochi's BotEngine and outfits, pills, diff, models). `NotchBuddy/Sources/Phone/` — iPhone app (`CoucouPhone` target), with its widgets in `Sources/Widgets/` and the expanded approval notification in `Sources/NotificationContent/`. `NotchBuddy/Resources/sounds/` — the 28 WAV sounds. `NotchBuddy/project.yml` — XcodeGen project (never edit the `.xcodeproj` by hand).
- `NotchBuddy/Sources/CoucouKit/PillCatalog.swift` — single source of truth for all declared pills (workspace tools, agents, AI providers, services). Every pill ID, color, category and subtitle lives here.
- `docs/SPEC.md`, `docs/INTEGRATIONS.md` — behaviour, views, states, integrations (in French).
- `design/prototype/notch-buddy.html` — original prototype, the visual source of truth. `design/captures/` — target screenshots.
- `windows/` — the Tauri app for Windows and Linux: Rust in `src-tauri/`, TypeScript in `src/`, the `coucou-hook` relay in `hook/`. `windows/README.md` lists what differs from the Mac.
- `docs/*.html` — the GitHub Pages site (privacy, terms, support, legal notice).
- `relay/` — the Coucou relay, a stateless Cloudflare Worker that forwards the iPhone Live Activity pushes (it holds the APNs key, which must never ship in an app). See `relay/README.md`.

## This fork: one repository, several apps
This is Alan's fork. Work from the branch **`unify/monorepo`** (not `main`: `main` mirrors Louis's upstream and the App Store
build). Branch from it and merge back into it; never start from `main` or from an old branch, that is how work got stranded.

- `windows/` desktop (Tauri) · `NotchBuddy/` macOS · `relay/` · `android/` the phone app (SmartIsland-derived, **GPL-3.0**,
  its own `android/LICENSE`) · `jinx/` Jinx's tooling and the server-side sources (`jinx/server-side/`).
- Layout, shared contracts and how to pull Louis's upstream: `docs/MONOREPO.md`. Read it before touching `android/` or `windows/`.
- The PC and the phone share two JSON vectors (`merge-vectors.json`, `presence-vectors.json`). Change both sides, then run
  `bash scripts/check-contracts.sh`.
- Verify before saying "done": desktop `cd windows && npx tsc --noEmit`; hook `cd windows/hook && cargo test`;
  Android `cd android && ./gradlew :app:compileDebugKotlin` (needs `local.properties`); Jinx panel
  `node --test jinx/vga-panel/test_mochi_logic.js` and `python3 -m unittest discover -s jinx/vga-panel/server`.
- Swift (`NotchBuddy/`) can only be compiled on a Mac: say so explicitly when you changed it and could not build it.
- **Where things run: the server first, the PC only as the fallback.** Everything that must stay up for the
  system to work (Jinx's gateway and shared thread, presence, her mood, the Telegram mirror, the pendientes,
  migrations/sync…) runs on the home server; the laptop keeps a copy that clients use only when the server fails,
  and gives back what it did meanwhile when the server returns. New pieces follow the same rule. What is the PC's by
  nature (Claude Code's hooks, the island itself) stays on the PC. See `docs/MONOREPO.md`, "Where things run".
- Do not push to `upstream`. Do not deploy to the home server from a cloud session (no access); the `deploy.sh` scripts are
  for Alan's laptop.
- Secrets never go in the repo. Server-side code under `jinx/server-side/` is a snapshot of what is deployed.

## Build
```
cd NotchBuddy && xcodegen && xcodebuild -scheme NotchBuddy -configuration Debug build
```
Windows and Linux: `cd windows && npm install && npm run tauri dev`

## Rules
- Swift 6, SwiftUI + AppKit. No third-party dependencies unless truly unavoidable. The character is drawn in code (`Canvas` + `TimelineView`), no Rive/Lottie/images.
- Secrets live in the Keychain, never on disk or in git.
- No telemetry. Network calls only to services the user configured.
- Never block Claude Code: if the app doesn't answer, the hook exits immediately.
- Never overwrite `~/.claude/settings.json`: dated backup, merge, show the diff, write only after the user confirms.
- Never send an email or approve a Claude Code or Codex permission without an explicit click.
- Performance: 0 % CPU when the island is hidden.
- Keep the bundle identifier `fr.louisraille.NotchBuddy` (Keychain items, preferences and permissions depend on it).
- Never restyle what already ships (pills, cards, Settings, chat…): existing views stay exactly as they are in `main`, which is the App Store build. Change the look of an existing view only when explicitly asked.
- Pill IDs are stable contract values (Keychain, UserDefaults, hook routing): never rename an existing pill ID.
- New views follow the existing app style. `design/prototype/notch-buddy.html` and `design/captures/` are references for new work, not a reason to change existing views.
- Every release adds its CHANGELOG.md section, a row in the README Versions table, and commits the regenerated Info.plist with the new version.
