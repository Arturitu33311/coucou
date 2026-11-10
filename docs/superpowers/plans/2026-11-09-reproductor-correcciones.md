# Reproductor: crítica y plan de correcciones (visual + optimización)

Fecha: 2026-11-09 · Base: `origin/unify/monorepo` 8b1fb2bc · Evidencia: captura real del escritorio (espacio vacío,
1920×1080, Spotify sonando "Honeypie"), medición en vivo de CPU/RAM por hilo, boceto aprobado v5
(claude.ai/artifact/FrUMW9RjHyrgUPuqEEa5A2) y el código de `windows/src/desktop-player/`.
**Este plan se ajusta con lo que Alan notó en el uso antes de implementar.**

## 1. Lo medido (optimización)

| Qué | Medida | Causa |
|---|---|---|
| Proceso principal `coucou` | **~150 % de CPU sostenido** (48 min de CPU en 45 min) | ver abajo |
| 3 hilos sin nombre | ~25 % cada uno, casi sin esperas voluntarias | pool de onnxruntime de Parakeet (`speech::threads()` = 4) |
| `coucou-voice` | ~21 % | "OK Coucou": VAD + **Parakeet transcribe cada tramo con sonido** |
| 6 hilos más | ~6 % cada uno | por identificar (probable otro pool ORT/Skia) |
| `coucou-audio` (cava) | ~7 % | visualizador en tiempo real |
| RAM | 1,2 GB proceso principal + ~1 GB en 5 WebKitWebProcess | Parakeet (650 MB) residente, 5 vistas web |
| Página del escritorio tapada | 0 % | bien: se pausa con `document.hidden` |

Hipótesis principal: **con música por los altavoces, el detector de voz se dispara todo el tiempo y Parakeet
transcribe música sin parar**. No es código del reproductor, pero es el reproductor el que lo provoca. Verificar
pausando la música 30 s y midiendo de nuevo (`thr.sh`).

## 2. Crítica visual (escritorio)

Contra el boceto v5 y la referencia iBaila. Registro: superficie ambiental tipo fondo de pantalla; el cristal está
**aprobado por Alan** (no aplica la regla general contra glassmorphism), pero debe verse como cristal, no como gris.

**Lo que funciona**
- La composición general es la del boceto: reloj arriba izq., "Continuar escuchando", cover flow, búsqueda, barra, letra.
- La barra de reproducción es clara (controles 44 px, tiempo, avance con el color de la portada, fuente).
- Tinte por portada y luz de bordes dan unidad de color; la página se apaga tapada.

**Problemas por prioridad**

- **P1 · El cristal no se lee como cristal.** Paneles gris-marrón planos, sin el reflejo diagonal ni el borde de luz
  fuerte del boceto (`.lg` del boceto: gradiente 0.20→0.03 blanco, `inset 0 1px 0 rgba(255,255,255,.55)`,
  radial arriba-izq.). Con el fondo apagado, todo se funde en un barro marrón: poco contraste figura/fondo.
  *Arreglo*: portar el `.lg` del boceto tal cual (sin backdrop-filter donde WebKit sin compositing lo haga caro: el
  fondo ya es una imagen difuminada, se puede pintar un "cristal falso" con la misma portada difuminada recortada).
- **P1 · Reloj.** Pide Oswald, que **no está instalada ni empaquetada** → cae en DejaVu Sans Condensed; además el
  relieve de capas se ve "doble" (contornos fantasma). Alan pidió el reloj del fondo actual: empaquetar Oswald
  (woff2 local, OFL) y revisar el trazo; si aun así pesa, ofrecer el reloj de cristal del boceto (Inter Tight).
- **P1 · Cover flow vacío y cojo.** Con artistas de pocos álbumes sale 1 portada + 1 vecina a la izquierda, nada a la
  derecha: composición desequilibrada y un hueco enorme en el tercio derecho. *Arreglo*: rellenar con sencillos/EP y
  "relacionados", centrar siempre el actual con vecinos a ambos lados; con ≤2 elementos, sin giro (fila plana).
- **P2 · Giro 2D = inclinación.** `scaleX + skewY` deforma el texto (se lee torcido) y no parece perspectiva.
  Probar perspectiva real solo en esas tarjetas (canvas o CSS 3D si se puede activar compositing solo en esta
  ventana); si no, giro mucho más suave (≤10°) para que el texto no se tuerza.
- **P2 · Recientes duplicados y vacíos.** La canción actual aparece como tarjeta grande **y** como primera fila;
  "DJ · Spotify" con portada gris vacía. Quitar la actual de la lista; icono propio para DJ.
