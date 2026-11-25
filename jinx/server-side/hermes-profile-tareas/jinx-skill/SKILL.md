---
name: tareas-agente
description: Use cuando Alan pida que se haga una tarea escolar de Teams (un trabajo, cuadro, presentación, documento). Jinx NO redacta el trabajo: se lo pide al agente `tareas` con el script.
---

# Pedirle una tarea al agente `tareas`

El agente `tareas` es un perfil de Hermes aparte, sin personalidad, con skills de documentos y búsqueda de fuentes, que corre
aislado (bwrap). Jinx decide y avisa; el agente hace el trabajo. No escribas tú el documento.

## Cómo
1. Encuentra la tarea con `tareas_revisar` / `tarea_detalle` y toma su `assignment_id`.
2. Corre con la terminal (tarda 2 a 4 minutos; usa un timeout de 600 s o ponlo en segundo plano):
   `python3 /home/alan/.hermes/scripts/borrador_tarea.py <assignment_id>`
3. La última línea dice qué pasó:
   - `ok:<ruta> (...)` — el agente hizo el trabajo y lo dejó en la tarea de Teams como adjunto SIN entregar; Alan ya recibió el aviso por Telegram.
   - `ya` — ya existe uno para esa tarea (no lo repitas; `--force` solo si Alan pide rehacerlo, y sube otro adjunto).
   - `repetida:<ruta>` — otra materia pide lo mismo: se reutilizó el mismo trabajo, no se hizo dos veces.
   - `no_aplica` — no se puede avanzar desde texto o archivos (webinars, asistencia, exámenes).
   - `error:...` — dilo tal cual y no inventes el resultado.

## Reglas
- El detector rápido (cada 15 min) ya lanza esto solo para cada tarea nueva: antes de pedirlo, revisa que no exista (`ya`).
- Nunca uses `tarea_entregar` sobre lo que hace el agente: Alan lo revisa y decide.
- No registres asistencia ni llenes formularios de asistencia de webinars; si hay un webinar, solo avisa a Alan con la liga.
- Los datos de portada de Alan (nombre, matrícula, grupo) ya están en la memoria del agente: no se los pidas a Alan.
