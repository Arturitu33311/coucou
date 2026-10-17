"""Ánimo de Jinx en el gateway: baseline_emote, la etiqueta oculta [[mood:…]], op: mood y el parseo de STATE.md."""
import json
import os
import sys
import tempfile
import time
import unittest

sys.path.insert(0, os.path.dirname(__file__))
import jinx_baseline
import jinx_chat
from jinx_chat import Deps, handle
from jinx_thread import Thread
from test_jinx_chat import FakeBrain, FakeMirror

# El frontmatter real de ~/.hermes/STATE.md (2026-10-14), con sus bloques anidados.
REAL_STATE = """---
emotion_state:
  affection: 70.6
  trust: 76.18
  possessiveness: 80.03
  patience: 65.6
  energy: 83.33
  pride: 45.03
  jealousy: 25.02
  worry: 31.86
  satisfaction: 58.08
  boredom: 19.36
  emotion_score: 0.076
  current_emotion: 0.076
  last_trigger_type: deterministic
  last_update: '2026-10-14T12:50:03.574311'
  baselines:
    affection: 70
    patience: 65
  decay_rate: 2.0
  inertia:
    consecutive_same: 1
    history:
    - 1
    - 1
  ops_streak: 0
---

## Estado emocional actual — Jinx
Afecto: 70.6/120 (tibia)
"""


def _st(**kw):
    base = {"affection": 70, "trust": 75, "patience": 65, "energy": 68}
    base.update(kw)
    return base


class StateParseTest(unittest.TestCase):
    def test_reads_the_flat_dimensions_and_skips_nested_blocks(self):
        s = jinx_chat.parse_emotion_state(REAL_STATE)
        self.assertEqual(s["affection"], 70.6)  # not baselines.affection (70)
        self.assertEqual(s["patience"], 65.6)
        self.assertEqual(s["energy"], 83.33)
        self.assertEqual(s["last_update"], "2026-10-14T12:50:03.574311")
        self.assertEqual(s["last_trigger_type"], "deterministic")
        self.assertNotIn("consecutive_same", s)
        self.assertNotIn("history", s)

    def test_garbage_is_none(self):
        for md in ("", None, "sin frontmatter", "---\nfoo: 1\n---\n",
                   "---\nemotion_state:\n  baselines:\n    a: 1\n---"):
            self.assertIsNone(jinx_chat.parse_emotion_state(md))

    def test_ssh_reader_reads_through_a_bounded_ssh_and_raises_without_state(self):
        calls = []

        class P:
            def __init__(self, out):
                self.stdout = out

        def ok(cmd, **kw):
            calls.append((cmd, kw))
            return P(REAL_STATE)

        s = jinx_chat.ssh_state_reader(runner=ok)
        self.assertEqual(s["trust"], 76.18)
        self.assertEqual(calls[0][0][0], "ssh")
        self.assertIn("BatchMode=yes", calls[0][0])
        self.assertEqual(calls[0][1]["timeout"], 12)
        with self.assertRaises(RuntimeError):
            jinx_chat.ssh_state_reader(runner=lambda cmd, **kw: P("nada"))


class HermesWhereTest(unittest.TestCase):
    def test_over_ssh_from_the_laptop_and_as_is_on_the_server(self):
        cmd = "/home/alan/.hermes/hermes-agent/venv/bin/python /home/alan/.hermes/scripts/jinx_hermes_chat.py"
        self.assertEqual(jinx_chat.hermes_argv(cmd, "server")[:4], ["ssh", "-o", "BatchMode=yes", "-o"])
        self.assertEqual(jinx_chat.hermes_argv(cmd, "server")[-2:], ["server", cmd])
        for local in ("local", "", "  "):
            self.assertEqual(jinx_chat.hermes_argv(cmd, local), cmd.split())
        self.assertEqual(jinx_chat.hermes_argv("cat /home/alan/.hermes/STATE.md", "local"), ["cat", "/home/alan/.hermes/STATE.md"])

    def test_on_the_server_hermes_is_found_here_on_the_laptop_over_ssh(self):
        cmd = "/srv/venv/bin/python /srv/scripts/jinx_hermes_chat.py"
        self.assertEqual(jinx_chat.default_hermes_host(cmd, exists=lambda p: True), "local")
        self.assertEqual(jinx_chat.default_hermes_host(cmd, exists=lambda p: p.endswith("python")), "server")
        self.assertEqual(jinx_chat.default_hermes_host(cmd, exists=lambda p: False), "server")


