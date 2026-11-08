# Reproductor de cristal en la isla (Linux) — diseño

Fecha: 2026-11-08 · Proyecto 1 de 4 del plan "portar funciones de iBaila a Coucou".

## Para qué y por qué

Alan escucha música mientras hace otras cosas. Hoy usa **Sonora** (cliente de terceros, Flatpak
`io.github.nolight132.sonora`) para Spotify y, a veces, YT Music, solo porque se ve mejor. Lo que le gusta de
Sonora: **colores de acento de la portada, el visualizador de audio y tener varias plataformas en un solo lugar**.
Quiere poder **buscar una canción en el momento y ponerla** sin dejar lo que está haciendo, desde Coucou.
Cuando esto funcione en su laptop, **Sonora se desinstala**.

Referencia de inspiración: iBaila (ibaila.app → aestheticmac.com/ibaila), su "glass player": cover flow de
cristal con los álbumes del artista, barra de reproductor discreta y búsqueda que pone la canción donde
escuchas.

## Qué hay ya (no reconstruir)

En `windows/` (Tauri, Linux y Windows):
- Vista **"music"** de la isla (`src/views/music.ts`): portada, título, tres líneas de letra, barra de avance,
  controles y visualizador. Color de acento de la portada: `musicTint` (`src/island/music.ts`).
- Visualizador en tiempo real: `src-tauri/src/bars.rs` (cava, 64 bandas, solo mientras la isla se ve y suena algo).
- Lectura de lo que suena por MPRIS: `src-tauri/src/music.rs` (cualquier reproductor) y
  `src-tauri/src/spotify.rs` (Spotify por D-Bus).
- Atajo de teclado: `coucou --toggle` (ver `lib.rs`) y `tauri-plugin-global-shortcut` ya está en Cargo.
- Secretos: llavero del sistema vía `keyring` (`src-tauri/src/secrets.rs`).
- El fondo de Linux (`linux-wallpaper/`) lee MPRIS; seguirá funcionando con lo que ponga el reproductor nuevo.

## Qué se construye

Una vista nueva de la isla, **"player"** (nombre interno; texto visible "Buscar"/"Reproductor"), más grande que
la isla normal, que sale del notch.

### Cómo se abre y se cierra
- **Atajo de teclado propio:** una acción nueva (p. ej. `openPlayer`) en el sistema de atajos de Coucou
  (`src-tauri/src/shortcuts.rs`, con su ajuste y manejo de choques), **activada por defecto** para esta acción.
  Respaldo si `platform::global_shortcuts_blocked()` (GNOME): bandera `coucou --player` y una línea en el
  README/instalador para enlazarla como atajo personalizado de GNOME (decisión de Claude). Abre la isla directo
  en "player" con el cursor en la búsqueda.
- **Desde el notch:** un botón de búsqueda (lupa) en la vista "music" lleva a "player". La pastilla compacta sigue
  haciendo lo que hace hoy (abre "music").
- Se cierra con **Esc**, clic fuera o tras un rato sin uso, como los demás estados de la isla.

### Qué muestra antes de buscar
- Lo que suena (portada, título, artista, controles) con **colores de acento** de la portada y el
  **visualizador** (reusar `bars.rs` y `musicTint`).
- Botón **DJ** (DJ de Spotify; ver riesgos).
- **Recientes**: Spotify y YT Music mezclados, más nuevos primero, sin duplicados.

### Búsqueda
- Un campo de texto. **Selector de fuente**: Spotify / YT Music / ambas (recordar la última elección).
- Resultados: canciones (con portada pequeña, título, artista, fuente) y artistas. Un toque/Enter en una
  canción la pone; flechas para moverse con teclado.
- Spotify devuelve máximo 10 por llamada (ver restricciones): pedir la siguiente página con `offset` al llegar
  al final de la lista.

### Página del artista
- Al tocar un artista: su nombre/foto, sus **álbumes en cover flow** (fila horizontal de portadas de cristal que
  se desliza con rueda/flechas; la del centro más grande) y, al tocar un álbum, sus canciones para ponerlas.
- Spotify: `GET /artists/{id}` + `GET /artists/{id}/albums` + `GET /albums/{id}`. "Top tracks" ya no existe
  para apps nuevas: en su lugar, canciones del artista vía búsqueda (`artist:"<nombre>"`).
- YT Music: la página de artista de YT Music (canciones y álbumes).

### Cómo suena cada fuente
- **Spotify (Premium):** Web API de Spotify con OAuth **PKCE** (sin client secret). Scopes mínimos:
  `user-read-playback-state`, `user-modify-playback-state`, `user-read-currently-playing`,
  `user-read-recently-played`. Se reproduce con `PUT /me/player/play?device_id=…` en uno de dos dispositivos
  (decisión de Alan: **B principal, A de respaldo**):
  - **B — Spotify sin ventana (principal):** un dispositivo Spotify Connect local sin interfaz (spotifyd o
    librespot), como servicio de usuario que Coucou arranca solo cuando hace falta, con nombre fijo (p. ej.
    "Coucou") para encontrarlo en `GET /me/player/devices`. Debe publicarse por MPRIS (o la isla debe seguirlo
    por la Web API) para que pastilla, letra, visualizador y fondo funcionen igual. Su inicio de sesión va por
    OAuth y sus credenciales al llavero/caché propia, nunca al repo.
  - **A — app oficial minimizada (respaldo):** si B no arranca, no aparece en dispositivos o falla al reproducir,
    usar la app oficial `com.spotify.Client`. Si está cerrada, lanzarla y **minimizar su ventana en cuanto
    aparezca** (desde el gestor de ventanas; se acepta un parpadeo breve solo en el arranque en frío), esperar
    a que salga en dispositivos y reproducir. Avisar en el panel, discreto, que se usó el respaldo.
