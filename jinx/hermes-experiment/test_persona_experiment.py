"""Pruebas del experimento del motor emocional (persona_experiment + parches + disparador).

    python3 -m unittest test_persona_experiment -v                      # solo las puras
    PERSONA_ENGINE_HOME=~/.hermes python3 -m unittest ... -v           # + integración con el motor real

La integración copia el motor (y zynara_worry.py) de PERSONA_ENGINE_HOME a un directorio temporal, le aplica
los parches y trabaja con HERMES_HOME apuntando ahí: nunca toca el STATE.md ni el SOUL.md vivos.
"""
import importlib
import importlib.util
import json
import os
import shutil
import sys
import tempfile
import unittest
from datetime import date, datetime, timedelta
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import persona_experiment as px  # noqa: E402
import persona_experiment_tripwire as tw  # noqa: E402
import apply_patches  # noqa: E402

BL = dict(affection=70, trust=75, possessiveness=80, patience=65, energy=68, pride=45, jealousy=25, worry=15,
          satisfaction=58, boredom=20)


def _linear_rate(dev):  # la tasa continua de emotion_calculator._decay_rate
    pts = ((0, .015), (12, .015), (18, .06), (45, .06), (60, .45))
    for (x0, y0), (x1, y1) in zip(pts, pts[1:]):
        if dev <= x1:
            return y0 + (y1 - y0) * (dev - x0) / (x1 - x0)
    return .45


class Home:
    """HERMES_HOME temporal con el interruptor escrito."""

    def __init__(self, flag):
        self.dir = Path(tempfile.mkdtemp(prefix="px_"))
        self.old = os.environ.get("HERMES_HOME")
        os.environ["HERMES_HOME"] = str(self.dir)
        self.set(flag)

    def set(self, flag):
        p = self.dir / "persona_experiment.json"
        if flag is None:
            p.unlink(missing_ok=True)
        else:
            p.write_text(json.dumps(flag))
        px._cache.update(path=None, mtime=None)

    def close(self):
        if self.old is None:
            os.environ.pop("HERMES_HOME", None)
        else:
            os.environ["HERMES_HOME"] = self.old
        px._cache.update(path=None, mtime=None)
        shutil.rmtree(self.dir, ignore_errors=True)


class FlagTest(unittest.TestCase):
    def tearDown(self):
        self.h.close()

    def test_missing_or_broken_file_is_off(self):
        self.h = Home(None)
        self.assertFalse(px.on())
        (self.h.dir / "persona_experiment.json").write_text("{nope")
        px._cache.update(path=None, mtime=None)
        self.assertFalse(px.on())

    def test_switch_is_read_on_every_call(self):
        self.h = Home({"enabled": True})
        self.assertTrue(px.on("exp_decay"))
        os.utime(self.h.dir / "persona_experiment.json", (1, 1))  # mtime distinto sin reiniciar
        self.h.set({"enabled": False})
        self.assertFalse(px.on("exp_decay"))

    def test_feature_switch_inside_enabled(self):
        self.h = Home({"enabled": True, "exp_decay": False, "event_gain": {}})
        self.assertFalse(px.on("exp_decay"))
        self.assertTrue(px.on("skip_automated"))


class DecayTest(unittest.TestCase):
    def setUp(self):
        self.h = Home({"enabled": True})

    def tearDown(self):
        self.h.close()

    def test_off_returns_the_original_value(self):
        self.h.set({"enabled": False})
        self.assertEqual(px.decay_value(_linear_rate, 117, 70, 17, 70.0), 70.0)

    def test_does_not_snap_to_baseline(self):
        v = px.decay_value(_linear_rate, 117.0, 70.0, 17, 70.0)
        self.assertGreater(v, 70.0 + 0.5)  # lo original lo dejaba en 70.0 exacto
        self.assertLess(v, 117.0)

    def test_step_invariant(self):
        one = px.decay_value(_linear_rate, 110.0, 68.0, 10, 0)
        v = 110.0
        for _ in range(10):
            v = px.decay_value(_linear_rate, v, 68.0, 1, 0)
        self.assertAlmostEqual(one, v, delta=0.35)

    def test_monotone_and_never_overshoots(self):
        prev = 119.0
        for h in (0.5, 1, 2, 4, 8, 16, 32, 64, 128):
            v = px.decay_value(_linear_rate, 119.0, 75.0, h, 0)
            self.assertLessEqual(v, prev + 1e-9)
            self.assertGreaterEqual(v, 75.0)
            prev = v

    def test_below_baseline_rises(self):
        self.assertGreater(px.decay_value(_linear_rate, 10.0, 20.0, 5, 0), 10.0)


