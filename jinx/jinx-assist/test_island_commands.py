"""Lo que Jinx puede pedirle a la isla: una lista cerrada, con lo de fuera de la lista rechazado."""
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(__file__))
import island_commands as ic
import jinx_thread


class SharedVectorsTest(unittest.TestCase):
    """island-vectors.json lo comprueban también el PC (island_cmd.rs) y scripts/check-contracts.sh."""

    def test_the_vectors_are_what_the_validator_says(self):
        import json
        with open(os.path.join(os.path.dirname(__file__), "island-vectors.json"), encoding="utf-8") as f:
            vectors = json.load(f)
        self.assertGreater(len(vectors), 25)
        for v in vectors:
            ok, clean = ic.validate(v["action"], v["args"])
            self.assertEqual(ok, v["ok"], v)
            if ok:
                self.assertEqual(clean, v["clean"], v)

    def test_the_skill_names_every_action_emote_and_hub_tool(self):
        with open(os.path.join(os.path.dirname(__file__), "island", "SKILL.md"), encoding="utf-8") as f:
            md = f.read()
        for a in ic.ACTIONS:
            self.assertIn("`%s`" % a, md)
        for word in ic.EMOTES + ic.HUB_TOOLS:
            self.assertIn(word, md)


class ValidateTest(unittest.TestCase):
    def test_the_list_is_closed_and_has_nothing_that_approves_sends_or_runs(self):
        self.assertEqual(set(ic.ACTIONS), {"state", "music", "pills", "mochi", "hub", "open_app", "close_app"})
        for bad in ("approve", "deny", "send_email", "prepare_email", "run", "shell", "settings", "open_url",
                    "set_key", "", None, 5, ["music"]):
            ok, why = ic.validate(bad, {})
            self.assertFalse(ok, bad)

    def test_the_mood_names_are_the_same_as_the_threads(self):
        self.assertEqual(tuple(ic.EMOTES), tuple(jinx_thread.EMOTES))

    def test_music(self):
        self.assertEqual(ic.validate("music", {"action": "next"}), (True, {"action": "next"}))
        self.assertEqual(ic.validate("music", {"action": "set_volume", "volume": 140}), (True, {"action": "set_volume", "volume": 100.0}))
        self.assertEqual(ic.validate("music", {"action": "set_volume", "volume": -3})[1]["volume"], 0.0)
        for bad in ({}, {"action": "format"}, {"action": "set_volume"}, {"action": "set_volume", "volume": "loud"},
                    {"action": "set_volume", "volume": True}, {"action": "search"}, {"action": "search", "query": "  "}):
            self.assertFalse(ic.validate("music", bad)[0], bad)
        ok, a = ic.validate("music", {"action": "search", "query": "  café\n del mar\x00 " + "x" * 300, "evil": "1"})
        self.assertTrue(ok)
        self.assertEqual(set(a), {"action", "query"})
        self.assertLessEqual(len(a["query"]), ic.QUERY_MAX)
        self.assertNotIn("\x00", a["query"])

    def test_pills(self):
        self.assertEqual(ic.validate("pills", {}), (True, {"action": "list"}))
        self.assertEqual(ic.validate("pills", {"action": "add", "names": "GitHub"}), (True, {"action": "add", "names": ["GitHub"]}))
        ok, a = ic.validate("pills", {"action": "add", "names": ["a", "b", "c", "d", "e", 5, None]})
        self.assertEqual(a["names"], ["a", "b", "c", "d"])
        self.assertEqual(ic.validate("pills", {"action": "set_main", "names": ["Jinx", "Music"]})[1]["names"], ["Jinx"])
        for bad in ({"action": "add"}, {"action": "add", "names": []}, {"action": "nuke", "names": ["x"]}):
            self.assertFalse(ic.validate("pills", bad)[0], bad)

    def test_mochi_and_hub(self):
        self.assertEqual(ic.validate("mochi", {"emote": "love"}), (True, {"emote": "love", "seconds": 3.0}))
        self.assertEqual(ic.validate("mochi", {"emote": "love", "seconds": 99})[1]["seconds"], 8.0)
        self.assertEqual(ic.validate("mochi", {"emote": "love", "seconds": 0})[1]["seconds"], 1.0)
        self.assertFalse(ic.validate("mochi", {"emote": "dance"})[0])
        self.assertEqual(ic.validate("hub", {"tool": "tasks"}), (True, {"tool": "tasks"}))
        self.assertFalse(ic.validate("hub", {"tool": "../../etc"})[0])

    def test_apps_take_only_a_name(self):
        for action in ("open_app", "close_app"):
            ok, a = ic.validate(action, {"name": "  Firefox  ", "args": ["--evil"], "path": "/bin/sh"})
            self.assertEqual((ok, a), (True, {"name": "Firefox"}))
            self.assertFalse(ic.validate(action, {})[0])
            self.assertFalse(ic.validate(action, {"name": "   "})[0])
            self.assertFalse(ic.validate(action, {"name": 7})[0])

    def test_state_takes_nothing(self):
        self.assertEqual(ic.validate("state", {"x": 1}), (True, {}))


if __name__ == "__main__":
    unittest.main()
