"""El portátil como respaldo: lo que atendió mientras el servidor estaba caído vuelve al servidor."""
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(__file__))
import jinx_chat
from jinx_chat import Deps, handle
from jinx_thread import Thread
from test_jinx_chat import FakeMirror


def line(i, src="s21", text="hola", mirrored=True):
    return {"id": "%s:%s" % (i, src), "ts": float(i), "src": src, "text": text, "cid": str(i), "mirrored": mirrored}


class SyncToServerTest(unittest.TestCase):
    def setUp(self):
        self.laptop = Thread(tempfile.mkdtemp())
        self.serverdir = tempfile.mkdtemp()
        self.server = Thread(self.serverdir)

    def server_ask(self, req):
        # The server's own merge op, for real.
        self.mirror = getattr(self, "mirror", FakeMirror())
        deps = Deps(self.server, None, self.mirror, "", "", dict, self.serverdir)
        return list(handle(req, deps))

    def test_what_the_laptop_answered_reaches_the_server_and_back(self):
        self.server.merge([line(1), line(2, "jinx", "hola!")])
        self.laptop.merge([line(1), line(3, "pc", "¿sigues?"), line(4, "jinx", "aquí, en el portátil", mirrored=False)])
        sent, got = jinx_chat.sync_to_server(self.laptop, self.server_ask)
        self.assertEqual(sent, 2)
        self.assertEqual(got, 1)
        ids = lambda t: [d["id"] for d in t.read()]
        self.assertEqual(ids(self.server), ids(self.laptop))
        self.assertEqual(ids(self.server), ["1:s21", "2:jinx", "3:pc", "4:jinx"])
        # The server mirrors to Telegram only what nobody had mirrored yet (the laptop's own unmirrored reply).
        self.assertEqual(len(self.mirror.sent), 1)
        self.assertIn("aquí, en el portátil", self.mirror.sent[0])
        # Twice changes nothing.
        self.assertEqual(jinx_chat.sync_to_server(self.laptop, self.server_ask), (0, 0))

    def test_a_server_that_is_not_the_gateway_changes_nothing_here(self):
        self.laptop.merge([line(1)])
        with self.assertRaises(RuntimeError):
            jinx_chat.sync_to_server(self.laptop, lambda req: [{"ok": False}])
        with self.assertRaises(RuntimeError):
            jinx_chat.sync_to_server(self.laptop, lambda req: [{"ev": "error", "msg": "src desconocido"}])
        self.assertEqual(len(self.laptop.read()), 1)


if __name__ == "__main__":
    unittest.main()
