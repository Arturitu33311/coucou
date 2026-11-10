# Reproductor de escritorio (estilo iBaila) — diseño

Fecha: 2026-11-09 · Amplía el proyecto 1 (`2026-11-08-reproductor-cristal-design.md`). Boceto aprobado por Alan:
https://claude.ai/artifact/FrUMW9RjHyrgUPuqEEa5A2 (versión 5). Referencia: captura de iBaila que Alan compartió
(reloj grande arriba a la izquierda, "Continue Playing" a la izquierda, cover flow de cristal al centro, búsqueda y barra
de reproducción debajo, luz en los bordes).

## Para qué

Alan quiere el reproductor **en el escritorio**, lo más fiel posible a la referencia de iBaila, cuando el fondo se ve; y el
panel del notch (ya hecho) cuando hay ventanas encima. Mismas funciones en los dos: buscar Spotify/YT Music, recientes,
DJ, artista con cover flow, controles.

## Lo que decidió Alan

- **Ambos**: escritorio cuando se vea; notch cuando no.
- **Fiel a la referencia**; el **medidor** (batería, red) se queda, integrado como píldora de cristal arriba a la derecha.
- El carro del fondo actual sale (la referencia no lo tiene).
- **Reloj**: el que ya tiene el fondo de la PC (el de `linux-wallpaper/wallpaper.html`), no uno nuevo.
- **Liquid glass** marcado en portadas, paneles y barras: reflejo diagonal arriba, borde de luz, desenfoque y saturación
  del fondo detrás, reflejo de las portadas en el piso del cover flow.
- **Lista de la izquierda** como en la referencia: etiqueta "Continuar escuchando"; la canción actual como tarjeta grande
  (portada, título, artista, botón play); debajo filas de cristal sueltas, **giradas hacia el centro (rotateY ≈ 22°) sin
  hundirse**, desvaneciéndose hacia abajo; punto verde = Spotify, rojo = YT Music.

## Disposición (pantalla 1920×1080; escala con la resolución)

Del boceto (versión 5), de arriba a abajo:
- Arriba izquierda: fecha y reloj (el actual).
- Arriba derecha: medidor como píldora de cristal (aro de batería con número, red).
- Izquierda, debajo del reloj: "Continuar escuchando" (tarjeta actual + ~6 recientes).
- Centro: cover flow de cristal (álbumes del artista que suena o del artista abierto; el central más grande, vecinos
  girados hacia adentro, reflejo abajo) y su título (álbum · año) en una píldora de cristal.
- Debajo: barra de búsqueda con selector Spotify / YT Music / Ambas; los resultados se despliegan en cristal sobre el
  cover flow (como el panel del notch).
- Debajo: barra de reproducción (anterior, pausa, siguiente, mini portada, título, avance con tiempo, artista · fuente,
  letra, DJ) y una línea de letra sincronizada.
- Fondo: escena teñida con el color de la portada actual (portada ampliada y difuminada); sin música, los colores de la
  hora como hoy.
- Bordes: luz fina con el color de la portada (solo dentro del fondo; la luz sobre las ventanas es el proyecto 3).

## Cómo se construye (decisiones de Claude)

- **Dos capas**:
  1. **Fondo (Hidamari + `linux-wallpaper/`)**: lo que no se toca — escena teñida, reloj, medidor, luz de bordes, línea de
     letra. Se rehace `wallpaper.html` con este estilo (sin el carro); `server.py` ya da música, portada, letra, batería y red.
  2. **Ventana de escritorio de Coucou** (nueva, de pantalla completa, transparente, **debajo de las ventanas normales y
     encima del fondo**): lo que se toca — "Continuar escuchando", cover flow, búsqueda/resultados, barra de reproducción.
     Solo esas zonas reciben el ratón (región de entrada), el resto deja pasar los clics al escritorio. Reusa los comandos
     `player_*`, `player_art`, tinte y barras de la isla; el estilo de cristal se comparte con el panel del notch.
- **Cuándo se usa cuál**: la ventana de escritorio se muestra mientras el escritorio está a la vista (ninguna ventana
  normal maximizada/cubriendo la pantalla del fondo) y se oculta en cuanto algo la tapa; `Super+Alt+M` y la lupa abren
  el escritorio si está a la vista, si no el panel del notch. Detectar "escritorio a la vista" en X11 (lista de ventanas
  y su geometría por `_NET_CLIENT_LIST_STACKING`) solo cuando cambia el foco o la pila, nunca en bucle.
- **0 % CPU oculto**: la ventana de escritorio no anima nada mientras está tapada; el fondo ya se pausa oculto.

## Decisiones tras la revisión del diseño (de Claude; Alan puede cambiarlas)

1. **Una sola ventana, no dos capas.** El fondo de escritorio completo (escena teñida, reloj actual, medidor, luz de
   bordes, letra **y** las partes que se tocan) lo dibuja **Coucou en una ventana de escritorio** que reemplaza a
   Hidamari en el perfil diario (Alan, 11-09: si queda bien visualmente, en **todos los perfiles**). Así el desenfoque del cristal funciona (todo está en la misma página) y no hay que
   sincronizar dos capas. Hidamari queda para los otros perfiles y `linux-wallpaper/wallpaper.html` sigue funcionando
   solo, como respaldo cuando Coucou no corre (no se rompe ni se bifurca; la nueva página reusa su reloj y su medidor).
   Esto sustituye la "Cómo se construye: dos capas" de arriba.
