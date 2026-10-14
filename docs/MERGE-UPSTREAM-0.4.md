# Merge del original (Coucou de Windows/Linux 0.4.0) en el fork

Hecho el 14 de octubre de 2026 en una sesión en la nube (Linux, sin pantalla, sin Mac ni móvil). Trae
`upstream/main` en `1456bd3` (`Louis-CFM/coucou`) sobre `unify/monorepo` (`7623b20`). Es un merge de verdad:
el próximo `git merge upstream/main` solo traerá lo nuevo. `docs/ANALISIS_UPSTREAM.md` explica el porqué.

## Qué se comprobó aquí y qué no

| Comprobación | Resultado |
|---|---|
| `cd windows && npx tsc --noEmit` | limpio |
| `cd windows && npm test` (tests del original más los ajustados) | 399/399 |
| `cd windows && cargo test --workspace --locked` (app y hook) | 598 + 37, 0 fallos |
| `npx vite build` | compila |
| `bash scripts/check-contracts.sh` | ok |
| Panel VGA: `node --test jinx/vga-panel/test_mochi_logic.js`, `python3 -m unittest discover -s jinx/vga-panel/server`, `build.py` | ok; el bundle sigue en 29 KB |
| La isla en Chromium (modo navegador de `npm run dev`) | sin errores de JS; antes y después se ven igual (compacta, abierta, píldora de música); armario, chat y Hub abren |
| **La app Tauri de verdad en un escritorio** | **no** (no hay pantalla) |
| **Swift (`NotchBuddy/`)** | **no compilado** (no hay Mac); solo se resolvieron 3 bloques de texto |
| `android/` | sin cambios: es el nuestro, idéntico a antes del merge |

## Decisiones del merge (para no deshacerlas en el próximo)

1. **`android/` nunca viene del original.** Su app Android (otro autor, MIT) vive en el mismo directorio que la
   nuestra (SmartIsland, GPL-3.0). Se quitaron sus workflows `android.yml` y `android-release.yml`, y de
   `phone-link.yml` los trabajos que la usan. Los vectores de prueba de su relay que `phone_link` incluye se copiaron a
   `windows/src-tauri/src/phone_link/testdata/`. Receta en `docs/MONOREPO.md`.
2. **Preguntas de Claude Code (`AskUserQuestion`): se queda nuestro camino**, el hook `coucou-hook --ask` en
   `PreToolUse`, que es también el del Mac del original (desde Claude Code 2.1.85 la pregunta ya no llega como
   `PermissionRequest`). Un `PermissionRequest` de `AskUserQuestion` de Claude Code vuelve al terminal; el de otro
   agente es un permiso normal. La tarjeta de preguntas del original sigue en el código pero no se usa. La app contesta
   `{"permissionDecision":"answer","answers":…}`, que entienden las dos vías del hook.
3. **Hook:** `main.rs` es el del original (diff en vivo, statusline, Claude Desktop, Cursor, respuestas por agente) más
   nuestro `--remote` (el enlace por ssh con los móviles) y `--ask`.
4. **Dos módulos con el mismo nombre:** nuestros `agents.rs` / `island/agents.ts` (las sesiones de Claude Code en el chat)
   pasan a llamarse `claude_sessions.rs` / `island/claude-sessions.ts`. Los `agents.*` del original son los instaladores
   de hooks y los nombres y colores de los agentes.
5. **Mochi de PC: se queda el nuestro** (`mochi/wardrobe.ts` dibuja, `mochi/outfits.ts` elige), con los 21 atuendos y las
   15 emociones. El original tenía los nombres al revés. Sobre el nuestro van: su API de armario (al final de
   `outfits.ts`), sus textos en `mochi/wardrobe-labels.ts` (aparte para que el panel VGA no cargue las traducciones), los
   iconos del armario en `mochi/wardrobe-icon.ts` (dibujados con nuestro motor) y su baile con la música en `engine.ts`.
6. **Catálogo de píldoras:** la principal sigue llamándose "Claude Code" en naranja (el original: "VS Code" en blanco);
   `integration_music` es nuestra píldora Music de MPRIS, solo en Linux; `integration_jinx` está en el catálogo. Los tests
   de paridad con el Mac se ajustaron para dejar escritas esas tres diferencias.
7. **Abrir al pasar el ratón:** se queda el nuestro (espera a que el puntero repose, con el tiempo configurable). El del
   original abre al instante sobre el mismo ajuste, así que está desactivado; su fila duplicada en Ajustes se quitó.
