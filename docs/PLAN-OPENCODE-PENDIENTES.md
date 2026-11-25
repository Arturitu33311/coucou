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

## Hecho 2026-11-01 (tarde): música bidireccional con portada — CONSTRUIDO, SIN PROBAR EN DISPOSITIVO
- Celular→PC: líneas `{"op":"now",...}` y `{"op":"art",hash,b64}` (portada JPEG ≤64 KB, 320 px) → `phonenow.rs` publica el
  reproductor MPRIS `coucou.phone` con `artUrl` en `~/.cache/coucou/phone-art/<hash>.jpg`. Isla y fondo lo ven solos;
  el espejo de KDE Connect se oculta (`music.rs::without_duplicate_mirror`, `server.py`). Los controles se reenvían al espejo.
  `audiolink_players::is_mirror` incluye `coucou.phone` (evita el bucle "la PC suena → pausa el celular").
- PC→celular: tramas 6 (`PcNow` JSON) y 7 (`PcArt` JPEG ≤60 KB, se achica con ImageMagick `convert`) → notificación
  silenciosa "Sonando en la laptop" con portada (`AudioLinkService.showPcNow`).
- Compatibilidad: el hello de la PC trae `"now":1`; el celular manda `{"op":"caps","now":1}` solo si lo ve. Una PC o un
  celular viejos siguen funcionando como antes.
- El enlace ahora también se abre cuando una app del celular tiene sesión de medios (antes solo con los Bose puestos);
  se cierra a los 7 s sin ninguna de las dos cosas. Costo: la conexión ssh queda abierta mientras haya una sesión.
- Pendiente de la prueba real: instalar con `install-phone.sh`, compilar la laptop con
  `cargo build --release --features tauri/custom-protocol`, reproducir algo en el celular y mirar `playerctl -l`
  (debe salir `coucou.phone`) y que la isla/fondo muestren portada; y al revés, la notificación en el celular.
- No hecho: espectro REAL (Visualizer de Android pide RECORD_AUDIO y en Samsung suele dar ceros; el sintético sigue).
  Controles desde la notificación del celular hacia la PC. Mostrar lo de la PC en la isla del celular en vez de una notificación.

## Falta, en orden
1. **Probar lo de arriba en el dispositivo** (ver "Pendiente de la prueba real"). Alternativa si la sesión de medios
   no trae portada en alguna app: KDE Connect más nuevo, `playerctl -p kdeconnect.mpris_* metadata`.
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
