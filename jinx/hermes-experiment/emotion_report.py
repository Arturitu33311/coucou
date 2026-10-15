#!/usr/bin/env python3
"""emotion_report — medidas del motor emocional de Hermes (persona-engine), antes y después del experimento.

Corre en el servidor (usa ~/.hermes por defecto) o en cualquier sitio con copias de los archivos:

    python3 emotion_report.py                         # informe completo, en el servidor
    python3 emotion_report.py --since 2026-10-14 --json-out despues.json
    python3 emotion_report.py --log etl.jsonl --zlog zynara_worry.log --db state.db --day 2026-10-14

Qué mide (todo sale de `logs/emotion_turn_log.jsonl`, `logs/zynara_worry.log` y `state.db`, solo lectura):
  1. Filas del log por `last_trigger_type` Y eventos reales (`last_event`) + corridas de presión de zynara_worry.
     `last_trigger_type` es pegajoso: una fila "deterministic" no es un evento nuevo, es el último escrito.
  2. Rango de cada dimensión (min / p10 / mediana / p90 / max) frente a su línea base.
  3. Tabla hora a hora de un día, con el último valor conocido arrastrado, marcando los fines de sesión
     (`session_reset` en state.db) y las corridas de presión (:50), y cualquier salto >= 8 entre horas.
  4. % de turnos reales con ánimo (evento no neutro) y reparto de la intensidad que ve el modelo (`tone_head`).
  5. % del tiempo diurno (06-22) y nocturno en que Mochi tendría cara, con `jinx_baseline` (si está al lado).
"""
from __future__ import annotations

import argparse
import collections
import datetime as dt
import json
import os
import sqlite3
import statistics
import sys

DIMS = ["affection", "trust", "possessiveness", "patience", "energy", "pride", "jealousy", "worry",
        "satisfaction", "boredom"]
BASELINES = dict(affection=70, trust=75, possessiveness=80, patience=65, energy=68, pride=45, jealousy=25,
                 worry=15, satisfaction=58, boredom=20)
NEUTRAL_TRIGGERS = {"normal_interaction", "greeting", "deterministic", "decay"}
SHORT = dict(affection="aff", trust="tru", possessiveness="pos", patience="pat", energy="ene", pride="pri",
             jealousy="jea", worry="wor", satisfaction="sat", boredom="bor")
HOME = os.environ.get("HERMES_HOME", os.path.expanduser("~/.hermes"))


def load_rows(path):
    rows = []
    for p in (path + ".1", path):  # el log rota a .jsonl.1 a los 2 MB
        if not os.path.exists(p):
            continue
        for ln in open(p, encoding="utf-8"):
            try:
                r = json.loads(ln)
                r["t"] = dt.datetime.fromisoformat(r["timestamp"])
                rows.append(r)
            except (ValueError, KeyError):
                continue
    rows.sort(key=lambda r: r["t"])
    return rows


def load_zlog(path):
    out = []
    if not path or not os.path.exists(path):
        return out
    for ln in open(path, encoding="utf-8", errors="replace"):
        ln = ln.strip()
        if not ln.startswith("{"):
            continue
        try:
            out.append(json.loads(ln))
        except ValueError:
            pass
    return out


def session_resets(db):
    if not db or not os.path.exists(db):
        return []
    c = sqlite3.connect(f"file:{db}?mode=ro", uri=True, timeout=10)
    try:
        q = ("select ended_at, source, message_count from sessions where end_reason='session_reset' "
             "order by ended_at")
        return [(dt.datetime.fromtimestamp(a), s, n) for a, s, n in c.execute(q)]
    finally:
        c.close()


def pct(sorted_v, p):
    return sorted_v[min(len(sorted_v) - 1, int(len(sorted_v) * p))]


def section_triggers(rows, zlog, since):
    sub = [r for r in rows if r["timestamp"] >= since]
    by_state = collections.Counter(r["dimensions"].get("last_trigger_type") for r in sub)
    events = [r for r in sub if r.get("last_event")]
    by_event = collections.Counter(r["last_event"]["trigger"] for r in events)
    deltas = collections.defaultdict(collections.Counter)
    for r in events:
        deltas[r["last_event"]["trigger"]][json.dumps(r["last_event"].get("deltas"), sort_keys=True)] += 1
    presion = [z for z in zlog if z.get("fuente") == "presion" and z.get("ts", "") >= since]
    applied = [z for z in zlog if z.get("deltas_extra_ok") and z.get("ts", "") >= since]
    return {
        "rows": len(sub), "by_last_trigger_type": dict(by_state.most_common()),
        "real_events": len(events), "by_event_trigger": dict(by_event.most_common()),
        "deltas_by_trigger": {t: c.most_common(3) for t, c in deltas.items()},
        "zynara_presion_signals": len(presion), "zynara_deltas_applied_runs": len(applied),
        "zynara_last_deltas": applied[-1].get("deltas_extra") if applied else None,
    }