NOW = 1_760_000_000.0  # un instante cualquiera (epoch)
NOON = 14              # hora local del mediodía


def ev(trigger, minutes_ago=0, **deltas):
    return {"t": NOW - minutes_ago * 60, "trigger": trigger, "deltas": deltas}


class BaselineEmoteTest(unittest.TestCase):
    """El ánimo de fondo SOLO con estado (sin eventos): calidez con histéresis, noche, extremos."""

    def b(self, hour=NOON, **kw):
        return jinx_chat.baseline_emote(_st(**kw), hour)

    def test_a_normal_day_has_no_background_mood(self):
        self.assertIsNone(self.b())
        self.assertIsNone(jinx_chat.baseline_emote(jinx_chat.parse_emotion_state(REAL_STATE), 15))

    def test_warmth_enters_at_74_5(self):
        # warmth = mean(affection, trust, patience)
        self.assertEqual(self.b(affection=77, trust=76, patience=70.5), "happy")      # 74.5
        self.assertIsNone(self.b(affection=77, trust=76, patience=70.4))              # 74.47

    def test_no_negative_threshold_rules_remain(self):
        # Ninguna de estas llegó nunca en 3 semanas de datos; ya no existen como reglas de estado.
        for kw in ({"patience": 5}, {"trust": 5}, {"jealousy": 100}, {"worry": 100}, {"boredom": 100},
                   {"emotion_score": -4}, {"energy": 5}, {"affection": 30, "trust": 30, "patience": 30}):
            self.assertIsNone(self.b(**kw), kw)

    def test_night_window_edges(self):
        for h in (22, 23, 0, 5):
            self.assertEqual(self.b(hour=h), "sleepy", h)
        for h in (6, 12, 21):
            self.assertIsNone(self.b(hour=h), h)

    def test_night_beats_background_happiness(self):
        self.assertEqual(self.b(hour=2, affection=90, trust=90, patience=90), "sleepy")

    def test_extreme_states_still_show(self):
        self.assertEqual(self.b(affection=95), "love")
        self.assertEqual(self.b(pride=85), "proud")
        self.assertEqual(self.b(energy=100, emotion_score=1), "excited")
        self.assertIsNone(self.b(energy=100, emotion_score=0.9))
        self.assertEqual(self.b(emotion_score=2), "happy")
        self.assertEqual(self.b(satisfaction=90), "happy")
        self.assertIsNone(self.b(satisfaction=89.9))

    def test_missing_or_bad_dimensions_skip_their_rule(self):
        self.assertIsNone(jinx_chat.baseline_emote({}, 14))
        self.assertIsNone(jinx_chat.baseline_emote({"affection": 90}, 14))  # no warmth without trust+patience
        self.assertEqual(jinx_chat.baseline_emote({"affection": 120}, 14), "love")  # love needs only affection
        self.assertIsNone(jinx_chat.baseline_emote({"patience": "x", "trust": True}, 14))
        self.assertIsNone(jinx_chat.baseline_emote(None, 14))
        self.assertEqual(jinx_chat.baseline_emote(None, 3), "sleepy")
        self.assertEqual(jinx_chat.baseline_emote({"energy": 10}, 23), "sleepy")

    def test_only_valid_emotes_come_out(self):
        for kw in ({}, {"patience": 1}, {"energy": 1}, {"affection": 120, "trust": 120, "patience": 120}):
            for h in (0, 12):
                e = jinx_chat.baseline_emote(_st(**kw), h)
                self.assertTrue(e is None or e in jinx_chat.EMOTES, e)
        for t, e in jinx_baseline.EVENT_EMOTE.items():
            self.assertIn(e, jinx_chat.EMOTES, t)

    def test_the_emote_list_is_mochis(self):
        self.assertEqual(len(jinx_chat.EMOTES), 19)
        self.assertEqual(set(jinx_chat.EMOTES), {
            "happy", "love", "proud", "surprised", "excited", "confused", "shy", "sad", "curious", "bored",
            "scared", "grumpy", "smug", "laughing", "crying", "relieved", "determined", "flustered", "sleepy"})