- **P2 · Búsqueda con texto viejo.** Queda "olivia" escrito sin resultados a la vista; la x es un icono de
  retroceso. Vaciar al cerrar resultados / al cambiar de canción, icono "x" estándar.
- **P2 · Selector Spotify/YT/Ambas.** "Ambas" activa en verde Spotify confunde el significado del color; color
  neutro (blanco) para el activo, verde/rojo solo como punto de fuente.
- **P3 · Punto de fuente solo por color** (verde/rojo, 8 px): añadir forma o letra en la descripción; daltonismo.
- **P3 · Medidor** dice "En línea" (boceto: tipo de red + señal "LAN · ●●●●").
- **P3 · Reflejo de la mini portada** bajo la barra de reproducción (artefacto) y el título del álbum tapa el reflejo
  del cover flow.
- **P3 · Duplicado con el notch.** Con el escritorio a la vista, la isla sigue mostrando título y letra recortados
  arriba: redundante. Con el escritorio visible, la isla en modo mínimo.

**Notch**: sin captura (el atajo no abrió el panel en 2 s durante la prueba; puede ser el foco de la terminal).
Pendiente de lo que Alan haya notado.

## 3. Lo que notó Alan en el uso (2026-11-09) — manda sobre el orden de abajo

1. **Al terminar una canción se queda en silencio.** Causa: Spotify recibía `uris:[una pista]` y mpv `loadfile …
   replace` de un solo video; nada en cola. **Arreglado** (rama `worktree-player-visual-optim`): una pista de Spotify
   se reproduce dentro de su álbum (`context_uri` + `offset`, luego el autoplay de la app); en YT Music, al empezar,
   se encola en segundo plano el resto del álbum o la radio de YouTube Music (≤25), cada una entra al historial al
   sonar (portada y fuente correctas), y una reproducción nueva descarta la cola vieja.
2. **Tarda bastante en iniciar** (YT Music ~10 s, casi todo yt-dlp). → Fase A, paso 5: resolver la URL de audio antes.
3. **A veces la portada y el fondo no coinciden.** Portada de mpv buscada por título en el historial; el fondo se
   tiñe de otra fuente. → unificar: el fondo se tiñe de la misma portada que muestra la tarjeta.
4. **La columna izquierda gira al revés**: apunta hacia afuera, debe apuntar al centro. → invertir el signo de
   `flatTurn` (y revisar el lado que se acerca).
5. **La letra casi no se ve y le falta personalidad.** → más tamaño/contraste, línea actual destacada con la del
   antes/después tenue, entrada animada (subida + desenfoque), color del acento de la portada.
6. **Los videos de las canciones no se muestran** (Canvas de Spotify / video de YT). → investigar: Canvas no está en la
   API pública de Spotify; YT sí tiene video (mpv `--no-video` hoy). Decidir fuente.
7. **Al minimizar una ventana para ver el fondo se traba y se ve el fondo viejo del carro** (Hidamari /
   `linux-wallpaper`) antes del reproductor. → mostrar la ventana de escritorio antes (precargada y pintada), no apagar
   y prender Hidamari en cada cambio; medir.
8. **Faltan animaciones**: tarjetas que se levantan al pasar el mouse; un **vinilo que sale de la portada** con hover.
9. **El DJ no funciona.** Coucou no tiene sesión de Spotify (`Spotify login needed` en coucou.err) y, aun con ella, el
   DJ no se puede iniciar por la API pública (es de la app móvil). **Quitado** (8cb91523): ni en el notch, ni botón en
   la barra, ni en recientes.
10. **Las luces del borde no reaccionan a la canción.** Nunca se conectaron: `.dp-edge` es un `box-shadow` fijo que
    solo cambia de color. Para que reaccionen: mandar `music-bars` (cava, hoy solo a la isla) también a la ventana del
    escritorio y modular la intensidad con graves/volumen. Ojo con el costo: WebKit corre sin compositing, así que
    repintar un resplandor de pantalla completa a 30 fps es caro → franjas delgadas por borde, ~15 fps, con medición
    de CPU antes/después.

## 4. Plan (orden propuesto)

Fase A · Optimización (impacto mayor, poco visible)
1. Confirmar la hipótesis de la voz (medir con/sin música).
2. "OK Coucou" no transcribe mientras suena música por los altavoces, o lo hace con un filtro previo barato
   (energía de voz/VAD más estricto, tope de tramos por minuto); Parakeet con 2 hilos y sin espera activa
   (`allow_spinning=0` si sherpa lo expone); descargar el modelo tras N min sin uso.
