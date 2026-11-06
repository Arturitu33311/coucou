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
| Iniciar sesión de Claude Code desde la isla | **Hecho**: en el chat «+ New» hay un backend «Claude Code (terminal)» que abre tu terminal (gnome-terminal, konsole, kitty…) en la carpeta con el prompt, sin ponerlo en la línea de comandos. Probado: pruebas unitarias de los argumentos; **no** abrí una terminal real. |
| Soltar un archivo sobre una sesión | Pendiente (también lo trae AgentBar), medio. |
| Alertas de cualquier pill / uso del plan en la isla compacta | Pendiente, bajo coste (`rate_limits.json` ya existe). |
| Cola de permisos, ticker >20 pasos | Ya en la base. (Revisar ahí que cada clic quede ligado a la solicitud mostrada y haya una guarda breve tras mostrar una tarjeta nueva: lo apuntó la revisión de seguridad sobre un intento mío que se descartó.) |
| Mochi lee la presión de memoria (#795) | Descartado por ahora: poco valor y exigiría sondear `/proc` desde la isla. |

## Funciones solo de Mac: ¿valen la pena en Linux?
| Función Mac | Veredicto |
|---|---|
| ⌘⇧2 captura completa + miniatura | No: GNOME ya tiene Shift+PrtSc con portapapeles. |
| Coucoudoro | No: Pomodoro del Hub es más completo. |
| Mail pill (Apple Mail) | No portable. |
| «Coucou en una esquina» | No: choca con el offset/dodge y Just Perfection. |
| Chat con tu suscripción (`claude -p`) | No: el chat ya va a Jinx/Hermes. |
| Velocidad de voz, reseña App Store, iPhone | Solo Mac/iOS. |

## Proyectos similares (investigado 2026-11-06)
Ninguno es multiplataforma como Coucou: casi todos son solo macOS. Lo útil son las ideas.

| Proyecto | Licencia | Qué tiene que nos sirve |
|---|---|---|
| [AgentBar](https://github.com/michalstrnadel/AgentBar) | MIT | **Denegar con nota** (hecho aquí), aprobar «todas las iguales» de una vez, etiquetas de riesgo del comando (borra, tubería a shell, red), reglas propias en JSON que primero solo observan, historial de decisiones y resumen semanal, mantener el Mac despierto mientras trabajan agentes (hecho aquí), soltar un archivo sobre una sesión, aprobaciones por ntfy en el móvil, **CLI para Linux + módulo waybar** (`agentbar report`). Principio: nada se responde a tus espaldas, y si falla cae a la terminal. |
| [Notch Island](https://github.com/MANISH007700/notch-island) | MIT | «Always allow» en la tarjeta, saltar al panel exacto de WezTerm/tmux, isla redimensionable, un sonido por evento, recuperar sesiones ya abiertas al arrancar, hooks que no bloquean salvo los permisos. |
| [LiveIsland](https://dev.to/aissam_af7e3548e20435d720/i-turned-my-macbooks-notch-into-a-dynamic-island-that-keeps-claude-code-and-codex-working-past-15p7) | MIT | Cambio automático de cuenta (claude-swap/ccswap) al acercarse al límite semanal; consola transparente que muestra cada comando que ejecuta. Solo útil si usas varias cuentas. |
| [NexNotch](https://github.com/NexVar/NexNotch) | GPL-3 | Extensión GNOME 49/50 casi igual a nuestro Hub (sistema, calendario, tareas, clima, pomodoro, notas, shelf, quick, notificaciones). Ideas: ocultar el banner nativo de las notificaciones, presets de color (Nord, Dracula…), expandir al arrastrar en Wayland, <0.1 % CPU en reposo, timers alineados al segundo. |
| [Vibe Notch / Claude Island](https://github.com/farouqaldori/vibe-notch), [Notch Talk](https://github.com/jan-hga/notch-talk) | Apache-2.0 | Base del género (hooks → socket Unix). Notch Talk añade enviar mensajes a la sesión. Vibe Notch manda analíticas a Mixpanel: no copiar. |
| [NotchPilot](https://notchpilot.husainjhalod.me/), [MioIsland](https://github.com/MioMioOS/MioIsland) (CC BY-NC, no usar código), CodeIsland | MIT / — | Medidores de uso por proveedor, pill de respaldo para pantallas sin notch. |
| [HIMURAw/gnome-dynamic-island](https://extensions.gnome.org/review/75738), [Dynamic Music Pill](https://github.com/Andbal23/dynamic-music-pill) | — | Las extensiones GNOME que Coucou reemplaza (isla de notificaciones y música). |

### Ideas ordenadas por valor para Alan
1. **Hecho (sin ver en pantalla real):** denegar con motivo (3 motivos fijos, pruebas del hook OK) · sesión en tu terminal · mantener despierto mientras trabajan agentes (Ajustes → General → Stay awake, apagado por defecto; el bloqueo `sleep` fue **denegado** desde la sesión sin asiento donde probé, en tu escritorio real debería permitirse; si no, queda un aviso en el log) · Mochi con sueño.
2. **Aprobar «todas las iguales»** de la cola (misma herramienta, mismo input, mismo directorio): muy natural sobre la cola que ya existe.
3. **Hecho:** etiquetas de riesgo en la tarjeta de permisos (`island/risk.ts`: borra archivos, ejecuta código descargado, root, force-push, red…; 16 asserts; solo informativas, nunca deciden).
4. **Historial de decisiones / «tu día»** con tiempo de espera por permiso: ya existe `recap`, falta la vista.
5. **Módulo waybar/CLI** no aplica (GNOME), pero un `coucou status` por terminal sí encaja con `coucou-island`.
6. **Reglas propias en modo «observar»** antes de dejarlas aprobar: la forma segura de tener «Always allow».