class WarmthHysteresisTest(unittest.TestCase):
    def run_seq(self, warmths, hour=NOON):
        """Cada valor de calidez es affection=trust=patience=w; devuelve la cara en cada paso."""
        latch, out = {}, []
        for w in warmths:
            e, latch = jinx_baseline.mood_now({"affection": w, "trust": w, "patience": w}, [], NOW, latch, hour)
            out.append(e)
        return out

    def test_enters_at_74_5_and_leaves_below_72_5(self):
        self.assertEqual(self.run_seq([74.0, 74.5, 73.5, 72.5, 72.4, 73.5]),
                         [None, "happy", "happy", "happy", None, None])

    def test_does_not_flicker_around_the_entry_threshold(self):
        seq = [74.6, 74.4, 74.6, 74.4, 74.6, 74.4]
        self.assertEqual(self.run_seq(seq), ["happy"] * 6)

    def test_latch_survives_a_night_and_an_event(self):
        latch = {}
        _, latch = jinx_baseline.mood_now({"affection": 76, "trust": 76, "patience": 76}, [], NOW, latch, NOON)
        e, latch = jinx_baseline.mood_now({"affection": 73, "trust": 73, "patience": 73},
                                          [ev("praise", 1, affection=10, trust=5, pride=7)], NOW, latch, 2)
        self.assertEqual(e, "proud")
        self.assertTrue(latch["happy"])  # 73 > 72.5: sigue contenta cuando amanezca

    def test_the_callers_latch_is_not_mutated(self):
        latch = {"happy": True}
        jinx_baseline.mood_now({"affection": 60, "trust": 60, "patience": 60}, [], NOW, latch, NOON)
        self.assertEqual(latch, {"happy": True})