3. Identificar el pool de 6 hilos; revisar cava (pausar cuando nadie muestra barras).
4. Letra: una sola consulta compartida entre isla y escritorio; caché de álbumes sin guardar errores de red.
5. YT Music: arranque de ~10 s → precargar la URL de audio (yt-dlp) al pasar por encima/al buscar, `--no-playlist`,
   reusar mpv ya abierto.

Fase B · Visual del escritorio
6. Cristal del boceto (`.lg`, `.cover` con reflejo) y más contraste del fondo.
7. Reloj: Oswald empaquetada + trazo limpio (o reloj de cristal si Alan prefiere).
8. Cover flow equilibrado (vecinos a ambos lados, relleno, caso de 1–2 elementos).
9. Recientes sin duplicado, DJ con icono, giro legible.
10. Búsqueda, selector neutro, medidor con tipo de red, artefactos de reflejo, isla mínima con escritorio visible.

Fase C · Verificación
11. Capturas antes/después en el espacio vacío; CPU/RAM antes/después (objetivo: <10 % CPU en reposo con música).
12. Probar en vivo con Alan; luego integrar a `unify/monorepo`.

## 5. Análisis del video de referencia (aestheticmac.com/media/clips/hero, 13,8 s, 1600×900)

Cuadro a cuadro (2 fps) más acercamientos a resolución completa:

- **Dos modos** que se alternan con un botón redondo (icono ↘↖) sobre la barra:
  - *Explorar* (0–4 s): cover flow de los álbumes del artista al centro (5 portadas de cristal, la central más
    grande, cada una con su pie "Drip Harder · 2018 · Album" en cristal esmerilado), barra ancha "Search Apple Music"
    y la barra de reproducción. El video de fondo va atenuado.
  - *Inmersivo* (5 s en adelante): desaparecen el cover flow y la búsqueda (se desvanecen y encogen); el video queda
    a plena luz y la letra ocupa el centro.
- **Fondo = video musical de la canción**, nítido y a pantalla completa. Al cambiar de canción (9 s) hace fundido al
  video nuevo, y la letra arranca con el título y el artista ("25k jacket / Gunna").
- **Letra cinética al centro**: serif negrita con itálicas, 2–3 líneas apiladas y centradas, palabra por palabra
  (karaoke). Una palabra de acento sale en amarillo o naranja ("mornin", "pushin'"), y las palabras entran y salen
  con fundido y desenfoque.
- **Columna izquierda = la cola/contexto, no el historial**. Encabezado "Continue Playing ⌄" con miniportadas
  apiladas, subtítulo del contexto ("Songs like LIKE ME…") y botón ⟳. La actual va en tarjeta de cristal clara con
  ecualizador animado; las demás son miniatura + texto sin caja, giradas hacia el centro, desvaneciéndose. La lista
  avanza con la cola (12–13 s).
- **Reloj**: condensado y muy alto, de trazo fino, gris translúcido metálico; fecha pequeña arriba ("Fri Sep 25").
  La lista se monta sobre la parte baja del reloj.
- **Barra**: compacta y oscura, abajo al centro sobre el dock. Lleva aleatorio, anterior, pausa, siguiente,
  repetir, miniportada, título, "Artista — Álbum", una línea de avance finísima, ⋯, letra (activa), cola y volumen.
  Encima, a la derecha, tres botones redondos de cristal: buscar, (visualizador/modo) y alternar modo.
- **Borde**: resplandor neón grueso amarillo-verde→cian que late con la música (cambia de intensidad cuadro a
  cuadro), con chispas pequeñas cerca del borde.
- Sin medidor de batería ni red.

### Propuesta (orden)
1. Dos modos (Explorar / Inmersivo) con botón de alternar; por defecto inmersivo cuando suena algo.
2. Letra cinética al centro (Fraunces u otra serif OFL empaquetada; palabra por palabra repartiendo el tiempo de
   la línea de lrclib; acento con el color de la portada).
3. Fondo nítido: la portada a pantalla completa ya; luego el video (ver 6).
4. Columna = cola/contexto con ecualizador en la actual; filas sin caja.
5. Barra compacta + 3 botones redondos; búsqueda detrás de la lupa; reloj delgado.
6. Video de fondo: mpv aparte (sin sonido, 720p, decodificación por hardware) en una ventana de escritorio
   debajo de la de Coucou, que pasa a ser transparente. Para Spotify se busca el video en YouTube por
   "artista título". WebKit sin compositing no puede reproducir video barato, por eso no se hace dentro de la página.