- **YT Music:** búsqueda y páginas de artista sin iniciar sesión. Audio con **mpv** (`--no-video`) + **yt-dlp**,
  una sola instancia de mpv controlada por su socket IPC. mpv debe publicarse por MPRIS (complemento
  `mpv-mpris`) para que la isla, la pastilla, la letra, el visualizador y el fondo lo vean como cualquier
  reproductor.
- **Una fuente a la vez:** al poner algo de una fuente, pausar la otra.
- Los controles (pausa, siguiente, anterior) siguen yendo por MPRIS como hoy.

### Recientes
- Coucou guarda su **propio historial** de lo que puso desde el reproductor (fuente, id, título, artista,
  portada, fecha), últimos ~50, en el directorio de datos de Coucou. Así YT Music no necesita cuenta.
- Para Spotify se suma `GET /me/player/recently-played` (lo puesto desde la app de Spotify u otros dispositivos).

## Metas, no-metas, anti-metas

**Metas:** buscar y poner de Spotify y YT Music desde la isla en pocos segundos, con teclado; verse a la altura
de Sonora (acento + visualizador); permitir desinstalar Sonora.

**No-metas (este proyecto):** playlists y canciones guardadas, biblioteca, música local, luz en los bordes
(proyecto 3), fondo animado/video e icono en Plank (proyecto 2), teléfono (proyecto 4), cola editable.

**Anti-metas (fracaso aunque "funcione"):**
- Romper o cambiar el aspecto de la vista "music", la pastilla o cualquier vista existente (regla del repo).
  Lo único que se agrega a "music" es el botón de búsqueda.
- CPU cuando la isla está oculta (regla: 0 %). Nada de sondeo en bucle; mpv y cava solo cuando hacen falta.
- Tokens o claves en disco o en git: van al llavero.
- Que una ventana de Spotify aparezca al poner una canción (solo se tolera un parpadeo en el arranque en frío del respaldo A).

## Restricciones

- Reglas de `CLAUDE.md` del repo (sin telemetría; red solo a servicios que el usuario configuró; vistas nuevas
  con el estilo existente; verificar con `cd windows && npx tsc --noEmit` y los `cargo test`).
- Spotify Development Mode (desde feb-2026): el dueño de la app necesita Premium (Alan lo tiene), máx. 5 usuarios,
  búsqueda con `limit` máx. 10, sin "artist top tracks", sin `GET /artists` ni `GET /albums` por lotes (una id
  por llamada), objetos sin `popularity`/`followers`. Redirect URI de loopback por IP (`http://127.0.0.1:<puerto>/…`),
  no `localhost`.
- Linux primero. El código es compartido con Windows: no romper su compilación; si YT Music/mpv queda solo para
  Linux, la vista debe ocultar esa fuente en Windows.

## Preparación en la laptop (una vez)

- **App de Spotify en developer.spotify.com**: la crea Claude en el navegador de Alan con su cuenta (Alan lo pidió).
  Datos: nombre "Coucou", API "Web API", redirect URI **`http://127.0.0.1:43821/callback`** (decisión de Claude; el código debe usar exactamente esta). El Client
  ID va a la config de Coucou (no es secreto con PKCE); los tokens, al llavero.
- Instalar el Spotify sin ventana (spotifyd o librespot) e iniciar su sesión una vez.
- Actualizar **yt-dlp** (el instalado es 2024.04.09, demasiado viejo para YouTube) a una versión que se pueda
  actualizar sola (binario en `~/.local/bin` o pipx), instalar **mpv-mpris** y lo que use la capa de YT Music.
- Cuando Alan confirme que todo funciona: `flatpak uninstall --user io.github.nolight132.sonora` (antes, copia de
  `~/.var/app/io.github.nolight132.sonora/` por si quiere volver).

## Riesgos

1. **DJ de Spotify:** no hay confirmación de que la Web API pueda arrancarlo. Primer paso del plan: un ensayo
   que intente ponerlo como contexto. Si no se puede, el botón DJ abre el DJ en la app de Spotify (por URI) sin
   traer la ventana al frente si es posible; si tampoco, se quita el botón y se le dice a Alan.
2. **YT Music sin API oficial:** la capa (ytmusicapi u otra) y yt-dlp se rompen cuando YouTube cambia; yt-dlp debe
   poder actualizarse fácil y los errores deben verse claros en el panel ("YT Music no responde"), sin colgar la isla.
3. **Cuota de Spotify** en Development Mode: 429 con `QUOTA_EXCEEDED`; búsqueda con retardo al teclear (debounce)
   y caché corta.

## Lo que queda a criterio de quien construye

- Si la capa de YT Music va en Rust directo o como ayudante Python **local** (subproceso en la laptop; nada de
  esto corre en el servidor, a diferencia de `activities.py` — decisión de Claude).
- Tamaño exacto del panel, animación de apertura desde el notch y diseño fino del cover flow, dentro del estilo
  existente de la isla (`design/prototype/notch-buddy.html` como referencia).
- Formato del historial, tiempos de cierre automático y de debounce.
- Combinación de atajo por defecto (que no choque con los de GNOME/Pop Shell que ya usa Alan).