class EventMoodTest(unittest.TestCase):
    PRAISE = dict(affection=10, trust=5, possessiveness=2, pride=7, energy=4, satisfaction=4)  # |d| = 32 -> 1.0
    HUMOR = dict(energy=8, satisfaction=4, boredom=-6, affection=2)                              # |d| = 20 -> 0.83

    def face(self, events, minutes=0, state=None, hour=NOON):
        return jinx_baseline.mood_now(state or _st(), events, NOW + minutes * 60, {}, hour)[0]

    def test_a_fresh_event_gives_its_full_face(self):
        self.assertEqual(self.face([ev("praise", 0, **self.PRAISE)]), "proud")
        self.assertEqual(self.face([ev("humor", 0, **self.HUMOR)]), "laughing")
        self.assertEqual(self.face([ev("criticism", 0, patience=-6, trust=-3, satisfaction=-5, pride=-5)]), "sad")
        self.assertEqual(self.face([ev("other_ai_mentioned", 0, jealousy=14, possessiveness=10)]), "grumpy")

    def test_intensity_halves_every_25_minutes(self):
        e = ev("praise", 0, **self.PRAISE)
        self.assertAlmostEqual(jinx_baseline.event_intensity(e, NOW), 1.0)
        self.assertAlmostEqual(jinx_baseline.event_intensity(e, NOW + 25 * 60), 0.5)
        self.assertAlmostEqual(jinx_baseline.event_intensity(e, NOW + 50 * 60), 0.25)

    def test_face_lasts_until_the_sum_drops_below_the_threshold_then_calm(self):
        e = [ev("praise", 0, **self.PRAISE)]
        self.assertEqual(self.face(e, 24), "proud")      # 1.0 * 0.5^(24/25) = 0.51
        self.assertIsNone(self.face(e, 26))              # 0.49
        self.assertIsNone(self.face(e, 120))

    def test_accumulation_extends_the_face(self):
        solo = [ev("praise", 0, **self.PRAISE)]
        trio = solo + [ev("praise", 0, **self.PRAISE), ev("gratitude", 0, trust=10, affection=5, pride=9)]
        self.assertIsNone(self.face(solo, 30))           # 1.0 * 0.5^(30/25) = 0.43
        self.assertIsNotNone(self.face(trio, 30))        # 3 * 0.43 = 1.3

    def test_the_total_is_capped(self):
        many = [ev("praise", 0, **self.PRAISE) for _ in range(10)]
        _, total = jinx_baseline.event_mood(many, NOW)
        self.assertEqual(total, jinx_baseline.EVENT_CAP)

    def test_neutral_triggers_never_give_a_face(self):
        for t in ("normal_interaction", "greeting", "deterministic", "decay"):
            self.assertIsNone(self.face([ev(t, 0, affection=50, trust=50)]), t)

    def test_unknown_trigger_and_garbage_are_ignored(self):
        self.assertIsNone(self.face([ev("brand_new_thing", 0, affection=30)]))
        self.assertIsNone(self.face([None, {"t": "x", "trigger": "praise"}, {"trigger": "praise"}, "praise"]))

    def test_future_events_do_not_count(self):
        self.assertIsNone(self.face([ev("praise", -5, **self.PRAISE)]))

    def test_events_beat_night_and_warmth_but_night_beats_calm(self):
        e = [ev("humor", 0, **self.HUMOR)]
        self.assertEqual(self.face(e, hour=2), "laughing")
        self.assertEqual(self.face([], hour=2), "sleepy")
        warm = {"affection": 90, "trust": 80, "patience": 70}
        crit = [ev("criticism", 0, patience=-6, trust=-3, satisfaction=-5, pride=-5)]
        self.assertEqual(self.face(crit, state=warm), "sad")

    def test_the_dominant_emote_wins(self):
        mixed = [ev("praise", 0, **self.PRAISE), ev("humor", 0, **self.HUMOR), ev("humor", 0, **self.HUMOR)]
        self.assertEqual(self.face(mixed), "laughing")

    def test_half_life_is_configurable(self):
        e = [ev("praise", 0, **self.PRAISE)]
        os.environ["JINX_MOOD_HALF_LIFE_MIN"] = "5"
        try:
            self.assertIsNone(self.face(e, 10))          # 0.25
            self.assertEqual(self.face(e, 4), "proud")   # 0.57
        finally:
            del os.environ["JINX_MOOD_HALF_LIFE_MIN"]
        os.environ["JINX_MOOD_HALF_LIFE_MIN"] = "nope"
        try:
            self.assertEqual(self.face(e, 20), "proud")  # vuelve a 25: 0.57
        finally:
            del os.environ["JINX_MOOD_HALF_LIFE_MIN"]

    def test_an_event_without_readable_deltas_weighs_less_than_a_full_one(self):
        w = jinx_baseline.event_intensity({"t": NOW, "trigger": "praise", "deltas": {}}, NOW)
        self.assertAlmostEqual(w, 0.4)