2. **Datos**: la página de Coucou toma música, portada, tinte, barras y letra de lo que la isla ya tiene (MPRIS,
   `music.rs`); la letra sigue el ajuste existente de Coucou (`musicOnline`), sin abrir nada nuevo a lrclib. Batería y red
   de lo que Coucou ya lee (sysstate) o, si falta, igual que `server.py`.
3. **Clics**: la ventana es de escritorio (tipo DESKTOP, debajo de todo, fuera de Alt+Tab/Actividades); los clics
   fuera de los paneles caen en ella sin efecto, como en un escritorio normal (no hace falta región de entrada).
4. **Pantalla**: el monitor principal. Otros monitores conservan su fondo.
5. **"Escritorio a la vista"** (Alan usa Pop Shell en mosaico): el reproductor de escritorio se usa cuando el **espacio de
   trabajo actual no tiene ventanas visibles** (vacío o todo minimizado); en cualquier otro caso, el notch. Se reevalúa
   con los eventos de la pila de ventanas de X11, nunca en bucle.
6. **Orden**: primero se arregla el fallo del notch (cierre solo / sin teclado con el atajo); el ensayo de la ventana de
   escritorio (orden de capas y foco para escribir) va después y decide si se sigue.
7. **Cover flow sin música**: los álbumes del último artista del historial; sin historial, se oculta y queda solo la
   búsqueda.

## Riesgos (van primero en el plan, como ensayos)

1. **Orden de capas en X11/GNOME**: que la ventana de Coucou quede **encima de la ventana de Hidamari y debajo de todo lo
   demás**, sin aparecer en Alt+Tab ni en la vista de actividades. Ensayo: tipo `_NET_WM_WINDOW_TYPE_DESKTOP` o `BELOW` +
   `skip_taskbar/pager`, comprobar con capturas. Si no se puede, alternativa: que Coucou muestre la página del fondo él
   mismo (reemplazando Hidamari en el perfil diario) y todo viva en una sola ventana de escritorio.
2. **Desenfoque real del fondo** (liquid glass): WebKitGTK no puede desenfocar lo que hay detrás de una ventana
   transparente. Solución: la ventana de escritorio pinta su propio fondo (la misma escena del fondo, sincronizada) y
   desenfoca sobre eso; o, con la alternativa del riesgo 1, todo está en la misma página.
3. **Foco y escritura** en una ventana a nivel escritorio (la búsqueda): mismo problema que el notch (ver pendiente).

## Pendiente heredado del notch

En la prueba en vivo el panel del notch abre con `Super+Alt+M` y busca en YT Music (resultados con portada), pero **a
veces se cierra solo al hacer clic y escribir**, y al abrir con el atajo no siempre recibe el teclado. Investigarlo (es
del proyecto 1) antes o junto con el ensayo 3.

## Metas, no-metas, anti-metas

- **Metas**: el escritorio se parece al boceto/referencia; buscar y poner canciones desde ahí; cambio automático
  escritorio ↔ notch.
- **No-metas**: luz en los bordes sobre las ventanas (proyecto 3); fondo animado/video, color de acento del sistema e
  iconos (proyecto 2); teléfono (proyecto 4); dock (se queda Plank).
- **Anti-metas**: que el escritorio tape o robe clics a las apps; que aparezca en Alt+Tab o en Actividades; CPU con el
  escritorio tapado; tocar las vistas existentes de la isla.

## A criterio de quien construye

Tamaños exactos y escalado a otras resoluciones, animaciones de entrada, cuántos recientes caben, cómo se despliegan los
resultados de búsqueda sobre el cover flow, y el mecanismo exacto de detección de "escritorio a la vista".

## Pendientes tras la integración (2026-11-09, pedido de Alan)

Integrado en `unify/monorepo` funcionando, pero **falta mejorar la optimización y la parte visual**:

- **Optimización**: el primer audio de YT Music tarda ~10 s (casi todo yt-dlp); la página del escritorio decodifica
  portadas aunque esté tapada; la isla y el escritorio piden la letra por separado (dos consultas a lrclib a la vez);
  un fallo de red al buscar los álbumes de un artista queda en caché hasta reiniciar; medir CPU/RAM de la ventana de
  escritorio frente a Hidamari.
- **Visual**: el giro de las tarjetas es una imitación 2D (WebKit corre con `WEBKIT_DISABLE_COMPOSITING_MODE=1`, sin 3D
  real); acercar más el look al boceto/iBaila (liquid glass, reflejos, cover flow); el medidor solo dice En línea /
  Sin conexión (el boceto muestra tipo de red y señal); revisar desplazamiento con rueda/arrastre y escalas en otras
  resoluciones.
- **Sin probar en vivo**: que Hidamari vuelva al cerrarse Coucou; Spotify (falta el inicio de sesión de spotifyd y del
  panel); DJ de Spotify.
- Menores anotados en las revisiones (ledger de SDD): coincidencia de portada solo por título, `desktop_is_visible` sin uso,
  capacidad compartida `default` para la ventana de escritorio, etc.