class FakeEvent:
    def __init__(self, trigger_type, deltas, confidence=.8, context="x"):
        self.trigger_type, self.deltas, self.confidence, self.context = trigger_type, deltas, confidence, context


class EventTest(unittest.TestCase):
    def setUp(self):
        self.h = Home({"enabled": True})

    def tearDown(self):
        self.h.close()

    def test_praise_is_amplified_within_caps(self):
        ev = FakeEvent("praise", {"affection": 10, "trust": 5, "pride": 7})
        neutral, out = px.shape_event(ev, False, dict(BL), BL)
        self.assertFalse(neutral)
        self.assertEqual(out.deltas["affection"], 12)  # 17.5 -> tope 12
        self.assertEqual(out.deltas["trust"], 8.75)
        self.assertEqual(ev.deltas["affection"], 10)  # el original no se muta

    def test_jealousy_event_is_not_amplified(self):
        ev = FakeEvent("other_ai_mentioned", {"jealousy": 14, "possessiveness": 10})
        self.assertIs(px.shape_event(ev, False, dict(BL), BL)[1], ev)

    def test_band_drops_pushes_beyond_it(self):
        st = dict(BL, affection=100)  # base+30
        out = px.shape_event(FakeEvent("praise", {"affection": 10, "trust": 5}), False, st, BL)[1]
        self.assertNotIn("affection", out.deltas)
        self.assertIn("trust", out.deltas)

    def test_negative_deltas_are_amplified_too(self):
        out = px.shape_event(FakeEvent("criticism", {"patience": -6, "trust": -3}), False, dict(BL), BL)[1]
        self.assertEqual(out.deltas["patience"], -9)

    def test_deterministic_becomes_neutral(self):
        ev = FakeEvent("deterministic", {"worry": 7, "energy": 5})
        neutral, out = px.shape_event(ev, False, dict(BL), BL)
        self.assertTrue(neutral)
        self.assertIs(out, ev)

    def test_off_changes_nothing(self):
        self.h.set({"enabled": False})
        ev = FakeEvent("praise", {"affection": 10})
        self.assertEqual(px.shape_event(ev, False, dict(BL), BL), (False, ev))
        self.assertFalse(px.is_automated([{"role": "user", "content": "[Home Assistant] Alan llegó"}]))

    def test_automated_messages(self):
        users = lambda t: [{"role": "assistant", "content": "hola"}, {"role": "user", "content": t}]  # noqa: E731
        self.assertTrue(px.is_automated(users("[Home Assistant] Alan (person.alan): changed")))
        self.assertTrue(px.is_automated(users([{"type": "text", "text": "  [BRIEFING_MATUTINO] 06:30"}])))
        self.assertFalse(px.is_automated(users("Hola Jinx, [Home Assistant] está raro")))
        self.assertFalse(px.is_automated([]))