class EventLogTest(unittest.TestCase):
    def row(self, minutes_ago, trigger, deltas=None, event=True):
        import datetime as dt
        t = dt.datetime.fromtimestamp(NOW - minutes_ago * 60).isoformat()
        r = {"timestamp": t, "dimensions": {"affection": 70.0}}
        r["last_event"] = {"trigger": trigger, "deltas": deltas or {"affection": 1}, "text": "x"} if event else None
        return json.dumps(r)

    def test_keeps_only_real_events_inside_the_window_in_order(self):
        text = "\n".join([
            '{"timestamp": "2026-10-14T1',                 # línea cortada por el tail
            self.row(300, "humor"),                        # demasiado vieja
            self.row(40, "praise", {"affection": 10}),
            self.row(40, "praise", {"affection": 10}),     # repetido del mismo segundo
            self.row(30, "normal_interaction"),
            self.row(20, None, event=False),               # fila sin evento
            self.row(5, "criticism", {"patience": -6}),
            "no es json",
        ])
        out = jinx_baseline.parse_events_log(text, NOW)
        self.assertEqual([e["trigger"] for e in out], ["praise", "normal_interaction", "criticism"])
        self.assertEqual(out[0]["deltas"], {"affection": 10})
        self.assertAlmostEqual(out[0]["t"], NOW - 40 * 60, delta=1)

    def test_load_recent_events_reads_the_tail_and_never_fails(self):
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "emotion_turn_log.jsonl")
            with open(path, "w") as f:
                f.write("\n".join(self.row(m, "humor") for m in (200, 50, 10)) + "\n")
            got = jinx_baseline.load_recent_events(path, NOW)
            self.assertEqual(len(got), 2)
            self.assertEqual(jinx_baseline.load_recent_events(os.path.join(d, "nope.jsonl"), NOW), [])
            self.assertEqual(jinx_baseline.load_recent_events(d, NOW), [])  # un directorio: ilegible

    def test_a_huge_log_is_read_by_its_tail_only(self):
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "log.jsonl")
            with open(path, "w") as f:
                for _ in range(3000):
                    f.write(self.row(500, "humor") + "\n")
                f.write(self.row(1, "praise", {"affection": 10}) + "\n")
            got = jinx_baseline.load_recent_events(path, NOW, max_bytes=4096)
            self.assertEqual([e["trigger"] for e in got], ["praise"])


class MoodTagTest(unittest.TestCase):
    def test_strips_the_tag_and_returns_the_emote(self):
        self.assertEqual(jinx_chat.split_mood("jaja qué bueno [[mood:laughing]]"), ("jaja qué bueno", "laughing"))
        self.assertEqual(jinx_chat.split_mood("[[mood:sad]] lo siento"), ("lo siento", "sad"))
        self.assertEqual(jinx_chat.split_mood("a [[ MOOD : Smug ]] b"), ("a b", "smug"))

    def test_unknown_or_empty_tag_is_stripped_without_an_emote(self):
        self.assertEqual(jinx_chat.split_mood("hola [[mood:furious]]"), ("hola", None))
        self.assertEqual(jinx_chat.split_mood("hola [[mood:]]"), ("hola", None))

    def test_no_tag_is_untouched(self):
        self.assertEqual(jinx_chat.split_mood("hola"), ("hola", None))
        self.assertEqual(jinx_chat.split_mood("lista [[1]] y [[nota]]"), ("lista [[1]] y [[nota]]", None))

    def test_last_valid_tag_wins(self):
        self.assertEqual(jinx_chat.split_mood("x [[mood:sad]] y [[mood:happy]]"), ("x y", "happy"))