7. Borde neón que late con los graves (franjas delgadas, ~15 fps, medido).

## 6. Pendientes al cerrar el 2026-11-09 (todo lo que falta, en orden)

Estado: rama `fix/player-autoadvance` (13c974f0), instalada en la laptop (respaldo `coucou.prev-20261109c`).

### Video de fondo (lo que notó Alan en vivo)
1. **Al cambiar de canción el video anterior se queda congelado** hasta que carga el nuevo (yt-dlp tarda de ~10 a
   ~48 s). Arreglo: al cambiar de canción, quitar `has-video` en el acto (vuelve la portada) y `stop` en el mpv de
   video. Resolver la URL antes: en paralelo con la búsqueda del id, con caché de URLs resueltas y con la siguiente
   canción de la cola precargada.
2. **El video va desfasado de la canción.** Se pide con la posición del momento de la petición, pero empieza
   segundos después, y el video oficial o lyric video no siempre coincide con la versión de audio. Arreglo: al
   empezar, `seek` a la posición actual de la canción (MPRIS); corregir la deriva cuando pase de ~1,5 s
   (comparar `time-pos` contra la posición extrapolada) y al hacer seek o pausa. Preferir el video cuya duración
   se acerque a la de la canción.
3. **La letra no se ve sobre el video.** La línea bajo la barra es pequeña y no contrasta. → letra cinética al
   centro (§5, propuesta 2), con sombra o velo detrás.
4. Recortar las franjas negras que traen algunos videos (lyric videos en cinemascope); mpv `vf=crop` a partir de
   la proporción detectada.
5. Hacer el mpv compatible con versiones >0.37: quitar `--background=#000000` (innecesario con panscan).

### Rediseño hacia la referencia (§5)
6. Modos Explorar / Inmersivo con botón; el cover flow hoy tapa el video.
7. Letra cinética palabra por palabra (serif OFL empaquetada, acento con el color de la portada).
8. Columna = cola/contexto con ecualizador en la actual; filas sin caja.
9. Barra compacta + 3 botones redondos; búsqueda detrás de la lupa; reloj delgado (Oswald no está instalada).
10. Borde neón que late con los graves (franjas delgadas, ~15 fps, medido).
11. Animaciones: tarjetas que se levantan con hover y vinilo que sale de la portada.

### Rendimiento
12. **Scroll de la columna trabado**: 7 s de CPU por 3,5 s de scroll; en reposo 0,5 s. Descartado: el desenfoque
    de las tarjetas y el blur del fondo (ya quitados de todos modos). Sospecha principal:
    `WEBKIT_DISABLE_COMPOSITING_MODE=1` en `~/.local/bin/coucou-island`; probar con compositing encendido (medir
    CPU, ver si hay parpadeo o fallos de transparencia; vuelta atrás fácil). El banco con MiniBrowser
    (`tmp/bench`) no medía el scroll: arreglarlo antes de usarlo.
13. "OK Coucou" transcribe música con Parakeet (~1 núcleo con música): no transcribir mientras suena música por
    los altavoces, Parakeet a 2 hilos, sin espera activa.
14. Arranque lento de YT Music (~10 s, yt-dlp): resolver antes y reusar.
15. Letra: una sola consulta compartida entre isla y escritorio; caché de álbumes sin guardar errores de red.

### Sin verificar en vivo
16. Autoavance: una canción de YT Music y una de Spotify desde el buscador de Coucou; adelantar hasta casi el
    final (`playerctl -p mpv position …`) y ver que sigue la siguiente con su portada. Spotify ahora debería sonar
    por spotifyd ("Coucou"), no por la app.
17. Relay que pausaba la PC: arreglo de Android (sesiones remotas / Spotify Connect) **sin instalar en el S22**
    (`install-phone.sh`).
18. Portada y fondo que a veces no coinciden: revisar con el autoavance (cada canción encolada entra al historial
    al sonar).

### Otros
19. Medidor: tipo de red y señal (hoy solo "En línea").
20. Integrar a `unify/monorepo` cuando 1–3 y 16 estén verificados (regla de `~/estado`: lo estable va a la
    principal).
21. Menores: Node 22 como predeterminado para las pruebas (`nvm alias default 22`), actualizar la skill
    Impeccable, Tauri 2.12.5, prueba rota previa `PendingPcmTest.kt` en Android, 2 pruebas i18n previas
    fallando (hub y ajustes).