8. **Ventana en X11 (GNOME): cambia el valor por defecto.** Sin `COUCOU_X11_MODE` la isla es ahora una ventana *Dock*
   siempre encima, como en el original (lo deja en paz "mostrar escritorio"). `COUCOU_X11_MODE=normal` vuelve a lo de
   antes; `utility`, `or`, etc. siguen igual.
9. **Chat:** se quedan nuestros destinos (sesiones, nuevo agente, Jinx, Mochi). El destino Mochi gana el selector de
   proveedor y modelo del original (Anthropic, Google, OpenAI, OpenRouter, Ollama, LM Studio, servidor propio), el
   Markdown y el streaming. Mochi aparece si hay cualquier proveedor configurado, no solo la clave de Anthropic.
10. **Ajustes:** no se muestra la sección del móvil Android del original (empareja con *su* app). El servidor
    `phone_link` existe en Rust pero sigue apagado por defecto.
11. **Otros sin cambio visible:** la tarjeta de soltar archivo es la nuestra (Jinx, OneDrive, Google Drive); los iconos
    de reproducción son los nuestros; conviven nuestro sonido de "terminado" y la carpeta de sonidos del original.
12. **Música:** están las dos píldoras, Music (la nuestra, con letras) y Spotify (la del original). Si se activan las dos,
    hay dos lectores de MPRIS.
13. La versión de `windows/` pasa a 0.4.0 (la del original).

## Lo que trae el original (resumen)

Codex, Copilot CLI y Muse Code con Allow/Deny desde la isla; Gemini, Antigravity, Cursor, OpenCode, Amp y Hermes en su
píldora; un único escritor de configuración (diff, copia fechada, huella) para todos; uso del plan de Claude y Codex en
la cabecera; diff en vivo de cada archivo editado; GitHub (PRs, CI, reviews); atajos globales (portal de Wayland,
`Ctrl+Alt+D` = Mochi al escritorio); Mochi en el escritorio; resumen semanal; 10 idiomas; Spotify por MPRIS; colores por
píldora; arreglos de Linux (cierre en KDE/Wayland y GNOME, Hyprland/Sway, texto grande de GNOME, terminal al frente) y
paquete Arch (`linux/arch/`).

## Lista de pruebas para el portátil

Compilar e instalar: `cd windows && npm ci && npx tauri build --no-bundle`, luego `coucou-island stop; coucou-island`.

- [ ] La isla aparece donde siempre en GNOME/Zorin (decisión 8). Si no: `COUCOU_X11_MODE=normal`.
- [ ] Ajustes → reinstalar hooks: el diff de `~/.claude/settings.json` añade solo lo esperado; se hace copia fechada.
- [ ] Claude Code: tarjeta de permiso (Allow/Deny), pregunta con opciones (una y varias), "Reply in terminal".
- [ ] El móvil por ssh (`coucou-hook --remote`, S21 y S22) sigue viendo sesiones y contestando.
- [ ] Píldora Music con letras y barras; la isla compacta no se esconde mientras suena.
- [ ] **Jinx como principal (después del merge):** primero `jinx/jinx-assist/install.sh` (el gateway de `~/bin`
      tiene que aceptar el origen `pc`; el anterior contesta "src desconocido"). Luego: la píldora de Jinx es la
      primera y está en foco; `Ctrl+Alt+Space` abre el chat con ella; el chat muestra el hilo de los móviles y
      Telegram con su origen; lo que escribes aparece en Telegram como `💻 …`; la línea de estado dice quién
      contestó (Hermes o respaldo). Ajustes → Hub → *Jinx through* = *Hermes API* recupera el camino anterior.
- [ ] Jinx con *Hermes API*: chat, respuestas en streaming, tarjeta de permiso.
- [ ] Android: `cd android && ./gradlew :app:compileDebugKotlin :app:testDebugUnitTest` (aquí pasó con un
      espejo de Maven Central: 354 tests) e `install -r` en S21 y S22.
- [ ] Hub (Today, Tasks, Pomodoro, Notes, Clipboard…) y presence entre portátil y móvil.
- [ ] Chat → Mochi: el selector de modelo; Ollama si está.
- [ ] Clic derecho en Mochi: armario con los 21 atuendos; Mochi baila con Spotify si se activa esa píldora.
- [ ] Atajos globales y `Ctrl+Alt+D` (Mochi al escritorio y de vuelta).
- [ ] Idioma en Ajustes (por ejemplo, español) y vuelta a "sistema".
- [ ] Con la isla oculta, 0 % de CPU (`top`).
- [ ] Panel VGA: `jinx/vga-panel/deploy.sh` solo desde el portátil, si se quiere publicar.

Lo que no esté bien se arregla sobre esta rama antes de fusionarla en `unify/monorepo`.
