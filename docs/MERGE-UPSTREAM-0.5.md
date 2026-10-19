# Merge del original (Windows/Linux 0.5.0 y Mac 0.5.1) en el fork

Hecho el 19 de octubre de 2026 en una sesión en la nube (Linux, sin pantalla, sin Mac ni móvil). Trae `upstream/main`
en `98a767c` (`Louis-CFM/coucou`, 96 commits desde `1456bd3`). Las decisiones de `MERGE-UPSTREAM-0.4.md` siguen
vigentes; aquí solo lo nuevo.

## Qué trae el original que nos importa (`windows/`)

- **Flow**: dictado nativo. Mantienes una tecla, hablas, sueltas y el texto se pega donde escribes. Reconocimiento local con
  Parakeet (sherpa-onnx, 25 idiomas, sin internet), diccionario, snippets, estilos por app, estadísticas. En Wayland usa el
  portal de atajos. `src-tauri/src/flow/`, `speech/`, `src/core/flow.ts`, `src/pill/`.
- **Notetaker**: graba el micrófono y lo que suena en el equipo, transcribe por persona y resume con la clave de Claude.
  `src-tauri/src/notes/`.
- **« OK Coucou »**: asistente por voz (palabra de activación, Claude como cerebro con tu clave). `src-tauri/src/voice/`,
  `src/island/voice.ts`. Apagado por defecto.
- **La ventana de Coucou**: ventana propia con barra lateral (Flow y Notch). `src/hub/shell.ts`, `hero.ts`, `pages/`.
- **Spotify en Windows** (controles del sistema), y arreglos de zoom, etiquetas y plugin de OpenCode 2.
- Solo Mac (llegan en `NotchBuddy/`, **no compilado aquí**): WhatsApp, captura de pantalla, Mochi que siente el Mac.

## Decisiones

1. **Dictado: se queda el de ellos.** Lo nuestro (`dictation.rs`) no dictaba: solo detectaba que Handy grababa y mostraba
   «Escuchando…». Se conserva ese archivo porque también es el **indicador de micrófono** (el punto naranja con el programa
   que graba), que el original no tiene. Cambio: la propia Coucou (Flow, Notetaker, OK Coucou) ya no cuenta como «otro
   programa grabando»: se excluye por el pid que da el servidor de sonido (un programa no puede decir que es Coucou).
2. **Tres archivos nuestros se renombraron antes del merge** porque chocaban con los del original: `hub/ui.ts` →
   `hub/hub-ui.ts`, `hub/hub.css` → `hub/hub-island.css`, `notes.rs` → `hub_notes.rs`. El Hub de la isla (tareas, notas,
   calendario, Pomodoro…) es nuestro; la «ventana de Coucou» es de ellos y comparte la carpeta `src/hub/`.
3. **La ventana de Coucou sustituye a la ventana de Ajustes.** Es una ventana normal (sale en la barra de tareas); se quitó
   nuestro `skip_taskbar` de Linux. Abrir `coucou` sin argumentos la muestra; `coucou --toggle` sigue abriendo o cerrando la isla.
4. **Páginas de la barra lateral.** Lo nuestro entra así: Mochi en *General*; una página nueva **«Jinx y Hub»** con los
   ajustes del Hub y de la música. La página «Teléfono Android» del original está oculta (empareja con *su* app, no la
   nuestra; decisión 10 del merge anterior): `HIDDEN_PAGES` en `hub/shell.ts`, con la línea del original intacta.
5. **Píldoras**: igual que antes (Claude Code naranja, Music y Jinx nuestras). Spotify ahora es de las dos plataformas.
6. **Isla**: se usa el ancho real de la ventana (arreglo de zoom del original) también en nuestro código de «esquivar ventanas».
7. **Traducciones**: `extra.json` y `strings.json` se mezclan por clave (tres vías); `strings.json` se regenera del Mac.
8. **Android**: no se toma nada del original (decisión 1 anterior); se quitó otra vez su `Pills.kt` y se dejó nuestro
   `phone-link.yml`.

## Qué se comprobó aquí y qué no

| Comprobación | Resultado |
|---|---|
| `npx tsc --noEmit`, `npm test` | limpio, 430/430 |
| `cargo test --workspace` (con `SHERPA_ONNX_ARCHIVE_DIR`, ver abajo) | 653 + 37, 0 fallos |
| `npx vite build`, `scripts/check-contracts.sh`, Jinx (Python), panel VGA | ok |
| **Dictar de verdad** (tecla, micrófono, Parakeet, pegar) | **no** (sin pantalla ni micrófono) |
| **Notetaker, OK Coucou, la píldora y la ventana** en un escritorio | **no** |
| Swift (`NotchBuddy/`) | **no compilado** (no hay Mac) |

Compilar en un contenedor con proxy: la librería de sherpa-onnx se descarga en el `build.rs` con un cliente que no usa el
certificado del proxy. Se baja a mano con `curl` y se indica `SHERPA_ONNX_ARCHIVE_DIR` (una carpeta con
`sherpa-onnx-v1.13.8-linux-x64-static-lib.tar.bz2`). También hacen falta `libasound2-dev` y `libpulse-dev`.

## Lista de pruebas para el portátil

- [ ] `cd windows && npm ci && npx tauri build --no-bundle`; instalar; `coucou-island stop; coucou-island`.
- [ ] Abrir Coucou: sale la ventana nueva con **Flow** y **Notch**; «Jinx y Hub» está en Notch; no hay «Teléfono Android».
- [ ] Flow → Dictation: activar; Download (~700 MB, una vez); mantener la tecla (Ctrl derecho), hablar, soltar: el texto se
      pega. En GNOME/Wayland: Ctrl+Alt+V y el permiso de escribir. Probar en la terminal, el navegador y VS Code.
- [ ] Mientras dictas, el punto naranja del micrófono **no** dice «coucou-island» (decisión 1).
- [ ] Si Handy sigue instalado: apagarlo o quitar su atajo, para que no dicten los dos.
- [ ] Notetaker: grabar una llamada corta; la transcripción por persona; el audio se borra al terminar.
- [ ] «OK Coucou» (apagado por defecto): activarlo solo si se quiere; usa la clave de Claude. Ojo: hace lo mismo que Jinx
      con otra voz; ver la nota de abajo.
- [ ] La isla de siempre sigue igual: Jinx, Music, tarjetas, esquivar ventanas, abrir al pasar el ratón.
- [ ] `coucou --toggle` desde el atajo de teclado sigue abriendo y cerrando la isla.
- [ ] Con la isla oculta y Flow apagado, 0 % de CPU.

## Pendiente de decidir (Alan)

- **« OK Coucou » contra Jinx.** Los dos son «habla con un asistente». OK Coucou usa Claude con la clave del equipo; Jinx
  es Hermes en el servidor. Recomendación: dejar OK Coucou apagado, y si se quiere voz para Jinx, mandar lo dictado por Flow
  al chat de Jinx. No se tocó nada.
- **Handy**: ya no hace falta si Flow funciona bien. Quitarlo después de probar.
