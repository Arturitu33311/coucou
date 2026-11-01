# Pendientes PC↔celular — plan corto para un agente de opencode

Rama de trabajo: `unify/monorepo` (leer `CLAUDE.md` y `docs/MONOREPO.md`). Nunca `main`, nunca push a `upstream`.
Instalar en el celular SOLO con `android/scripts/install-phone.sh R3CT50BMXYR` (nunca `adb install` ni `adb uninstall`).
No matar `gnome-shell`. No tocar `~/.claude/settings.json`.

## Estado (2026-11-01)
- Relay de audio (laptop → Bose en el S22): funciona, documentado en `docs/AUDIO-LINK.md`.
- Fondo interactivo (`linux-wallpaper/`): instalado, perfiles daily y work usan la web `http://127.0.0.1:8765/`.
  `?noclock` y el manejo del perfil work ya están en el repo. Fondo de ventanas (GNOME `picture-uri`) = captura sin reloj
  en `~/.var/app/io.github.jeffshee.Hidamari/config/hidamari/static-947648.png` (respaldo `.bak-coucou-20261101`).
- Música del celular en la PC: el espejo de KDE Connect vuelve a aparecer porque el S22 se agregó por IP Tailscale
  (`customDevices=100.118.153.16` en `~/.config/kdeconnect/config`). Título/artista/álbum/letra funcionan.

## Falta, en orden
1. **Portada del celular en fondo e isla.** KDE Connect 23.08.5 (Ubuntu) no publica `mpris:artUrl` en el espejo.
   Opciones: (a) instalar un KDE Connect más nuevo (flatpak/PPA) y comprobar con
   `playerctl -p kdeconnect.mpris_* metadata`; (b) que la app Coucou del celular mande la portada por el canal ssh/relay
   y la PC la guarde en un archivo para `server.py` (`/art`) y `music.rs`. Empezar por (a), es lo barato.
2. **Persistencia del fondo estático.** Comprobar tras un login que Hidamari no regenera `static-947648.png`
   (si lo hace, mover la captura a `apply-profile.sh`). Regenerar con
   `chromium --headless=new --no-sandbox --user-data-dir=$HOME/coucou-shot/p --screenshot=... "http://127.0.0.1:8765/?still=1&noclock=1"`
   (chromium es snap: no puede escribir en carpetas ocultas).
3. **Robustez del enlace KDE Connect.** El celular cambia de red: `customDevices` solo cubre la IP Tailscale.
   Comprobar que la LAN sigue descubriéndose al volver a casa.
4. **Relay:** WifiLock/WakeLock + prueba de 10 min con pantalla apagada; indicador en la laptop de que el relay está
   activo y acción "recuperar bocinas"; lista de ignorados para reproductores espejo; medir el delay.
5. **Integración:** `git pull --ff-only` en el checkout principal; `ccr-b3617fcf-vd9kt3` aún no está en `unify/monorepo`.
6. **Portapapeles:** plan en `docs/superpowers/plans/2026-10-31-clipboard-sync.md` (rama `feat/clipboard-sync`);
   4 preguntas abiertas para Alan, no empezar sin sus respuestas.

## Verificar antes de decir "listo"
`cd windows && npx tsc --noEmit`; `cd windows/hook && cargo test`; `cd android && ./gradlew :app:compileDebugKotlin`;
`bash scripts/check-contracts.sh`.
