"""Ánimo de Jinx en el gateway: baseline_emote, la etiqueta oculta [[mood:…]], op: mood y el parseo de STATE.md."""
import os
import sys
import tempfile
import time
import unittest

sys.path.insert(0, os.path.dirname(__file__))
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


class BaselineEmoteTest(unittest.TestCase):
    def b(self, hour=14, **kw):
        return jinx_chat.baseline_emote(_st(**kw), hour)

    def test_a_normal_day_has_no_background_mood(self):
        self.assertIsNone(self.b())
        self.assertIsNone(jinx_chat.baseline_emote(jinx_chat.parse_emotion_state(REAL_STATE), 15))

    def test_patience_boundary(self):
        self.assertEqual(self.b(patience=30), "grumpy")
        self.assertIsNone(self.b(patience=31))

    def test_trust_boundary(self):
        self.assertEqual(self.b(trust=30), "sad")
        self.assertIsNone(self.b(trust=31))

    def test_warmth_boundaries(self):
        # warmth = mean(affection, trust, patience)
        self.assertEqual(self.b(affection=90, trust=78, patience=66), "happy")   # 78.0
        self.assertIsNone(self.b(affection=90, trust=78, patience=65.9))         # 77.97
        self.assertEqual(self.b(affection=50, trust=35, patience=35), "sleepy")  # 40.0
        self.assertIsNone(self.b(affection=50, trust=36, patience=35))           # 40.33

    def test_energy_boundary(self):
        self.assertEqual(self.b(energy=30), "sleepy")
        self.assertIsNone(self.b(energy=31))

    def test_night_window_edges(self):
        for h in (22, 23, 0, 5):
            self.assertEqual(self.b(hour=h), "sleepy", h)
        for h in (6, 12, 21):
            self.assertIsNone(self.b(hour=h), h)

    def test_negative_moods_beat_happy_and_night(self):
        self.assertEqual(self.b(hour=2, patience=20), "grumpy")
        self.assertEqual(self.b(hour=2, trust=10), "sad")
        self.assertEqual(self.b(hour=14, affection=120, trust=120, patience=29), "grumpy")

    def test_missing_or_bad_dimensions_skip_their_rule(self):
        self.assertIsNone(jinx_chat.baseline_emote({}, 14))
        self.assertIsNone(jinx_chat.baseline_emote({"affection": 90}, 14))  # no warmth without trust+patience
        self.assertEqual(jinx_chat.baseline_emote({"affection": 120}, 14), "love")  # love needs only affection
        self.assertIsNone(jinx_chat.baseline_emote({"patience": "x", "trust": True}, 14))
        self.assertIsNone(jinx_chat.baseline_emote(None, 3))
        self.assertEqual(jinx_chat.baseline_emote({"energy": 10}, None), "sleepy")

    def test_the_other_dimensions(self):
        # Each at its limit and just short of it (the real day has jealousy 25, worry 32, boredom 19, pride 45).
        self.assertEqual(self.b(jealousy=75), "grumpy")
        self.assertIsNone(self.b(jealousy=74.9))
        self.assertEqual(self.b(worry=70), "scared")
        self.assertIsNone(self.b(worry=69.9))
        self.assertEqual(self.b(emotion_score=-2), "sad")
        self.assertIsNone(self.b(emotion_score=-1.9))
        self.assertEqual(self.b(boredom=70), "bored")
        self.assertEqual(self.b(affection=95), "love")
        self.assertEqual(self.b(pride=85), "proud")
        self.assertEqual(self.b(energy=100, emotion_score=1), "excited")
        self.assertIsNone(self.b(energy=100, emotion_score=0.9))
        self.assertEqual(self.b(emotion_score=2), "happy")
        self.assertEqual(self.b(satisfaction=90), "happy")
        self.assertIsNone(self.b(satisfaction=89.9))

    def test_the_negative_ones_come_first(self):
        self.assertEqual(self.b(worry=90, affection=110), "scared")
        self.assertEqual(self.b(jealousy=90, worry=90), "grumpy")
        self.assertEqual(self.b(emotion_score=-3, satisfaction=95), "sad")
        self.assertEqual(self.b(hour=2, boredom=80), "bored")

    def test_only_valid_emotes_come_out(self):
        for kw in ({}, {"patience": 1}, {"trust": 1}, {"energy": 1},
                   {"affection": 120, "trust": 120, "patience": 120}):
            for h in (0, 12):
                e = jinx_chat.baseline_emote(_st(**kw), h)
                self.assertTrue(e is None or e in jinx_chat.EMOTES, e)

    def test_the_emote_list_is_mochis(self):
        self.assertEqual(len(jinx_chat.EMOTES), 19)
        self.assertEqual(set(jinx_chat.EMOTES), {
            "happy", "love", "proud", "surprised", "excited", "confused", "shy", "sad", "curious", "bored",
            "scared", "grumpy", "smug", "laughing", "crying", "relieved", "determined", "flustered", "sleepy"})


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

    def events(self, req, text="respuesta", state=None, hour=None):
        deps = Deps(self.thread, FakeBrain(text), self.mirror, "CORE", "MEM", lambda: {}, self.dir.name,
                    hermes=self.hermes, state=state, hour=hour)
        return list(handle(req, deps))

    def test_a_tagged_reply_is_clean_in_the_thread_and_telegram_and_carries_the_mood(self):
        ev = self.events({"op": "send", "text": "cuéntame un chiste", "cid": "m1", "src": "pc"},
                         text="jajaja buenísimo [[mood:laughing]]")
        self.assertEqual(ev[-1], {"ev": "done", "mode": "or", "text": "jajaja buenísimo",
                                  "mirrored": True, "mood": "laughing"})
        line = self.thread.read()[-1]
        self.assertEqual((line["text"], line["mood"]), ("jajaja buenísimo", "laughing"))
        self.assertEqual(self.mirror.sent[-1], "🟠 jajaja buenísimo")
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
                mock.patch.dict(os.environ, {}, clear=False):
            os.environ.pop("JINX_NO_HERMES", None)
            jinx_chat.main(io.StringIO('{"op":"mood"}\n'), out)
        ev = json.loads(out.getvalue().splitlines()[0])
        self.assertEqual((ev["ev"], ev["source"]), ("mood", "state"))
        self.assertIn(ev["emote"], (None,) + jinx_chat.EMOTES)


if __name__ == "__main__":
    unittest.main()