class PresionTest(unittest.TestCase):
    TASKS = (
        "- [ ] [BIOLOGIA I] Práctica 5 — vence 2026-10-14 (HOY)  · Teams\n"
        "- [ ] [HISTORIA] 20. Maya — vence 2026-10-16 (en 2d)  · Teams\n"
        "- [ ] [QUIMICA] reporte — vence 2026-10-30 (en 16d)  · Teams\n"
        "- [ ] [FISICA] sin fecha\n"
        "- [x] [ARTE] hecho — vence 2026-10-14\n"
        "- [ ] [VIEJA] — vence 2026-09-01\n"
    )

    def setUp(self):
        self.h = Home({"enabled": True})

    def tearDown(self):
        self.h.close()

    def test_only_due_soon_count(self):
        got = px.urgent_tasks(self.TASKS, date(2026, 10, 14), 3)
        self.assertEqual(len(got), 2)
        self.assertTrue(all("BIOLOGIA" in g or "HISTORIA" in g for g in got))

    def test_off_returns_all_pending(self):
        self.h.set({"enabled": False})
        pend = ["- [ ]"] * 6
        self.assertEqual(px.presion_tasks(self.TASKS, pend, 3), pend)

    def test_once_per_budget_day(self):
        self.assertTrue(px.presion_claim("2026-10-14"))
        self.assertFalse(px.presion_claim("2026-10-14"))
        self.assertTrue(px.presion_claim("2026-10-15"))

    def test_dry_run_does_not_consume_the_day(self):
        self.assertTrue(px.presion_claim("2026-10-14", commit=False))
        self.assertTrue(px.presion_claim("2026-10-14", commit=False))
        self.assertTrue(px.presion_claim("2026-10-14"))
        self.assertFalse(px.presion_claim("2026-10-14", commit=False))

    def test_off_always_claims(self):
        self.h.set({"enabled": False})
        self.assertTrue(px.presion_claim("2026-10-14"))
        self.assertTrue(px.presion_claim("2026-10-14"))


class NuanceTest(unittest.TestCase):
    NOW = datetime(2026, 10, 14, 18, 0)

    def setUp(self):
        self.h = Home({"enabled": True})

    def tearDown(self):
        self.h.close()

    def test_calm_state_has_no_nuance(self):
        self.assertEqual(px.build_nuance(dict(BL), BL, 0, self.NOW), "")

    def test_good_mood(self):
        es = dict(BL, affection=77, trust=82, patience=70, energy=76)
        self.assertIn("buen humor", px.build_nuance(es, BL, 0, self.NOW))

    def test_worry_mentions_deadlines_only_when_there_are(self):
        es = dict(BL, worry=26)
        self.assertNotIn("entregas", px.build_nuance(es, BL, 0, self.NOW))
        self.assertIn("entregas de Alan", px.build_nuance(es, BL, 2, self.NOW))

    def test_recent_event_wins_and_expires(self):
        es = dict(BL, last_raw_trigger_type="criticism", last_update=(self.NOW - timedelta(minutes=30)).isoformat())
        self.assertIn("roce", px.build_nuance(es, BL, 0, self.NOW))
        es["last_update"] = (self.NOW - timedelta(hours=5)).isoformat()
        self.assertEqual(px.build_nuance(es, BL, 0, self.NOW), "")

    def test_never_more_than_three_phrases(self):
        es = dict(BL, energy=50, worry=30, boredom=32, pride=56, last_raw_trigger_type="praise",
                  last_update=self.NOW.isoformat())
        self.assertLessEqual(px.build_nuance(es, BL, 1, self.NOW).count(";"), 2)

    def test_add_nuance_is_inserted_inside_the_block_and_off_is_intact(self):
        block = "\n<emotion_modifier>\nx\n</emotion_modifier>\n"
        es = dict(BL, worry=27)
        out = px.add_nuance(block, es, BL)
        self.assertIn("【matiz del día】", out)
        self.assertTrue(out.rstrip().endswith("</emotion_modifier>"))
        self.h.set({"enabled": False})
        self.assertEqual(px.add_nuance(block, es, BL), block)


class TripwireTest(unittest.TestCase):
    CFG = {"tripwire": {"band": 40, "negative_events_6h": 4, "jealousy_max": 60}}
    NOW = datetime(2026, 10, 14, 18, 0)

    def test_calm_state_does_not_trip(self):
        h = Home({"enabled": True})
        try:
            self.assertIsNone(tw.check(dict(BL, worry=31, energy=83), self.CFG, self.NOW))
        finally:
            h.close()

    def test_band_and_jealousy(self):
        h = Home({"enabled": True})
        try:
            self.assertIn("energy", tw.check(dict(BL, energy=110), self.CFG, self.NOW))
            self.assertIn("jealousy", tw.check(dict(BL, jealousy=61), self.CFG, self.NOW))
        finally:
            h.close()

    def test_negative_burst(self):
        h = Home({"enabled": True})
        try:
            logs = h.dir / "logs"
            logs.mkdir()
            rows = [{"timestamp": (self.NOW - timedelta(minutes=10 * i)).isoformat(),
                     "last_event": {"trigger": "criticism"}} for i in range(4)]
            (logs / "emotion_turn_log.jsonl").write_text("\n".join(json.dumps(r) for r in rows))
            tw.HOME = h.dir
            self.assertIn("negativos", tw.check(dict(BL), self.CFG, self.NOW))
        finally:
            h.close()


