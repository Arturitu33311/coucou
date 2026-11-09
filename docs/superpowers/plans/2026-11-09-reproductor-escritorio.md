# Reproductor de escritorio — plan de implementación

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Una ventana de escritorio de Coucou (pantalla completa, debajo de las apps) que muestra el reproductor estilo iBaila del boceto aprobado, y que se usa cuando el espacio de trabajo no tiene ventanas visibles; si no, el panel del notch.

**Architecture:** Ventana Tauri nueva `desktop-player` (Rust `src-tauri/src/desktop_player/`) con tipo X11 DESKTOP en el monitor principal, mostrando una página Vite nueva `desktop.html` (`src/desktop-player/`). Un vigía X11 (x11rb, eventos de la raíz, sin sondeo) decide "escritorio a la vista" y lo emite a la isla y a la página. La página reusa el contrato del reproductor (`src/core/player.ts`, `Bridge.player*`, `player_art`), el tinte, las barras y la letra de la isla, y el reloj y el medidor del fondo actual.

**Tech Stack:** Tauri 2, Rust (x11rb 0.13, gtk 0.18, gdkx11), TypeScript sin framework, Canvas 2D, CSS backdrop-filter.

**Spec:** `docs/superpowers/specs/2026-11-09-reproductor-escritorio-design.md` (amplía `docs/superpowers/specs/2026-11-08-reproductor-cristal-design.md`). Boceto: https://claude.ai/artifact/FrUMW9RjHyrgUPuqEEa5A2 (versión 5) — la fuente visual; su `Main.dc.html` está en `.superpowers/mockup/project/Main.dc.html`.

Rutas relativas a `windows/` salvo que se diga otra cosa.

## Global Constraints

