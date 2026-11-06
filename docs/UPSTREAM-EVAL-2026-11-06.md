# Upstream (Louis-CFM/coucou 0.6) → nuestra versión Linux/Android

Fecha: 2026-11-06. Evaluado contra `origin/unify/monorepo` (`2b27b32e`), la base consolidada.
(Una primera pasada se hizo por error contra un `unify/monorepo` local 689 commits más viejo; ya corregido.)

Upstream 0.6 sigue por delante, pero la base ya absorbió buena parte: Flow (dictado, Notetaker, «OK Coucou»),
la ventana Coucou con Ajustes por páginas, cola de permisos (`approval-queue.ts`), arreglo del ticker (`stepSeq`),
sesiones de Claude Code en la isla (`claude_sessions.rs`: listar, escribir, `claude --bg`). **No se hace un merge
completo**: se portan funciones sueltas.

## Hecho en esta rama
| Qué | Nota |
|---|---|
| **Hub en 4 grupos** (Today / Focus / System / Files) con sub-pestañas | Antes 12 pestañas en una tira con scroll. ←/→ recorren todo, 1–9 eligen dentro del grupo, cada grupo recuerda su última pestaña. Altura del Hub 232 px. Estilos en `hub-island.css`. |
| **Subencabezados en Ajustes → Jinx** (Jinx & server / Notifications & phone / Data & integrations) y hints compactos | La página mezclaba ~16 filas con textos largos. |
| **Mochi con sueño** tras ≥3 h de trabajo de agentes sin una pausa de 10 min (Mac 0.6 #970) | `mochi/tired.ts` (lógica pura, 7 asserts) + un intervalo de 30 s en `main.ts`; bosteza (emote `sleepy`, 4 s) como mucho cada 25 min. Sin ajuste en Ajustes todavía. **Sin ver en la isla real.** |
| **Handy desinstalado** (a petición): app, modelos (1.4 GB), `coucou-ptt`, `coucou-voice`, autostart | Respaldo de historial y grabaciones en `~/handy-backup-20261106.tar.gz`. Dictado ahora = Flow de Coucou. |

## Pendiente / ya cubierto
| Función | Estado |
|---|---|
| Iniciar sesión de Claude Code desde la isla | **Parcial**: ya hay `claude --bg` (agente en segundo plano). Lo de upstream abre *tu terminal* con el prompt; se puede añadir como segunda opción. |
| Soltar un archivo sobre una sesión | Pendiente, medio. |
| Alertas de cualquier pill / uso del plan en la isla compacta | Pendiente, bajo coste (`rate_limits.json` ya existe). |
| Cola de permisos, ticker >20 pasos | Ya en la base. (Revisar ahí que cada clic quede ligado a la solicitud mostrada y haya una guarda breve tras mostrar una tarjeta nueva: lo apuntó la revisión de seguridad sobre un intento mío que se descartó.) |
| Mochi lee la presión de memoria (#795) | Pendiente, barato (`/proc/meminfo`). |

## Funciones solo de Mac: ¿valen la pena en Linux?
| Función Mac | Veredicto |
|---|---|
| ⌘⇧2 captura completa + miniatura | No: GNOME ya tiene Shift+PrtSc con portapapeles. |
| Coucoudoro | No: Pomodoro del Hub es más completo. |
| Mail pill (Apple Mail) | No portable. |
| «Coucou en una esquina» | No: choca con el offset/dodge y Just Perfection. |
| Chat con tu suscripción (`claude -p`) | No: el chat ya va a Jinx/Hermes. |
| Velocidad de voz, reseña App Store, iPhone | Solo Mac/iOS. |
