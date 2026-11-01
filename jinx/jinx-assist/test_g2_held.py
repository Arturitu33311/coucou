"""Las gafas Even G2 (src g2): la respuesta no se reparte sola; si nadie la recoge, `deliver` la reparte."""
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(__file__))
from jinx_chat import Deps, handle
from jinx_thread import Thread
from test_jinx_chat import FakeMirror


class Brain:
    def reply(self, messages):
        return "hermes", "Hola, soy Jinx."


class G2Test(unittest.TestCase):
    def setUp(self):
        self.mirror = FakeMirror()
        self.thread = Thread(tempfile.mkdtemp())
        self.island = []
        self.watched = True
        self.deps = Deps(self.thread, Brain(), self.mirror, "", "", dict, tempfile.mkdtemp(),
                         presence=lambda t, k, s: self.island.append(t) or self.watched)

    def ask(self, cid="c1"):
        return list(handle({"op": "send", "text": "hola", "cid": cid, "src": "g2"}, self.deps))

    def test_answer_is_held_for_the_glasses_not_delivered(self):
        ev = self.ask()
        self.assertEqual(ev[-1]["ev"], "done")
        self.assertTrue(ev[-1]["held"])
        self.assertEqual(self.island, [])
        self.assertEqual(self.mirror.sent, [])
        self.assertTrue(all(d["mirrored"] for d in self.thread.read()))

    def test_retry_of_a_held_cid_is_still_held(self):
        self.ask()
        ev = self.ask()
        self.assertEqual(ev[-1]["mode"], "cached")
        self.assertEqual((self.island, self.mirror.sent), ([], []))

    def test_deliver_goes_to_the_island_when_someone_watches(self):
        self.ask()
        ev = list(handle({"op": "deliver", "cid": "c1"}, self.deps))
        self.assertEqual(ev, [{"ev": "delivered", "mirrored": True}])
        self.assertEqual(self.island, ["Hola, soy Jinx."])
        self.assertEqual(self.mirror.sent, [])

    def test_deliver_goes_to_telegram_when_nobody_watches(self):
        self.watched = False
        self.ask()
        ev = list(handle({"op": "deliver", "cid": "c1"}, self.deps))
        self.assertEqual(ev[0]["ev"], "delivered")
        self.assertEqual(self.mirror.sent, ["hola", "Hola, soy Jinx."])  # the question goes with its answer

    def test_deliver_needs_a_known_cid(self):
        self.assertEqual(list(handle({"op": "deliver"}, self.deps))[0]["ev"], "error")
        self.assertEqual(list(handle({"op": "deliver", "cid": "zz"}, self.deps))[0]["ev"], "error")


if __name__ == "__main__":
    unittest.main()