- Fiel al boceto v5: reloj arriba izquierda (el de `linux-wallpaper/wallpaper.html`, mismo dibujo), medidor de cristal arriba derecha, "Continuar escuchando" a la izquierda (tarjeta actual + filas **rotateY(22deg) hacia el centro, sin translateZ**, desvaneciéndose), cover flow de cristal al centro con reflejo, búsqueda con selector, barra de reproducción, línea de letra, luz fina en bordes con el color de la portada, escena teñida.
- Liquid glass: clases `.lg` y `.cover` del boceto (gradientes, bordes de luz, `backdrop-filter: blur(26px) saturate(170%)`), compartidas con el panel del notch solo donde no cambien vistas existentes.
- Una sola ventana de escritorio: tipo `_NET_WM_WINDOW_TYPE_DESKTOP`, `skip_taskbar`, `skip_pager`, no en Alt+Tab/Actividades, debajo de las ventanas normales; monitor principal.
- "Escritorio a la vista" = el espacio de trabajo actual no tiene ventanas normales visibles (todas minimizadas o ninguna); se reevalúa por eventos X11 (`PropertyNotify` en la raíz: `_NET_CLIENT_LIST_STACKING`, `_NET_CURRENT_DESKTOP`, `_NET_WM_STATE` de las ventanas), nunca en bucle.
- `Super+Alt+M` y la lupa: escritorio si está a la vista, si no el panel del notch.
- Sin música: cover flow con álbumes del último artista del historial; sin historial, oculto.
- Letra: el mismo ajuste `musicOnline` de la isla, nada nuevo hacia lrclib.
- 0 % CPU con el escritorio tapado: nada anima ni sondea; las barras (cava) solo con el escritorio visible y música sonando.
- Hidamari: en el perfil diario se apaga cuando Coucou muestra su escritorio; `linux-wallpaper/wallpaper.html` queda intacto como respaldo. (Alan: si se ve bien, todos los perfiles — se hace en la Tarea 7 tras su OK.)
- Reglas de `CLAUDE.md`; no cambiar vistas existentes; i18n en `src/i18n/extra.json` (9 idiomas); secretos fuera del repo.
- Verificación: `cd windows && npx tsc --noEmit`; `cargo test --manifest-path windows/Cargo.toml -p coucou`; Node 22 `--test "tests/**/*.test.mjs"` (2 fallos i18n viejos conocidos, #212 y #214).

## Review Focus

1. **Una app maximizada o en mosaico en el espacio actual** → el escritorio se oculta y `Super+Alt+M` abre el notch. Test `desktopHiddenWhenAWindowIsVisible` (Tarea 3, función pura).
2. **Cambiar de espacio de trabajo** a uno vacío → el escritorio aparece sin parpadeos ni CPU extra. Test `emptyWorkspaceShowsDesktop` (Tarea 3).
3. **La ventana de escritorio nunca tapa ni roba clics a una app**, ni sale en Alt+Tab. Verificación en la Tarea 1 (ensayo) y Tarea 2 (captura + `xprop`).
4. **Escribir en la búsqueda del escritorio** recibe el teclado (mismo problema de foco que el notch; usar el arreglo `windowactivate` de `platform/linux.rs::set_activating`). Tarea 2, prueba en vivo.
5. **Coucou cerrado o caído** → el fondo de Hidamari vuelve (no queda la pantalla vacía). Tarea 7.

---

### Task 1: Ensayo — ventana DESKTOP encima de Hidamari, debajo de todo, con teclado

**Files:** `docs/superpowers/specs/2026-11-09-escritorio-ensayo.md` (resultado); código de ensayo descartable fuera del commit final.

- [ ] Crear desde Rust una ventana Tauri de prueba a pantalla completa en el monitor principal con type hint `Desktop`, `skip_taskbar_hint`, `skip_pager_hint`, transparente, mostrando una página con un `<input>`.
- [ ] Con Hidamari corriendo: comprobar con `xprop -root _NET_CLIENT_LIST_STACKING` y capturas (`import -window root`) que queda encima de Hidamari y debajo de una ventana normal; que no sale en Alt+Tab ni en Actividades.
- [ ] Comprobar que el `<input>` recibe el teclado tras `xdotool windowactivate <xid>` y tras un clic.
- [ ] Si la ventana queda debajo de Hidamari: probar a ocultar Hidamari (`gsettings`/perfil o `xdotool windowunmap` del escritorio de Hidamari) y documentar cuál funciona.
- [ ] Escribir el resultado (qué hint, qué orden, qué falló) en el archivo del ensayo y commitear solo ese archivo. **Si nada funciona: detenerse y reportar** (el diseño dice que el ensayo decide si se sigue).

### Task 2: Ventana `desktop-player` en Rust

**Files:**
- Create: `src-tauri/src/desktop_player/mod.rs`, `src-tauri/src/desktop_player/window.rs`
- Modify: `src-tauri/src/lib.rs` (módulo, comandos), `src-tauri/src/platform/linux.rs` (hint DESKTOP y activación de teclado reutilizando `set_activating`), `src-tauri/tauri.linux.conf.json` si hace falta CSP para `desktop.html`

**Interfaces:**
- Produces: `pub const LABEL: &str = "desktop-player";` `pub fn ensure(app: &AppHandle) -> Option<WebviewWindow>` (crea una vez, oculta), `pub fn show(app)`, `pub fn hide(app)`; comandos `desktop_player_focus(focused: bool)` (teclado para la búsqueda) y `desktop_player_geometry() -> {x,y,w,h,scale}` del monitor principal.
- Linux X11 solo; en Windows/Wayland `ensure` devuelve None y nada cambia.

- [ ] Tests puros: `primary_monitor_rect` elige el monitor principal de una lista; `geometry_scales_to_logical`.
- [ ] Implementar con lo que decidió el ensayo de la Tarea 1.
- [ ] Verificar en vivo (captura y `xprop`) y commit `feat(desktop-player): desktop-level window on the primary monitor`.

### Task 3: Vigía "escritorio a la vista"

**Files:** Create `src-tauri/src/desktop_player/visibility.rs`; Modify `lib.rs` (arranque), `src/core/bridge.ts` (`onEvent("desktop-visible")`), `src/core/state.ts` (`State.desktopVisible: boolean`).

**Interfaces:**
- `pub struct Win { pub id: u32, pub desktop: Option<u32>, pub hidden: bool, pub normal: bool, pub ours: bool }`
- `pub fn desktop_visible(wins: &[Win], current: u32) -> bool` — true si ninguna ventana `normal && !hidden && !ours` está en `current` (o en todos los espacios: `desktop == Some(0xFFFFFFFF)`).
- Hilo x11rb: se suscribe a `PropertyNotify` de la raíz; al cambiar `_NET_CLIENT_LIST_STACKING`/`_NET_CURRENT_DESKTOP`/estados, relee, calcula y emite `desktop-visible` **solo si cambió**; además `desktop_player::show/hide` según el valor.

- [ ] Tests: `desktopHiddenWhenAWindowIsVisible`, `emptyWorkspaceShowsDesktop`, `minimizedWindowsDontCount`, `stickyWindowsCount`, `ourOwnWindowsDontCount` (incluye isla, Mochi, Plank como dock = no normal).
- [ ] Implementar; commit `feat(desktop-player): know when the desktop is in view`.

### Task 4: Página `desktop.html`: fondo, reloj, medidor, luz, letra

**Files:** Create `desktop.html`, `src/desktop-player/main.ts`, `src/desktop-player/scene.ts`, `src/desktop-player/clock.ts`, `src/desktop-player/gauge.ts`, `src/desktop-player/desktop.css`, `tests/desktop-player.test.mjs`; Modify `vite.config.ts` (entrada `desktop`).

- Escena: portada actual ampliada y difuminada teñida con `musicTint`; sin música, colores de la hora (como `wallpaper.html`).
- Reloj: portar el dibujo de `linux-wallpaper/wallpaper.html` (funciones de `clockKey`/`clockFill`/`clockEcho`, ~:195-230; 12 h, misma fuente y relleno) a `clock.ts` sin el carro.
- Medidor: datos de `sysstate` (batería, enchufado, en línea) en píldora `.lg` como el boceto.
- Luz de bordes: `box-shadow` inset con el acento (boceto).
- Línea de letra: la del estado de música de la isla.
- [ ] Tests puros: `clockText(date)` 12 h sin cero inicial; `gaugeLabel(sys)`; `sceneColors(tint|null, hour)`.
- [ ] Implementar fiel al boceto; tsc; commit `feat(desktop-player): scene, clock, gauge, edge light`.

### Task 5: Paneles que se tocan

**Files:** Create `src/desktop-player/recents.ts`, `src/desktop-player/flow.ts`, `src/desktop-player/search.ts`, `src/desktop-player/bar.ts`; Modify `src/desktop-player/main.ts`, `desktop.css`, `tests/desktop-player.test.mjs`, `src/i18n/extra.json`.

- "Continuar escuchando": `Bridge.playerRecents()`; primera = tarjeta grande con play; resto filas `.lg` con `rotateY(22deg)` y opacidad decreciente; punto verde/rojo por fuente.
- Cover flow: reutilizar `coverflowTransform` de `src/views/coverflow.ts` a mayor escala; portadas `.cover` con reflejo; álbumes del artista que suena (`playerArtist`) o del último del historial; clic en álbum → canciones en la lista de resultados.
- Búsqueda: mismo `QueryGate`, debounce y selector que `src/views/player.ts`; resultados en panel `.lg` sobre el cover flow; teclado con `desktop_player_focus(true)`.
- Barra: controles por `Bridge.musicControl`, avance, artista · fuente, letra, DJ (`playerDj`).
- Errores y "Conectar Spotify": mismos mensajes que el notch.
- [ ] Tests: `recentRowsTiltWithoutDepth` (todas `rotateY(22deg)` sin translateZ), `flowFallsBackToLastArtist`, `flowHiddenWithoutHistory`, reuso de `staleResultsAreDropped`.
- [ ] Implementar; tsc; i18n; commit `feat(desktop-player): recents, cover flow, search and player bar`.

### Task 6: Enrutar notch ↔ escritorio

**Files:** Modify `src/island/shortcuts.ts` (`openPlayer`), `src/views/music.ts` (lupa), `src/main.ts` (`--player`), `tests/shortcuts.test.mjs`.

- `openPlayer`, lupa y `--player`: si `State.desktopVisible` → mostrar/enfocar la búsqueda del escritorio; si no → panel del notch (comportamiento actual).
- [ ] Test `openPlayerUsesDesktopWhenVisible` / `...NotchOtherwise`; commit `feat(desktop-player): open where it can be seen`.

### Task 7: Hidamari, respaldo y prueba con Alan

**Files:** Modify `linux-wallpaper/install.sh` o un script nuevo `scripts/desktop-player-profile.sh`; `README.md` de `windows/`.

- [ ] Perfil diario: Hidamari se detiene cuando la ventana de escritorio está lista y se reanuda si Coucou sale o cae (watchdog del `coucou-island` o unidad de usuario).
- [ ] Build release, instalar (respaldo `coucou.prev-*`), prueba en vivo con capturas y con Alan.
- [ ] Con OK de Alan: todos los perfiles.
- [ ] Integrar en `unify/monorepo` solo tras la prueba; `~/estado`.
