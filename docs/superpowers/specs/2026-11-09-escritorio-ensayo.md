# Ensayo: ventana de escritorio de Coucou en GNOME X11 (Tarea 1)

Fecha: 2026-11-09 · Laptop de Alan (Zorin, GNOME 46 X11, Hidamari activo). Ventana de prueba: GTK 3 en Python
(misma pila que Tauri en Linux), 1920×1080, type hint `DESKTOP`, `skip_taskbar`, `skip_pager`, visual RGBA, fondo
`rgba(10,10,30,0.35)`, un `GtkEntry`.

## Resultados

| Pregunta | Resultado |
|---|---|
| ¿Encima de Hidamari al mapearse? | **No**: GNOME la pone debajo de la ventana de Hidamari (ambas en la capa de escritorio). |
| ¿Encima de Hidamari tras `xdotool windowraise <xid>`? | **Sí**: `_NET_CLIENT_LIST_STACKING` = Hidamari, prueba, … y se ve en pantalla. |
| ¿Transparencia? | **Sí**: el fondo de Hidamari se ve a través de las zonas translúcidas. |
| ¿Debajo de ventanas normales? | **Sí**: con una ventana normal abierta (zenity), la prueba queda debajo en la pila. |
| ¿Teclado? | **Sí** tras `xdotool windowactivate <xid>` (el mismo arreglo que ya usa la isla): el `GtkEntry` recibió "hola". |
| ¿Estado? | GNOME añade solo `STICKY` (todos los espacios de trabajo) y respeta `SKIP_TASKBAR`/`SKIP_PAGER`. |

## Consecuencias para el plan

- La ventana `desktop-player` usa type hint `DESKTOP` y, **después de mapearse, se sube** sobre la ventana de Hidamari
  (`windowraise` o `XRaiseWindow` vía x11rb). Debe volver a subirse si Hidamari reaparece (vigía de la Tarea 3: al ver
  una ventana de escritorio nueva por encima, `raise`).
- Teclado: `windowactivate` al abrir la búsqueda (ya en `platform/linux.rs::set_activating`).
- Sigue en pie apagar Hidamari en el perfil diario (Tarea 7) para no gastar dos fondos animados a la vez; con esta
  forma también funcionaría encima de Hidamari si se dejara.
- No verificado aquí: que no salga en Alt+Tab ni en Actividades (GNOME no lista ventanas `DESKTOP`); comprobar en la Tarea 2.
