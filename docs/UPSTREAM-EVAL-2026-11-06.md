# Upstream (Louis-CFM/coucou 0.6) → nuestra versión Linux/Android

Fecha: 2026-11-06. Base de nuestro fork en `windows/`: 0.1.9 (merge-base `83708fe9`).
Upstream va 999 commits adelante (`windows/`: +104k líneas, 272 archivos): i18n de 14 idiomas, Flow
(dictado/Notetaker), Mail, WhatsApp, GitLab/Gitea, updater, etc.

**Decisión: no hacer `git merge upstream/main`.** El conflicto sería enorme (island.ts, views, settings,
hooks reescritos) y buena parte del código nuevo duplica cosas que ya tenemos (Hub, Handy, Pomodoro,
Jinx). Se portan funciones sueltas, reimplementadas o con `git apply -p1` cuando el parche aplica.

## Ya portado (rama `worktree-organize-ui-upstream`)
| Qué | Origen | Nota |
|---|---|---|
| Cola de permisos (hasta 5) | #870/#984 | Antes el 2.º permiso se devolvía a la terminal. Ahora espera su turno, tarjeta muestra «+N waiting», cada solicitud mantiene su reloj de 110 s. **Sin probar en vivo** (no hay test automatizado para `hooks.ts`). |
| Ticker de pasos no se congela en el paso 20 | #265 (`0b700de4`) | Bug real en nuestro fork; parche aplicado tal cual. |
| Env de apps hijas (GDK/GST) | #859 | Ya cubierto por `workspaces.rs`. |

## Recomendado portar a continuación (orden)
1. **Iniciar sesión de Claude Code desde la isla** (0.6, Win/Linux `497c5ea0`/`e693a812`; Mac `7d65c9a3`).
   Elegir proyecto (carpetas de `~/.claude/projects`), escribir/dictar el prompt, abre la terminal con `claude`.
   El prompt nunca va en la línea de comandos. Encaja como herramienta del Hub (grupo Focus). Valor alto, ~1 día.
2. **Soltar un archivo sobre una sesión** (0.6): su terminal pasa al frente y la ruta cae en el prompt. Medio.
3. **Alertas de cualquier pill** + **uso del plan en la isla compacta** (#976/#979): ya tenemos `rate_limits.json`. Bajo coste.
4. **Mochi con sueño tras muchas horas** (#970, Mac+) y **presión de memoria** (#795; en Linux `/proc/meminfo`). Barato, simpático.
5. **Tarjeta «terminado» se va con un prompt nuevo** (#587) y **tarjeta respondida en terminal se cierra sola** (#740). Revisar si nos afecta.
6. **Teléfono ↔ Mac/PC «phone link»** (#500): nosotros usamos nuestro gateway SSH; solo mirar el protocolo si se quiere unificar.
7. **Android 0.4.0**: #900 (isla descentrada con resolución baja) — pequeño, revisar en el S21/S22.

## Funciones de Mac: ¿valen la pena en Linux?
| Función Mac | Veredicto |
|---|---|
| Start session / drop on session | **Sí** (ver arriba; ya existe versión Win/Linux). |
| ⌘⇧2 captura completa + miniatura arrastrable | No: GNOME ya tiene Shift+PrtSc y portapapeles. |
| Coucoudoro (timer de foco) | No: tenemos Pomodoro más completo (stats, Jinx). |
| Mochi sleepy / memory pressure | **Sí**, barato. |
| Mail pill (Apple Mail) y Mail ordenado | No portable. Equivalente (Thunderbird/Gmail) solo si lo pides. |
| «Coucou en una esquina» | No: choca con nuestro offset/dodge y Just Perfection. |
| Chat con tu suscripción (`claude -p`) | No: el chat ya va a Jinx/Hermes. |
| Flow (dictado, Parakeet, Notetaker) | No por ahora: Handy ya dicta con CapsLock. Reconsiderar si hace falta Wayland/Notetaker. |
| Isla cerrada siempre visible (hora/Mochi) | Opcional, riesgo medio (ventana tipo `notification` en GNOME). |
| Voz con slider de velocidad, App Store review | Solo Mac. |
| Aprobar Antigravity/OpenCode desde la isla | Solo si usas esos agentes. |
| Nix flake, PKGBUILD, Wayland portal | No aplica (Zorin/GNOME X11). |

## Organización de la UI (hecho)
- **Hub**: 12 pestañas en una tira con scroll → 4 grupos (Today / Focus / System / Files) con sub-pestañas
  subrayadas. ←/→ recorren todo; 1–9 eligen dentro del grupo; el grupo recuerda su última pestaña.
  Altura del Hub 214 → 232 px.
- **Ajustes**: una página de 8 secciones → barra lateral (Claude, Integrations, Mochi & music, Hub, General),
  una categoría a la vez, buscador sobre todas, `#hub` abre directo. La sección Hub se divide en
  Server & Jinx / Privacy & input / Weather / Uploads y los textos de ayuda quedan pegados a su control.
- Verificado: `tsc`, `vite build` y capturas de Ajustes con Chromium headless. **El Hub en la isla real
  no se vio** (necesita Tauri + sesión gráfica): comprobar tras reiniciar Coucou.
