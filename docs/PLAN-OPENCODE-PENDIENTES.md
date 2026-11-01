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

## Ya resuelto 2026-11-01
- Un reproductor MPRIS colgado (Sonora) bloqueaba la isla (25 s por propiedad) y dejaba el fondo sin música: ahora
  `music.rs` lee cada reproductor en su hilo con tope de 1,5 s y `server.py` pregunta uno por uno (1 s).
- Música del celular en el fondo: `/state` trae `remote: true` y la página anima un espectro sintético.
- OJO al compilar la app de la laptop (línea ccr): `cargo build --release --features tauri/custom-protocol` (sin eso la
  isla apunta a localhost:1420 y sale "Could not connect to localhost"). En esta sesión el comando `coucou-island` necesita
  `DISPLAY=:0 XAUTHORITY=<la de gnome-shell>`.

## Falta, en orden
1. **Portada y espectro REAL del celular en fondo e isla (propuesta de Alan: el celular, que siempre sabe qué suena,
   manda la info a la PC, y viceversa).** KDE Connect 23.08.5 no publica `mpris:artUrl`. Diseño: nuevo frame del
   canal audiolink (tipo 6, celular→PC, JSON `{title,artist,album,status,positionMs,lengthMs}` + portada JPEG ≤64 KB y
   bandas de espectro si se quiere) en `AudioLinkWire.kt`/`audiolink_wire.rs` con casos nuevos en
   `audiolink-vectors.json` (copiar a ambos lados, `scripts/check-contracts.sh`); el celular lo saca de la sesión de
   medios (`Watchers`, ya excluye espejos). En la PC: publicar un reproductor MPRIS `coucou.phone` (zbus) con `artUrl`
   a un archivo local, así isla (`music.rs`), fondo (`server.py`, ya trata `coucou.phone*` como remoto) y cualquier
   cliente lo ven sin más cambios; ocultar el espejo de kdeconnect para no duplicar. Sentido inverso (PC→celular):
   mismo frame por el canal de la PC con lo que suena en la laptop, para que el fondo del celular reaccione.
   Alternativa barata: KDE Connect más nuevo (flatpak/PPA), comprobar con `playerctl -p kdeconnect.mpris_* metadata`.
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