class MoodGatewayTest(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.thread = Thread(self.dir.name)
        self.mirror = FakeMirror()
        self.hermes = None

    def tearDown(self):
        self.dir.cleanup()

    def events(self, req, text="respuesta", state=None, hour=None, events=None):
        deps = Deps(self.thread, FakeBrain(text), self.mirror, "CORE", "MEM", lambda: {}, self.dir.name,
                    hermes=self.hermes, state=state, hour=hour, events=events)
        return list(handle(req, deps))

    def test_a_tagged_reply_is_clean_in_the_thread_and_telegram_and_carries_the_mood(self):
        ev = self.events({"op": "send", "text": "cuéntame un chiste", "cid": "m1", "src": "pc"},
                         text="jajaja buenísimo [[mood:laughing]]")
        self.assertEqual(ev[-1], {"ev": "done", "mode": "or", "text": "jajaja buenísimo",
                                  "mirrored": True, "mood": "laughing"})
        line = self.thread.read()[-1]
        self.assertEqual((line["text"], line["mood"]), ("jajaja buenísimo", "laughing"))
        self.assertEqual(self.mirror.sent[-1], "⚠️ Respaldo (Hermes no contestó): jajaja buenísimo")
        self.assertNotIn("[[", "".join(self.mirror.sent))

    def test_untagged_reply_has_no_mood_key_at_all(self):
        ev = self.events({"op": "send", "text": "hola", "cid": "m2", "src": "pc"})
        self.assertNotIn("mood", ev[-1])
        self.assertNotIn("mood", self.thread.read()[-1])

    def test_an_invalid_tag_is_stripped_and_gives_no_mood(self):
        ev = self.events({"op": "send", "text": "hola", "cid": "m3", "src": "pc"}, text="ok [[mood:enfadadisima]]")
        self.assertEqual(ev[-1]["text"], "ok")
        self.assertNotIn("mood", ev[-1])

    def test_a_retry_returns_the_same_mood_from_the_cache(self):
        self.events({"op": "send", "text": "x", "cid": "m4", "src": "pc"}, text="uf, por fin [[mood:relieved]]")
        ev = self.events({"op": "send", "text": "x", "cid": "m4", "src": "pc"})
        self.assertEqual(ev[-1]["mode"], "cached")
        self.assertEqual((ev[-1]["text"], ev[-1]["mood"]), ("uf, por fin", "relieved"))

    def test_only_a_tag_is_an_empty_reply(self):
        ev = self.events({"op": "send", "text": "x", "cid": "m5", "src": "pc"}, text="[[mood:happy]]")
        self.assertEqual(ev[-1]["text"], "(sin respuesta)")

    def test_history_keeps_the_mood_and_hermes_lines_are_deduplicated_without_the_tag(self):
        self.events({"op": "send", "text": "hola", "cid": "m6", "src": "pc"}, text="aquí estoy [[mood:happy]]")

        class FakeHermes:
            def history(self, n):
                return [{"id": 9, "role": "assistant", "text": "aquí estoy [[mood:happy]]", "ts": time.time() + 5},
                        {"id": 10, "role": "assistant", "text": "otra [[mood:smug]]", "ts": time.time() + 6}]

        self.hermes = FakeHermes()
        items = self.events({"op": "history", "n": 10})[-1]["items"]
        self.assertEqual([i["text"] for i in items], ["hola", "aquí estoy", "otra"])  # no duplicate, no tag
        self.assertEqual([i.get("mood") for i in items], [None, "happy", "smug"])

    def test_merge_keeps_a_valid_mood_and_drops_a_bogus_one(self):
        lines = [{"id": "x:jinx", "ts": 1.0, "src": "jinx", "text": "hola", "mood": "love"},
                 {"id": "y:jinx", "ts": 2.0, "src": "jinx", "text": "adiós", "mood": "nope"},
                 {"id": "z:jinx", "ts": 3.0, "src": "jinx", "text": "vale [[mood:proud]]"}]
        ev = self.events({"op": "merge", "lines": lines, "have": []})
        by_id = {d["id"]: d for d in self.thread.read()}
        self.assertEqual(by_id["x:jinx"]["mood"], "love")
        self.assertNotIn("mood", by_id["y:jinx"])
        self.assertEqual((by_id["z:jinx"]["text"], by_id["z:jinx"]["mood"]), ("vale", "proud"))
        self.assertEqual(ev[-1]["ev"], "merged")

    def test_op_mood_from_state(self):
        st = {"affection": 90.0, "trust": 80.0, "patience": 70.0, "energy": 60.0, "last_update": "t"}
        ev = self.events({"op": "mood"}, state=lambda: st, hour=lambda: 14)
        self.assertEqual(ev, [{"ev": "mood", "emote": "happy", "source": "state", "emotion_state": st}])

    def test_op_mood_follows_a_recent_event_and_calms_down(self):
        import time as _t
        jinx_chat._MOOD_LATCH = {}
        now = _t.time()
        st = jinx_chat.parse_emotion_state(REAL_STATE)
        praise = [{"t": now - 60, "trigger": "praise", "deltas": {"affection": 10, "trust": 5, "pride": 7, "energy": 4}}]
        ev1 = self.events({"op": "mood"}, state=lambda: st, hour=lambda: 15, events=lambda: praise)[0]
        self.assertEqual((ev1["emote"], ev1["source"]), ("proud", "state"))
        old = [{"t": now - 3 * 3600, "trigger": "praise", "deltas": {"affection": 10, "trust": 5, "pride": 7}}]
        ev2 = self.events({"op": "mood"}, state=lambda: st, hour=lambda: 15, events=lambda: old)[0]
        self.assertIsNone(ev2["emote"])

    def test_op_mood_survives_a_dead_events_source(self):
        def boom():
            raise RuntimeError("ssh caído")
        jinx_chat._MOOD_LATCH = {}
        st = {"affection": 90.0, "trust": 80.0, "patience": 70.0, "energy": 60.0}
        ev1 = self.events({"op": "mood"}, state=lambda: st, hour=lambda: 14, events=boom)[0]
        self.assertEqual(ev1["emote"], "happy")  # la cara del estado, sin eventos

    def test_ssh_events_reader_reads_a_bounded_tail(self):
        calls = []

        class P:
            stdout = '{"timestamp": "2099-01-01T00:00:00", "last_event": {"trigger": "praise", "deltas": {}}}\n'

        def run(cmd, **kw):
            calls.append((cmd, kw))
            return P()

        jinx_chat.ssh_events_reader(runner=run)
        self.assertEqual(calls[0][1]["timeout"], 12)
        self.assertIn("tail", " ".join(calls[0][0]))
        self.assertIn("262144", " ".join(calls[0][0]))

    def test_op_mood_normal_day_is_state_with_no_emote(self):
        st = jinx_chat.parse_emotion_state(REAL_STATE)
        ev = self.events({"op": "mood"}, state=lambda: st, hour=lambda: 15)[0]
        self.assertEqual((ev["emote"], ev["source"]), (None, "state"))
        self.assertEqual(ev["emotion_state"]["trust"], 76.18)

    def test_op_mood_at_night(self):
        ev = self.events({"op": "mood"}, state=lambda: _st(), hour=lambda: 23)[0]
        self.assertEqual((ev["emote"], ev["source"]), ("sleepy", "state"))

    def test_op_mood_without_state_or_with_a_dead_server_never_fails(self):
        expected = {"ev": "mood", "emote": None, "source": "text", "emotion_state": None}
        self.assertEqual(self.events({"op": "mood"}), [expected])

        def boom():
            raise RuntimeError("ssh caído")
        self.assertEqual(self.events({"op": "mood"}, state=boom), [expected])

    def test_main_answers_op_mood_without_building_the_brain(self):
        """`jinx-chat` takes the light path for mood (no vault, no brain) and it still reads the state."""
        import io
        import json
        import unittest.mock as mock

        out = io.StringIO()
        with mock.patch.object(jinx_chat, "BASE", self.dir.name), \
                mock.patch.object(jinx_chat, "build_real_deps", side_effect=AssertionError("brain built")), \
                mock.patch.object(jinx_chat, "ssh_state_reader",
                                  return_value={"affection": 95.0, "trust": 80.0, "patience": 70.0, "energy": 60.0}), \
                mock.patch.object(jinx_chat, "ssh_events_reader", return_value=[]), \
                mock.patch.dict(os.environ, {}, clear=False):
            os.environ.pop("JINX_NO_HERMES", None)
            jinx_chat.main(io.StringIO('{"op":"mood"}\n'), out)
        ev = json.loads(out.getvalue().splitlines()[0])
        self.assertEqual((ev["ev"], ev["source"]), ("mood", "state"))
        self.assertIn(ev["emote"], (None,) + jinx_chat.EMOTES)


if __name__ == "__main__":
    unittest.main()