def section_range(rows, since):
    sub = [r for r in rows if r["timestamp"] >= since]
    out = {}
    for d in DIMS:
        v = sorted(r["dimensions"][d] for r in sub if d in r["dimensions"])
        if v:
            out[d] = dict(min=v[0], p10=pct(v, .1), median=statistics.median(v), p90=pct(v, .9), max=v[-1],
                          baseline=BASELINES[d], median_offset=round(statistics.median(v) - BASELINES[d], 1))
    return out


def section_day(rows, day, resets, zlog):
    d0 = dt.datetime.fromisoformat(day)
    before = [r for r in rows if r["t"] < d0]
    last = before[-1]["dimensions"] if before else None
    today = [r for r in rows if r["t"].date() == d0.date()]
    table, prev = [], last
    for h in range(24):
        rs = [r for r in today if r["t"].hour == h]
        if rs:
            last = rs[-1]["dimensions"]
        mark = []
        for t, src, n in resets:
            if t.date() == d0.date() and t.hour == h:
                mark.append(f"session_reset {t:%H:%M} ({src}, {n} msgs)")
        runs = [z for z in zlog if z.get("deltas_extra_ok") and z.get("ts", "").startswith(f"{day}T{h:02d}:")]
        if runs:
            mark.append("presión " + ",".join(z["ts"][11:16] for z in runs)
                        + " " + json.dumps(runs[-1].get("deltas_extra")))
        jumps = []
        if last and prev:
            jumps = [f"{SHORT[d]} {prev[d]:.0f}->{last[d]:.0f}" for d in DIMS if abs(last[d] - prev[d]) >= 8]
        table.append({"hour": h, "rows": len(rs), "values": {d: last[d] for d in DIMS} if last else None,
                      "marks": mark, "jumps_ge_8": jumps})
        prev = last
    return table


def section_turns(rows, since):
    sub = [r for r in rows if r["timestamp"] >= since]
    events = [r for r in sub if r.get("last_event")]
    emotional = [r for r in events if r["last_event"]["trigger"] not in NEUTRAL_TRIGGERS]
    heads = collections.Counter()
    for r in sub:
        h = (r.get("tone_head") or "").split("/")[0].strip() or "(vacío)"
        heads[h] += 1
    off = [r for r in sub if any(abs(r["dimensions"][d] - BASELINES[d]) >= 10 for d in DIMS if d in r["dimensions"])]
    return {
        "real_event_turns": len(events), "emotional_turns": len(emotional),
        "pct_turns_with_mood": round(100 * len(emotional) / len(events), 1) if events else None,
        "tone_intensity_rows": dict(heads.most_common()),
        "pct_rows_any_dim_10_off_baseline": round(100 * len(off) / len(sub), 1) if sub else None,
    }


def section_mochi(rows, since):
    """% del tiempo con cara: estado arrastrado cada 5 min; día = 06-22, noche = el resto."""
    here = os.path.dirname(os.path.abspath(__file__))
    for cand in (os.path.join(here, "..", "jinx-assist"), here, os.path.join(HOME, "vga_panel")):
        if os.path.exists(os.path.join(cand, "jinx_baseline.py")):
            sys.path.insert(0, cand)
            break
    try:
        import jinx_baseline as jb
    except ImportError:
        return {"error": "jinx_baseline.py no encontrado"}
    sub = [r for r in rows if r["timestamp"] >= since]
    if not sub:
        return {"error": "sin filas"}
    cnt = {"day": collections.Counter(), "night": collections.Counter()}
    i, t, end = 0, sub[0]["t"], sub[-1]["t"]
    cur, ev_hist = None, []
    latch = None
    step = dt.timedelta(minutes=5)
    while t <= end:
        while i < len(sub) and sub[i]["t"] <= t:
            cur = sub[i]["dimensions"]
            le = sub[i].get("last_event")
            if le:
                ev_hist.append({"t": sub[i]["t"].timestamp(), "trigger": le["trigger"],
                                "deltas": le.get("deltas") or {}})
                ev_hist = ev_hist[-200:]
            i += 1
        bucket = "day" if 6 <= t.hour < 22 else "night"
        if cur is not None:
            if hasattr(jb, "mood_now"):  # versión con eventos que decaen y histéresis
                face, latch = jb.mood_now(cur, ev_hist, t.timestamp(), latch, t.hour)
            else:
                face = jb.baseline_emote(cur, t.hour)
            cnt[bucket]["face" if face else "calm"] += 1
            cnt[bucket][f"emote:{face}"] += 1
        t += step
    out = {}
    for b, c in cnt.items():
        tot = c["face"] + c["calm"]
        out[b] = {"samples_5min": tot, "pct_with_face": round(100 * c["face"] / tot, 1) if tot else None,
                  "emotes": {k[6:]: v for k, v in c.items() if k.startswith("emote:")}}
    out["function"] = "mood_now" if hasattr(jb, "mood_now") else "baseline_emote (legacy)"
    return out


