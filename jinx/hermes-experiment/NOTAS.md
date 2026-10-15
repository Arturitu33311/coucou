# Motor emocional de Hermes: investigación y experimento

Servidor: `server:~/.hermes`. Motor vivo: `plugins/persona-engine/` (la copia en `hermes-agent/agent/` es
vieja y no se usa: no tiene `_last_event_log`). Datos de esta investigación: 2026-10-14, filas desde 2026-09-25.
Reproducible con `emotion_report.py`; la foto "antes" está en `antes-20261014.md/.json`.

## 1. El "reset por tiempo" NO toca los medidores

`~/.config/systemd/user/hermes-session-time-reset.timer` (cada 5 min) llama a
`~/.hermes/scripts/hermes_session_time_reset.py`, que solo hace `SessionDB.end_session(id, "session_reset")`
en `state.db` para sesiones de Telegram inactivas 60 min o anteriores a las 04:00 (`config.yaml: session_reset`).
No lee ni escribe `STATE.md`, ni `emotion_*`. Lo que sí cambia: la siguiente conversación abre una sesión nueva,
y por eso se reconstruye el prompt (y el bloque emocional de `SOUL.md`), ver §3.

Tabla del 14 oct (`antes-20261014.md` §3): el único `session_reset` fue a las 11:28 (52 mensajes). Los medidores
de las 10:19 y de las 16:24 son idénticos salvo el ruido de dos turnos: **no hay salto en el reset**.
Los únicos saltos grandes del log desde el 25 sep son de otro origen (§4).

## 2. Disparadores desde el 25 sep

`last_trigger_type` es **pegajoso**: se queda con el último evento escrito, aunque pasen horas de decaimiento
sin eventos. Por eso las filas "deterministic" (298) no son 298 eventos. Lo que cuenta es `last_event`:

| fuente | cantidad | qué aplica (deltas nominales, antes del suavizado α=0.35–0.55) |
|---|---|---|
| filas del log | 515 | — |
| eventos reales por turno (`last_event`) | 79 | — |
| `normal_interaction` | 76 | affection +0.5, boredom −0.5, patience +0.5, trust +1 (a veces energy +0.5) |
| `humor` (→ `juego`) | 2 | energy +8, affection +2, boredom −6, satisfaction +4 |
| `greeting` | 1 | affection +2, trust +1 |
| praise / gratitude / criticism / intimacy / achievement… | **0** | (tablas en `emotion_detector.py:620-758`) |
| presión de zynara_worry (cron, cada 4 h a :50) | 82 corridas | worry +7, energy +5, cada vez (`deterministic`) |

Tabla nominal por evento (`emotion_detector.py:620-758`, intensidad moderada): praise aff+10 trust+5 pride+7;
gratitude trust+10 aff+5 pride+9; achievement pride+10 sat+6; criticism pat−6 trust−3 sat−5 pride−5;
ignored pat−4 trust−4 ene−4 sat−4; other_ai_mentioned jealousy+14 poss+10 pat−4; humor ene+8 bor−6.

Cuántos turnos reales llegan: Telegram de Alan ≈162 mensajes reales desde el 25 sep (897 más son
`[[DISPARADOR…]]` automáticos); `api_server` (el chat de las apps) 39: **no llegan al hook** porque
`gateway/platforms/api_server.py` no emite `agent:start` (el emit está solo en `gateway/run_turn.py:2188`).
Los mensajes `[Home Assistant]` sí llegan y cuentan como charla de Alan (`normal_interaction`); el filtro de
sistema (`emotion_detector.py:895-908`) no los cubre.

**% de turnos con ánimo: 2/79 = 2.5 %.** El 96 % de los turnos son `normal_interaction` (+0.35 de efecto real
por mensaje tras el suavizado, con tope ±10 sobre la base: `emotion_state_manager.py:491-501`).

## 3. Qué ve el modelo y cómo se calcula

Camino: gateway `run_turn.py:2188` emite `agent:start` → hook `hooks/persona-engine-turn/handler.py`
(`handle`) llama `update_emotion_state` y reescribe el bloque `<!-- EMOTION_STATE_START -->…<!-- END -->`
de `~/.hermes/SOUL.md` con `get_tone_modifiers()` (`emotion_state_manager.py:710`). El plugin
(`plugins/persona-engine/__init__.py`, `on_session_start`) hace lo mismo al abrir sesión.

