"""op notify: lo que hacen los agentes del PC, dicho por Jinx donde estés, o por Telegram si nadie mira."""
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(__file__))
from jinx_chat import Deps, handle
from jinx_thread import Thread
from test_jinx_chat import FakeMirror


class NotifyTest(unittest.TestCase):
    def deps(self, presence):
        self.mirror = FakeMirror()
        self.thread = Thread(tempfile.mkdtemp())
        return Deps(self.thread, None, self.mirror, "", "", dict, None, presence=presence)

    def run_op(self, req, presence):
        return list(handle(dict({"op": "notify"}, **req), self.deps(presence)))

    def test_someone_watching_gets_it_on_their_device_only(self):
        got = []
        ev = self.run_op({"text": "Claude Code terminó en coucou (12 min).", "kind": "info", "source": "agentes"},
                         lambda t, k, s: got.append((t, k, s)) or True)
        self.assertEqual(ev, [{"ev": "notified", "watched": True, "telegram": False}])
        self.assertEqual(got, [("Claude Code terminó en coucou (12 min).", "info", "agentes")])
        self.assertEqual(self.mirror.sent, [])
        self.assertEqual(self.thread.read(), [])  # never in the shared thread

    def test_nobody_watching_or_no_presence_goes_to_telegram_as_jinx(self):
        ev = self.run_op({"text": "OpenCode tuvo un error en web."}, lambda t, k, s: False)
        self.assertEqual(ev[0]["telegram"], True)
        self.assertEqual(self.mirror.sent, ["OpenCode tuvo un error en web."])

        def down(t, k, s):
            raise OSError("no route")
        ev = self.run_op({"text": "x"}, down)
        self.assertEqual((ev[0]["watched"], ev[0]["telegram"]), (None, True))

    def test_text_is_cleaned_and_bounded_and_kind_checked(self):
        got = []
        self.run_op({"text": "  a\n  b  " + "z" * 400, "kind": "panic"}, lambda t, k, s: got.append((t, k)) or True)
        self.assertTrue(got[0][0].startswith("a b zzz"), got[0][0][:10])
        self.assertLessEqual(len(got[0][0]), 300)
        self.assertEqual(got[0][1], "info")
        self.assertEqual(self.run_op({"text": "   "}, lambda *a: True)[0]["ev"], "error")


if __name__ == "__main__":
    unittest.main()