ENGINE = os.environ.get("PERSONA_ENGINE_HOME")


@unittest.skipUnless(ENGINE and (Path(ENGINE).expanduser() / "plugins/persona-engine/emotion_calculator.py").exists(),
                     "PERSONA_ENGINE_HOME no apunta a un ~/.hermes con persona-engine")
class EngineTest(unittest.TestCase):
    """El motor real, parcheado, sobre una copia temporal."""

    @classmethod
    def setUpClass(cls):
        src = Path(ENGINE).expanduser()
        cls.tmp = Path(tempfile.mkdtemp(prefix="pxe_"))
        shutil.copytree(src / "plugins/persona-engine", cls.tmp / "plugins/persona-engine",
                        ignore=shutil.ignore_patterns("__pycache__", ".pytest_cache", "*.bak*"))
        (cls.tmp / "scripts").mkdir()
        shutil.copy(src / "scripts/zynara_worry.py", cls.tmp / "scripts/zynara_worry.py")
        (cls.tmp / "SOUL.md").write_text("nombre: Jinx\n<!-- EMOTION_STATE_START -->\n<!-- EMOTION_STATE_END -->\n")
        apply_patches.main(["--home", str(cls.tmp)])
        shutil.copy(HERE / "persona_experiment.py", cls.tmp / "plugins/persona-engine/persona_experiment.py")
        cls.pe = str(cls.tmp / "plugins/persona-engine")
        sys.path.insert(0, cls.pe)
        cls.env_old = os.environ.get("HERMES_HOME")
        os.environ["HERMES_HOME"] = str(cls.tmp)
        for m in ("emotion_calculator", "emotion_detector", "emotion_state_manager", "persona_experiment"):
            sys.modules.pop(m, None)
        cls.calc = importlib.import_module("emotion_calculator")
        cls.mgr = importlib.import_module("emotion_state_manager")
        cls.pxe = importlib.import_module("persona_experiment")

    @classmethod
    def tearDownClass(cls):
        sys.path.remove(cls.pe)
        if cls.env_old is None:
            os.environ.pop("HERMES_HOME", None)
        else:
            os.environ["HERMES_HOME"] = cls.env_old
        for m in ("emotion_calculator", "emotion_detector", "emotion_state_manager", "persona_experiment"):
            sys.modules.pop(m, None)
        shutil.rmtree(cls.tmp, ignore_errors=True)

    def flag(self, d):
        (self.tmp / "persona_experiment.json").write_text(json.dumps(d))
        self.pxe._cache.update(path=None, mtime=None)

    def write_state(self, **over):
        st = dict(BL, **over)
        lines = ["---", "emotion_state:"] + [f"  {k}: {v}" for k, v in st.items()]
        lines += [f"  last_update: '{datetime.now().isoformat()}'", "  last_trigger_type: normal_interaction",
                  "---", "", "## Estado", ""]
        (self.tmp / "STATE.md").write_text("\n".join(lines))

    def manager(self):
        return self.mgr.EmotionStateManager(hermes_home=self.tmp)

    def read(self):
        return self.manager().get_current_emotion_state()

    def test_patched_files_import_and_flag_off_is_the_original_decay(self):
        self.flag({"enabled": False})
        c = self.calc.EmotionCalculator()
        t0 = datetime(2026, 10, 14, 18, 0)
        out = c.apply_decay(dict(BL, energy=110.0, affection=117.0), t0, t0 + timedelta(hours=17))
        self.assertEqual(out["energy"], 68.0)      # el salto de golpe de siempre
        self.assertEqual(out["affection"], 70.0)

    def test_flag_on_does_not_snap(self):
        self.flag({"enabled": True})
        c = self.calc.EmotionCalculator()
        t0 = datetime(2026, 10, 14, 18, 0)
        out = c.apply_decay(dict(BL, energy=110.0, affection=117.0), t0, t0 + timedelta(hours=17))
        self.assertGreater(out["energy"], 69.0)
        self.assertGreater(out["affection"], 71.0)

    def test_deterministic_event_cannot_pump_past_neutral_cap(self):
        self.flag({"enabled": True})
        self.write_state(worry=26.0, energy=79.0)
        m = self.manager()
        ev = self.mgr.EmotionEvent(trigger_type="deterministic", deltas={"worry": 7, "energy": 5},
                                   confidence=.6, context="señales deterministas")
        self.assertTrue(m.update_emotion_state(messages=[], force_event=ev))
        st = self.read()
        self.assertLessEqual(st["worry"], 26.0 + 0.05)   # tope: base 15 + 10 = 25 -> sin delta
        self.assertLessEqual(st["energy"], 79.0 + 0.05)

    def test_flag_off_deterministic_event_still_pumps(self):
        self.flag({"enabled": False})
        self.write_state(worry=26.0, energy=79.0)
        m = self.manager()
        ev = self.mgr.EmotionEvent(trigger_type="deterministic", deltas={"worry": 7, "energy": 5},
                                   confidence=.6, context="señales deterministas")
        m.update_emotion_state(messages=[], force_event=ev)
        self.assertGreater(self.read()["worry"], 27.5)

    def test_praise_moves_more_with_the_experiment(self):
        results = {}
        for name, flag in (("off", {"enabled": False}), ("on", {"enabled": True})):
            self.flag(flag)
            self.write_state()
            m = self.manager()
            ev = self.mgr.EmotionEvent(trigger_type="praise", deltas={"affection": 10, "trust": 5, "pride": 7},
                                       confidence=.85, context="muy bien Jinx, gracias")
            m.update_emotion_state(messages=[], force_event=ev)
            results[name] = self.read()
        self.assertGreater(results["on"]["affection"] - 70, results["off"]["affection"] - 70)
        self.assertLess(results["on"]["affection"], 70 + 12)  # sin dispararse

    def test_home_assistant_message_is_not_an_event(self):
        self.flag({"enabled": True})
        self.write_state()
        m = self.manager()
        m.update_emotion_state(messages=[{"role": "user", "content": "[Home Assistant] Alan (person.alan): changed from 'Escuela' to 'home'"}])
        self.assertIsNone(getattr(m, "_last_event_log", None))

    def test_tone_block_gets_the_nuance_line_only_when_on(self):
        self.write_state(affection=77.0, trust=83.0, patience=70.0, energy=78.0, worry=27.0)
        self.flag({"enabled": False})
        off = self.manager().get_tone_modifiers()
        self.flag({"enabled": True})
        on = self.manager().get_tone_modifiers()
        self.assertNotIn("matiz del día", off)
        self.assertIn("matiz del día", on)
        self.assertTrue(on.rstrip().endswith("</emotion_modifier>"))

    def test_zynara_worry_counts_only_due_tasks(self):
        self.flag({"enabled": True})
        (self.tmp / "tasks.md").write_text(PresionTest.TASKS.replace("2026-10-14", date.today().isoformat())
                                           .replace("2026-10-16", (date.today() + timedelta(days=2)).isoformat()))
        spec = importlib.util.spec_from_file_location("zw_test", self.tmp / "scripts/zynara_worry.py")
        zw = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(zw)
        zw.HERMES_HOME = self.tmp
        d1 = zw.senales_deterministas()
        self.assertEqual(d1.get("worry"), 7)
        d2 = zw.senales_deterministas()      # misma jornada de presupuesto: ya no
        self.assertNotIn("worry", d2)


class PatchApplicationTest(unittest.TestCase):
    def test_refuses_when_the_engine_changed(self):
        d = Path(tempfile.mkdtemp())
        try:
            for rel, _ in apply_patches.TARGETS:
                p = d / rel
                p.parent.mkdir(parents=True, exist_ok=True)
                p.write_text("# otro motor\n")
            with self.assertRaises(SystemExit):
                apply_patches.plan(d)
        finally:
            shutil.rmtree(d, ignore_errors=True)


if __name__ == "__main__":
    unittest.main()