**Limite importante**: el prompt del sistema (SOUL.md incluido) se arma una vez por sesión y queda en
`agent._cached_system_prompt` (`agent/system_prompt.py:723`; lo dice el docstring del hook). Un evento a mitad
de conversación no cambia lo que el modelo ve hasta la **siguiente sesión**. El reset por tiempo (60 min de
inactividad / 04:00) hace que eso pase más seguido.

Texto exacto hoy (SOUL.md, 14 oct 18:10): cabecera `nivel emocional: moderate / negative (perfil mixto)`,
párrafos de marco/combinación/disparo/inercia/resaca/regulación casi todos genéricos ("oscilación leve",
"sale con naturalidad"), y **las 2–3 dimensiones de mayor desviación** con una frase cada una:

    - 【dimensión】worry｜negative｜moderate｜Preocupada de verdad: prioriza el estado de Alan sobre la tarea,
      pregunta directo qué pasó, baja el sarcasmo
    - 【dimensión】energy｜positive｜mild｜Un punto más despierta: respuestas con algo más de chispa…

`tone_head` del log: 395 filas "moderate", 116 "mild", 0 "intense" desde el 25 sep.

### Decaimiento (`decay_rate: 2.0` de STATE.md **no se usa**: comentario en `emotion_calculator.py:93`)

`apply_decay` (`emotion_calculator.py:287-333`), por dimensión:
`rate = _decay_rate(|valor−base|)` (`:276`, continuo: 0.015/h hasta 12 pts, 0.06/h de 18 a 45, 0.45/h desde 60) y
`factor = min(1.0, rate*horas)` (`:320`), `nuevo = valor + (base−valor)*factor`.
Los comentarios (`:63-65`) hablan de vidas medias (~46 h, ~11 h, ~1.2 h), que solo valen para decaimiento
exponencial; la fórmula es lineal y **se satura**: tras `1/rate` horas sin tocar el estado, el valor vuelve
entero a la base de un salto (con desviación ≥60: 2.2 h; 18–45: 16.7 h; <12: 67 h).
Prueba con la función real (solo lectura, desde el estado de las 18:00):

| horas | energy 82.4 | worry 30.6 | (energy 110 / affection 117 / trust 119) |
|---|---|---|---|
| 1 | 82.0 | 29.9 | 107.5 / 111.7 / 116.4 |
| 4 | 80.5 | 28.0 | 99.9 / 95.9 / 108.4 |
| 8 | 78.6 | 25.4 | 89.8 / 74.9 / 97.9 |
| 17 | — | — | **68.0 / 70.0 / 75.0** (vuelve a la base de golpe) |
| 24 | 70.9 | 15.0 | 68.0 / 70.0 / 75.0 |

Aparte, ausencia ≥12 h **sube** worry (`emotion_state_manager.py:311`: +4 +1.5/h, tope +20, techo 85).

## 4. Por qué "no se siente viva": hallazgos

1. **La regla de presión inmoviliza worry y energy.** `scripts/zynara_worry.py:185-199`: cada 4 h suma
   worry +7 / energy +5 si hay ≥2 `- [ ]` en `tasks.md`. `PRESION_DIAS = 3` está definido (`:96`) y **nunca se
   usa**: cuenta cualquier pendiente (hoy 6). Resultado: worry mediana 31 (base 15), energy 83 (base 68);
   así el prompt siempre dice "preocupada de verdad… baja el sarcasmo". Es el equilibrio entre ese empuje
   y el decaimiento. Esos eventos `deterministic` además pasan por α y momentum completos y actualizan la
   inercia (solo `normal_interaction` es "neutral", `:491`), lo que distorsiona los eventos reales.
2. **La charla normal no mueve nada**: +0.35 por mensaje y tope ±10; el 96 % de los turnos.
3. **Los eventos que sí moverían casi no ocurren**: 0 praise/gratitude/achievement/criticism en 3 semanas.
4. **El chat de las apps (api_server) no pasa por el motor**: no mueve medidores ni refresca el bloque.
5. **Mensajes de Home Assistant cuentan como charla de Alan.**
6. El saltón histórico (17 sep → 19 sep: affection 117 → 70.7, energy 114 → 71.8 en una fila) vino de la
   versión anterior con escalones y de la recalibración del 19 sep; ya no se reproduce con el código actual.

Rango desde el 25 sep (mínimo–máximo): affection 70–77, trust 76–85, patience 65–71, energy 72–86,
worry 18–35, boredom 12–20, resto prácticamente en su base: **todo el rango útil (0–120) está sin usar**
y el 93.6 % de las filas tienen alguna dimensión ≥10 fuera de su base (por la presión, no por conversación).