def render_md(rep):
    L = [f"# Informe del motor emocional — {rep['generated']}", ""]
    t = rep["triggers"]
    L += [f"## 1. Disparadores desde {rep['since']}", "",
          f"Filas del log: **{t['rows']}**. Eventos reales (`last_event`): **{t['real_events']}**. "
          f"Señales de presión de zynara_worry: **{t['zynara_presion_signals']}** "
          f"(corridas con deltas aplicados: {t['zynara_deltas_applied_runs']}, últimos deltas {t['zynara_last_deltas']}).", "",
          "| `last_trigger_type` (estado pegajoso) | filas |", "|---|---|"]
    L += [f"| {k} | {v} |" for k, v in t["by_last_trigger_type"].items()]
    L += ["", "| evento real (`last_event.trigger`) | veces | deltas más comunes |", "|---|---|---|"]
    for k, v in t["by_event_trigger"].items():
        L.append(f"| {k} | {v} | `{t['deltas_by_trigger'][k][0][0]}` |")
    L += ["", "## 2. Rango por dimensión", "", "| dim | min | p10 | mediana | p90 | max | base | mediana−base |",
          "|---|---|---|---|---|---|---|---|"]
    for d, s in rep["range"].items():
        L.append(f"| {d} | {s['min']:.1f} | {s['p10']:.1f} | {s['median']:.1f} | {s['p90']:.1f} | {s['max']:.1f} "
                 f"| {s['baseline']} | {s['median_offset']:+.1f} |")
    L += ["", f"## 3. Día {rep['day']} hora a hora (último valor conocido)", "",
          "El log solo escribe cuando hay un turno o inicio de sesión: las corridas de cron (presión) cambian "
          "STATE.md sin dejar fila, así que su efecto aparece en la fila siguiente. Un valor arrastrado no es "
          "una medición en esa hora.", "",
          "| h | filas | " + " | ".join(SHORT[d] for d in DIMS) + " | marcas |", "|---|---|" + "---|" * len(DIMS) + "---|"]
    for r in rep["day_table"]:
        vals = " | ".join(f"{r['values'][d]:.1f}" for d in DIMS) if r["values"] else " | ".join("-" for _ in DIMS)
        marks = "; ".join(r["marks"] + (["SALTO " + ", ".join(r["jumps_ge_8"])] if r["jumps_ge_8"] else []))
        L.append(f"| {r['hour']:02d} | {r['rows']} | {vals} | {marks} |")
    L.append(f"| base | | " + " | ".join(str(BASELINES[d]) for d in DIMS) + " | |")
    u = rep["turns"]
    L += ["", "## 4. Turnos con ánimo", "",
          f"- Turnos reales que llegaron al motor: **{u['real_event_turns']}**; con evento emocional (no neutro): "
          f"**{u['emotional_turns']}** → **{u['pct_turns_with_mood']} %**.",
          f"- Filas con alguna dimensión a ≥10 de su base: **{u['pct_rows_any_dim_10_off_baseline']} %**.",
          f"- Intensidad que ve el modelo (`tone_head`): {u['tone_intensity_rows']}"]
    m = rep.get("mochi", {})
    L += ["", "## 5. Mochi con cara", ""]
    if "error" in m:
        L.append(f"No calculado: {m['error']}")
    else:
        L.append(f"Función: `{m['function']}`")
        for b in ("day", "night"):
            L.append(f"- {'Día 06–22' if b == 'day' else 'Noche 22–06'}: **{m[b]['pct_with_face']} %** con cara "
                     f"({m[b]['samples_5min']} muestras de 5 min) {m[b]['emotes']}")
    return "\n".join(L) + "\n"


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--log", default=os.path.join(HOME, "logs", "emotion_turn_log.jsonl"))
    ap.add_argument("--zlog", default=os.path.join(HOME, "logs", "zynara_worry.log"))
    ap.add_argument("--db", default=os.path.join(HOME, "state.db"))
    ap.add_argument("--since", default="2026-09-25")
    ap.add_argument("--day", default=None, help="día de la tabla horaria (por defecto, hoy)")
    ap.add_argument("--json-out")
    ap.add_argument("--md-out")
    a = ap.parse_args(argv)
    rows = load_rows(a.log)
    zlog = load_zlog(a.zlog)
    day = a.day or dt.date.today().isoformat()
    rep = {
        "generated": dt.datetime.now().isoformat(timespec="minutes"), "since": a.since, "day": day,
        "triggers": section_triggers(rows, zlog, a.since), "range": section_range(rows, a.since),
        "day_table": section_day(rows, day, session_resets(a.db), zlog), "turns": section_turns(rows, a.since),
        "mochi": section_mochi(rows, a.since),
    }
    md = render_md(rep)
    if a.json_out:
        json.dump(rep, open(a.json_out, "w"), indent=1, ensure_ascii=False, default=str)
    if a.md_out:
        open(a.md_out, "w", encoding="utf-8").write(md)
    print(md)
    return 0


if __name__ == "__main__":
    sys.exit(main())
