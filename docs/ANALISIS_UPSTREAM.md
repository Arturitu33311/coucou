# Análisis: nuestro fork frente al Coucou original

Documento de trabajo para retomar en otra sesión. Compara `Arturitu33311/coucou` (nuestro fork) con
`Louis-CFM/coucou` (el original), con foco en Linux, Android y la parte visual de Mac e iPhone, y dice
qué merece la pena traer.

- **Fecha del análisis:** 14 de octubre de 2026
- **Rama de trabajo:** `ccr-ace9a467-157yax`
- **Punto donde se separó el fork:** `83708fe` — *Prepare Coucou 0.1.9 (#261)*
- **Original analizado:** `upstream/main` en `1456bd3` (14 oct 2026, *Merge #647 flow-pill-live-mochi*)
- **Distancia:** el original va **402 commits** por delante: 710 archivos, unas 146 000 líneas añadidas

> Limitación importante: todo esto sale de leer código, diffs y el CHANGELOG. **No se ha compilado ni
> ejecutado nada** (el entorno era Linux, sin Xcode ni SDK de Android). Las valoraciones de "vale la pena"
> son de criterio, no de haber probado las funciones.

> **Actualización (14 oct 2026, sobre `unify/monorepo` en `452e95a`): §2, §4, §5.2 y §8 se han quedado viejos.**
> Este análisis se hizo antes de unificar el monorepo. Desde entonces:
>
> - `android/` es nuestra app (SmartIsland, GPL-3.0) y el original tiene **otra** app en el mismo `android/`.
>   Las dos chocan (15 archivos, `android/LICENSE` incluido). **Nunca se toma `android/` del original**:
>   ver `docs/MONOREPO.md`.
> - `windows/` ya no es la 0.1.1 del original: lleva unas 22 600 líneas nuestras (Jinx, presence, tareas,
>   enlace por ssh con el móvil…). El ensayo de merge da **47 archivos en conflicto** (unos 130 bloques), no 2:
>   unos 30 en `windows/`, 15 en `android/` y los 3 bloques de Swift de §4, que siguen igual.
>   `windows/src/mochi/outfits.ts`, `wardrobe.ts`, `src-tauri/src/agents.rs` e `island/agents.ts` los escribieron
>   los dos lados por separado.
> - Los commits de Mochi de §3 (`5440f3a`, `1475442`, `aca9129`) ya están en `unify/monorepo` (merge `2879e04`),
>   y `7623b20` los porta: los 6 atuendos a PC (`windows/src/mochi/wardrobe.ts`) y a Android
>   (`MochiWardrobe.kt`), y las 15 emociones a PC (`engine.ts`). Android no tiene capa de emociones.
> - Resuelto un punto de §9: el Portfolio del original guarda las claves de los exchanges en el Keychain.
>
> **El Mochi de PC: se queda el nuestro.** Los dos lados portaron el armario del Mac por separado y con los nombres
> de archivo al revés: nuestro `wardrobe.ts` dibuja y `outfits.ts` elige; en el original es al contrario. Git lo
> marca como un conflicto de archivo entero, así que hay que elegir uno. Nos quedamos con el nuestro porque lleva
> los 6 atuendos y las 15 emociones, los extras de PC (auriculares, gorro de dormir, gafas) y el panel VGA lo
> empaqueta. Del original solo usan su API `windows/src/views/wardrobe.ts` (su selector) y su `engine.ts`; el
> original apenas ha tocado `windows/src/mochi/` desde el 9 de octubre. (`BotEmoteName`, `EMOTE_EYE` y
> `triggerEmote` existen en los dos lados desde antes de separarse: no son algo que solo tenga el original.)
>
> **Hecho (14 oct 2026):** el merge del punto 1 está en `docs/MERGE-UPSTREAM-0.4.md`, con sus decisiones y la lista
> de pruebas para el portátil.
>
> **Plan vigente** (no se usa Mac, así que la parte Swift no es prioritaria):
>
> 1. Merge completo de `upstream/main` en una rama desde `unify/monorepo`, con `android/` siempre de nuestro lado.
>    En `windows/`, resolver por capas y comprobar cada una: `hook/` (`cargo test`) → `src-tauri/`
>    (`cargo check`) → `src/core` (`tsc`) → `island/`, `views/`, `settings/` → `mochi/`.
>    En `mochi/`: `wardrobe.ts` y `outfits.ts` de nuestro lado; `engine.ts` resuelto bloque a bloque (sus cambios:
>    baile con Spotify, Mochi en el escritorio, saludo v2); su selector y su Mochi de escritorio adaptados a
>    nuestra API o descartados. Después, tests del panel VGA. Los 3 bloques de Swift, con los dos lados y sin compilar.
> 2. Hacer que las emociones nuevas se vean en PC: hoy nada llama a `triggerEmote` con ellas.

---

## 1. Cómo reproducir el análisis

```bash
# Traer el original como rama remota (el clon del fork es superficial; --depth basta)
GIT_LFS_SKIP_SMUDGE=1 git fetch --depth=400 https://github.com/Louis-CFM/coucou main:refs/remotes/upstream/main

git merge-base HEAD upstream/main                  # -> 83708fe (fork point)
git rev-list --count 83708fe..upstream/main        # -> commits que nos faltan
git diff --dirstat=files,2 83708fe upstream/main   # qué áreas cambiaron
git diff 83708fe upstream/main -- CHANGELOG.md     # resumen oficial de versiones

# Ensayo de merge sin tocar nada (lista los conflictos)
git merge-tree --write-tree --name-only HEAD upstream/main
```

---

## 2. Resumen ejecutivo

| Área | Nosotros | Original | ¿Vale la pena? |
|---|---|---|---|
| Linux (app Tauri en `windows/`) | 0.1.1 | 0.4.0 | **Sí, 9/10** si se usa Linux |
| Android (`android/`) | no existe | 0.2.0 | Depende: **2/10** con Mac, **8/10** con Linux/Windows |
| Mac: arreglos y rendimiento | 0.1.9 | 0.3.0 | **Sí, 10/10** |
| Mac: mejoras visuales de la isla | 0.1.9 | 0.3.0 | **Sí, 8–9/10** |
| Mac: voz y dictado | — | 0.3.0 + Unreleased | **6–7/10**, opcional |
| Mac: píldoras Spotify / Crypto / Portfolio | — | sí | **3/10** |
| iPhone | — | casi nada nuevo | **1/10** |

**Recomendación:** hacer un **merge completo** de `upstream/main` en nuestra rama, no elegir commits
sueltos. El ensayo de merge da **solo 2 archivos con conflicto (3 bloques)**, y los tres se resuelven
quedándose con ambos lados. Ver §4.

---

## 3. Qué tiene ya nuestro fork (para no perderlo en el merge)

Tres commits nuestros sobre `83708fe`, todos en `NotchBuddy/Sources/CoucouKit/`:

| Commit | Qué hace |
|---|---|
| `5440f3a` | 6 atuendos nuevos (necktie, monocle, headband, earmuffs, bandana, wizardHat) y 5 emociones (sleepy, confused, excited, shy, sad) |
| `1475442` | 5 emociones más: curious, bored, scared, grumpy, smug |
| `aca9129` | Ojos propios para cada emoción (14 `EyeShape` nuevas), partículas nuevas (`tear`, `question`, `anger`, `puff`), 5 emociones más (laughing, crying, relieved, determined, flustered), animaciones más ricas, `holdMs()` para que una duración corta no dé un tween de 0 ms, y arreglos de accesorios (diadema de earmuffs sobre la cabeza, piezas que pasan detrás del cuerpo al girar, lazo del headband anclado a la cabeza) |

Archivos tocados: `BotEngine.swift`, `IslandTypes.swift`, `MochiOutfitDrawing.swift`, `MochiWardrobe.swift`.
Total: 15 emociones nuevas (`BotEmote`) y 6 atuendos nuevos (`Outfit`), **solo en Swift (Mac/iPhone)**.

Hay un mockup visual (privado) de todo esto en <https://claude.ai/artifact/X5KndWZAQNeNGBB8bWf1ih>,
calibrado contra `design/captures/`.

Nada de esto se ha compilado. Lo primero en un Mac: `cd NotchBuddy && xcodegen && xcodebuild -scheme NotchBuddy -configuration Debug build`.

---

## 4. Coste del merge: los conflictos exactos

`git merge-tree` sobre `HEAD` (`aca9129`) y `upstream/main` (`1456bd3`):

- **Sin conflicto:** 708 de 710 archivos, entre ellos `MochiOutfitDrawing.swift` y `MochiWardrobe.swift`
  (el original no los tocó).
- **Con conflicto:** `BotEngine.swift` (2 bloques) e `IslandTypes.swift` (1 bloque).

Causa común: el original añadió la emoción **`listening`** para su modo de voz y nosotros añadimos 15
emociones justo en los mismos sitios. **Resolución en los tres casos: quedarse con los dos lados.**

### 4.1 `IslandTypes.swift` — `enum BotEmote`

- Nuestro lado: las líneas `case sleepy…`, `case curious…`, `case laughing…`.
- Su lado: `case listening   // attentive: head tilt + wide-open eyes`, seguido del nuevo
  `enum BotVoiceSubState { none, listening, thinking, speaking }`.
- Resolver: conservar nuestros tres `case`, añadir `case listening`, cerrar el `enum` y dejar
  `BotVoiceSubState` debajo tal cual viene.

### 4.2 `BotEngine.swift` — `triggerEmote` (switch de animaciones)

- Nuestro lado: los casos `.sleepy` … `.flustered`.
- Su lado: `case .listening:` (inclinación `tilt` −0.07 sostenida + `es` a 1.12).
- Resolver: conservar todo. Hay que tener en cuenta que su hold usa
  `CGFloat((duration - 0.4) * 1000)` sin protección: con `duration < 0.4` da un valor negativo. Conviene
  cambiarlo a `holdMs(duration, 0.4)`, el helper que ya tenemos.

### 4.3 `BotEngine.swift` — `emoteEyeShape`

- Nuestro lado: los 15 `case` nuevos.
- Su lado: `case .listening: return .wide`.
- Resolver: conservar todo.

### 4.4 Lo que el merge **no** marcará pero hay que revisar

- `doMiniBehaviorLoop` tiene `default:`, así que `.listening` compila, pero no tendrá comportamiento
  idle propio (en el original tampoco lo tiene).
- El original añadió en `BotEngine` un modo de escucha y voz: `enterListening/exitListening`,
  `enterVoiceCompact/exitVoiceCompact`, `voiceSubState`, `stepDebt` para Macs con pantalla lenta.
  **Guarda y restaura `permanentEmote`** al entrar y salir de la voz. Hay que comprobar en el Mac que
  una de nuestras emociones permanentes (p. ej. `.crying` con lágrimas) se restaura bien después de hablar.
- Su commit `b4343d9` (*idle CPU and smoothness pass*) cambia cómo se avanza la animación. Nuestras
  partículas nuevas usan `age`/`life` igual que las antiguas, así que debería ir bien, pero hay que verlo.

---

## 5. Análisis por área

### 5.1 Linux

Linux comparte app con Windows: es la app Tauri de `windows/` (Rust en `windows/src-tauri/`, TypeScript en
`windows/src/`). Nuestro `windows/package.json` está en **0.1.1**; el original, en **0.4.0**. Para Linux,
el CHANGELOG dice que 0.3.0 es "la primera versión de Linux desde la 0.1.1", así que nos falta todo lo de
0.2.0, 0.3.0 y 0.4.0.

Lo que nos falta (CHANGELOG, secciones *Windows and Linux 0.2.0 / 0.3.0 / 0.4.0*):

- **Agentes:** Codex, GitHub Copilot CLI y Muse Code con Allow/Deny desde la isla. Gemini CLI,
  Antigravity, Cursor Agent, OpenCode, Amp y Hermes en su propia píldora. Un único escritor de config
  endurecido para todos (diff + copia de seguridad fechada antes de escribir). Archivos: `src-tauri/src/agents.rs`, `agent_hooks.rs`, `config_file.rs`.
- **Preguntas** de Claude Code con varias opciones, respondidas desde la isla.
- **Chat:** Anthropic, Google, OpenAI y OpenRouter, más modelos locales (Ollama, LM Studio, cualquier servidor
  compatible con OpenAI), con Markdown y botón de copiar (`chat.rs`, `local_chat.rs`, `openai_compat.rs`).
- **Uso del plan** de Claude y Codex en la cabecera (`codex_plan.rs`, `src/core/plan.ts`).
- **Diff en vivo** de cada archivo editado (`src/core/diff.ts`, `src/views/diff.ts`).
- **GitHub:** PRs con CI, reviews pendientes, CI de las ramas por defecto y cuadrícula de contribuciones (`github.rs`).
- **Mochi:** armario, atuendos de temporada, saludo nuevo y Mochi en el escritorio
  (`src/mochi/outfits.ts`, 1158 líneas; `wardrobe.ts`; `desktop-logic.ts`). **Nuestro fork no tiene
  armario en Windows/Linux:** `windows/src/mochi/` solo tiene `engine.ts`, `greeting.ts` y `minibots.ts`.
- **Atajos globales** configurables. En Wayland se registran con el portal GlobalShortcuts (KDE Plasma 6,
  GNOME 48+, Hyprland, COSMIC) en `portal.rs` y `shortcuts.rs`. `Ctrl+Alt+D` manda a Mochi al escritorio.
- **Resumen semanal** compartible como imagen (`recap.rs`, `src/recap/`).
- **10 idiomas** (`src/i18n/`).
- **Spotify por MPRIS** (Linux): pista, play/pausa, siguiente, carátula, barra de progreso, volumen, y
  Mochi baila (`spotify.rs`).
- **Integración con el escritorio:** cierre automático en KDE/Wayland y GNOME, isla arriba en GNOME,
  fijada a su pantalla en Hyprland y Sway con selector de pantalla, texto grande de GNOME que ya no
  corta la isla, "Open terminal" que trae la terminal al frente en X11 y KDE (pestaña correcta en kitty)
  (`platform/linux_focus.rs`, `session_window.rs`).
- **Abrir al pasar el ratón**, **sonidos propios** (`sounds.rs`) y **color por Mochi** (`src/settings/colors.ts`).
- **Paquete Arch:** `linux/arch/PKGBUILD` y `coucou.desktop`.
- **Servidor para el móvil Android** (`src-tauri/src/phone_link/`, ver 5.2), apagado por defecto.

**Valoración: 9/10 si se usa Linux.** Es la mayor diferencia entre los dos repos. Entra sin conflictos.
Si no usamos Linux ni Windows, no aporta nada.

### 5.2 Android

App nueva (`android/`, 364 archivos, Kotlin), escrita por @Rhytam23 (código MIT; Mochi, nombre, icono y
sonidos con permiso de Louis Raillé, ver `android/PERMISSION.md`). Android 11 o superior, solo en inglés
y solo en modo oscuro. Se distribuye como APK firmada.

- **Solo se empareja con Coucou de Windows/Linux 0.4.0 o superior. No funciona con el Mac.** El lado del
  ordenador vive en `windows/src-tauri/src/phone_link/` y el protocolo está en `docs/ANDROID_LINK.md`.
- Lo que hace: lista de agentes, Allow con huella o bloqueo de pantalla (incluso con el móvil bloqueado;
  Deny no la pide), respuestas a preguntas de Claude, chat a través de las claves del ordenador, uso del
  plan, una "isla" opcional sobre otras apps, ajuste rápido, notificación persistente y **9 widgets**.
- Emparejamiento por QR con certificado fijado (*pinned*); redescubrimiento por mDNS. Fuera de la Wi-Fi de
  casa, relay opcional en Cloudflare con cifrado de extremo a extremo (`android/relay/`, `docs/RELAY_LINK.md`).
- Mochi propio en Kotlin (`android/.../mochi/`, `outfit/OutfitDrawing.kt`) con los **11 atuendos** originales.
- CI nueva: `.github/workflows/android.yml`, `android-release.yml`, `phone-link.yml`.

**Valoración:** con Mac + Android, **2/10**, porque no se conecta al Mac. Con Linux/Windows + Android,
**8/10**. Es un directorio nuevo: traerlo no cuesta conflictos.

### 5.3 Mac

Archivos nuevos en `NotchBuddy/Sources/App/` (más de 60). Los más cambiados: `HookServer.swift`
(+1972), `SettingsView.swift` (+1245), `PortfolioController.swift` (+1056), `IslandWindowController.swift`
(+961), `IslandViewContent.swift` (+656). La versión pasa de 0.1.9 a 0.3.0.

#### a) Arreglos, seguridad y rendimiento — **10/10**

- `~/.claude/settings.json`: nunca se reescribe desde cero si no se puede leer, la copia de seguridad
  tiene que salir bien antes de escribir, y no se escribe si el archivo cambió desde la vista previa (0.2.2, #243).
  Es una regla dura de nuestro CLAUDE.md.
- Crash una hora después de editar un archivo (0.2.2, #286).
- Se respeta el retardo de cierre automático; reabrir la app trae la isla de vuelta.
- Rendimiento: con la isla oculta, el puntero se comprueba 8 veces por segundo en vez de 60; animaciones a
  la frecuencia real de la pantalla; menos trabajo de fondo (commit `b4343d9`). Encaja con la regla
  "0 % de CPU con la isla oculta".
- La respuesta del chat ya no se corta tras el primer bloque cuando hay búsqueda web (#67).

#### b) Mejoras visuales y de uso de la isla — **8–9/10**

- **Cola de permisos** (`ApprovalQueue.swift`, commit `e391a14`): las peticiones se encolan en vez de
  taparse unas a otras, y la tarjeta muestra cuántas hay.
- **Elegir pantalla** (`IslandDisplayChoice.swift`): la del notch, la principal, una concreta o "seguir al ratón".
- **Sesiones desde terminales** (Warp, Terminal, iTerm, Ghostty, cmux, Orca) con "Open terminal" que trae
  la app correcta (`TerminalTarget.swift`, `ClaudeHost.swift`).
- **Abrir al pasar el ratón**, **sonidos propios**, **color propio por Mochi** (`CoucouKit/PillColors.swift`).
- Preguntas mostradas completas, con la descripción de cada opción; Escape pliega un permiso sin responderlo.
- Píldora de Claude Desktop; uso del plan de Codex (`CodexPlanGauge.swift`).
- Agentes nuevos: Copilot CLI, Muse Code, OpenCode, Amp, Hermes.
- 10 idiomas.

#### c) Funciones grandes — **6–7/10**, opcionales

Todas son **solo de la versión de GitHub**: no van en la del App Store.

- **«OK Coucou», hablar con Mochi** (0.3.0, `App/Voice/`, unos 25 archivos, `docs/VOICE.md`): palabra de
  activación, Claude como cerebro con tu clave de Anthropic, acciones (píldoras, música, Stripe, GitHub,
  Vercel, búsqueda web, abrir apps), emails por voz que no se envían sin clic, voz del Mac o ElevenLabs.
  En lo visual, Mochi escucha, piensa y habla (los cambios de `BotEngine` descritos en §4.4). El
  reconocimiento es local; el texto y los datos sí van a Anthropic.
- **Dictado Flow** (*Unreleased*, `FlowDictation.swift`, `FlowPill.swift`, `FlowAudioBands.swift`): mantener
  una tecla, hablar y soltar pega el texto, como Wispr Flow. Una píldora abajo con Mochi y 9 puntos que
  siguen la voz. Necesita permiso de Accesibilidad. Apagado por defecto.
- **Dictado en el chat** (botón de micro).
- **Resumen semanal** (`WeeklyRecapView.swift`, `RecapStore.swift`): tarjeta de los lunes e imagen de
  1080 × 1920 para compartir; el historial se queda en el Mac.

#### d) Píldoras nuevas — **3/10**

- **Spotify** (`SpotifyController.swift`, `SpotifyViews.swift`, `NowPlayingViews.swift`).
- **Crypto / Portfolio** (`CryptoController.swift`, `PortfolioController.swift`, `PortfolioViews.swift`,
  commits `d18e06e` → `8b51a93`, 12 oct, aún no en el CHANGELOG): precios de Coinbase u OKX, y saldos de
  exchanges y wallets de 16 redes. **Maneja claves de exchanges**: superficie sensible. Revisar dónde guarda
  las claves (CLAUDE.md exige el Keychain) antes de usarlo.
- Chat con DeepSeek, Qwen y Moonshot (nuevos casos en `ChatProvider`, en `IslandTypes.swift`).

### 5.4 iPhone — **1/10**

Casi nada: `Phone/SessionDetailView.swift` (5 líneas) y `CoucouKit/PillColors.swift` (compartido).
Widgets, notificación expandida y Live Activity siguen igual. No hay nada visual que traer.

### 5.5 Infraestructura y documentación

- CI: workflows nuevos `android.yml`, `android-release.yml`, `phone-link.yml`, `windows-ci.yml`; cambian
  `build.yml`, `linux.yml` y `windows.yml`.
- Docs nuevas: `docs/ANDROID_LINK.md`, `docs/RELAY_LINK.md`, `docs/RELAY_DEPLOY.md`, `docs/VOICE.md`;
  cambian `SPEC.md`, `INTEGRATIONS.md` y `AGENTS.md`.
- `tests/` y `scripts/` crecen (tests y scripts suman unas 4700 líneas junto con docs y relay).

---

## 6. Impacto sobre nuestro trabajo con Mochi

1. **Nuestros atuendos y emociones solo existen en Swift.** Tras el merge habrá tres motores de Mochi:
   - Mac/iPhone (Swift): 17 atuendos (11 originales + 6 nuestros), 22 emociones (7 originales + 15
     nuestras) más `listening`.
   - Windows/Linux (TypeScript, `windows/src/mochi/`): los 11 atuendos originales (más las opciones
     `auto` y `none` del selector).
   - Android (Kotlin, `android/.../mochi/`): 11 atuendos originales.

   Para tenerlos en todas partes habría que portar 6 atuendos y 15 emociones (con sus formas de ojo y
   partículas) dos veces más. Es un trabajo aparte, y conviene hacerlo **después** del merge, sobre el
   `outfits.ts` del original y no desde cero.
2. **Los ids son contratos.** `Outfit` se guarda en UserDefaults (`mochiOutfit`) por su `rawValue`. Si algún
   día se portan, hay que usar los mismos ids en TS y Kotlin (`necktie`, `monocle`, `headband`, `earmuffs`,
   `bandana`, `wizardHat`).
3. **`listening` del original** convive con las nuestras (§4). Su motor de voz guarda y restaura
   `permanentEmote`, así que hay que probar la restauración con las emociones nuevas.

---

## 7. Licencia: qué podemos hacer con nuestro trabajo de Mochi

`LICENSE` (MIT) cubre el código. `LICENSE-ASSETS.md` reserva para Louis Raillé los nombres "Coucou" y
"Mochi", **el personaje de Mochi, "su diseño, aspecto, expresiones y animaciones como personaje"**, el
icono y los sonidos.

- **Se puede:** compilarlo para uno mismo, hacer fork y **contribuir con pull requests**.
- **No se puede sin permiso escrito:** publicar o distribuir una app, un fork o una obra derivada con el
  nombre, el icono, el personaje o los sonidos (App Store, releases de GitHub o cualquier otro sitio).

Consecuencia: nuestras emociones y atuendos son, en la práctica, **una contribución al original**. El
camino natural es un PR a `Louis-CFM/coucou`. El repo acepta muchos PRs externos (desde la 0.1.9, el CHANGELOG
cita a unos 30 colaboradores distintos). Para eso hay que estar sincronizados con su `main`: otro motivo para hacer el merge.
Si alguna vez se quisiera publicar el fork por separado, haría falta otro nombre, otro personaje, otro
icono y otros sonidos.

---

## 8. Plan propuesto

1. `git fetch` del original (§1) y `git merge upstream/main` en `ccr-ace9a467-157yax`.
2. Resolver los 3 bloques (§4) quedándose con ambos lados. Cambiar su hold de `.listening` a `holdMs`.
3. En un Mac: `xcodegen` + `xcodebuild` (ver CLAUDE.md). Arreglar lo que no compile; hay que contar con
   ello, porque ni nuestro código ni el merge se han compilado nunca.
4. Probar en el Mac: el armario con los 6 atuendos nuevos (girando y con `roll`), las 15 emociones
   (`triggerEmote` y como emoción permanente en los mini-Mochi), y la voz → restaurar la emoción.
5. Decidir si se activan las funciones opcionales (voz, Flow, Portfolio). Están apagadas por defecto.
6. Si se quiere contribuir: preparar un PR al original solo con el trabajo de Mochi, en commits limpios
   sobre su `main`, con capturas reales del Mac (no el mockup).
7. Más adelante, y por separado: portar atuendos y emociones a `windows/src/mochi/outfits.ts` y a
   `android/.../mochi/`.

---

## 9. Pendiente de verificar

- Que todo compila (Swift 6, `SWIFT_STRICT_CONCURRENCY`) después del merge.
- Que el rendimiento con la isla oculta sigue en 0 % de CPU con nuestras partículas nuevas.
- Dónde guarda el Portfolio las claves de exchanges (tiene que ser el Keychain).
- Las valoraciones de §2 y §5: son de lectura de código y CHANGELOG, no de uso real.
