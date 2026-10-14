import os
import sys
import tempfile
import threading
import unittest

sys.path.insert(0, os.path.dirname(__file__))
from jinx_thread import Thread, HIST_MAX


class ThreadTest(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.t = Thread(self.dir.name)

    def tearDown(self):
        self.dir.cleanup()

    def test_append_and_read_roundtrip_with_emoji_and_newlines(self):
        d = self.t.append("s21", "hola\n\"mundo\" 😀", cid="c1", ts=10.0)
        self.assertEqual(d["id"], "c1:s21")
        self.assertEqual(self.t.read()[0]["text"], "hola\n\"mundo\" 😀")

    def test_same_cid_and_src_is_not_duplicated(self):
        self.t.append("s21", "uno", cid="c1", ts=1.0)
        self.t.append("s21", "uno otra vez", cid="c1", ts=2.0)
        self.assertEqual(len(self.t.read()), 1)
        self.assertEqual(self.t.read()[0]["text"], "uno")

    def test_answer_for_finds_the_jinx_line_of_that_cid(self):
        self.t.append("s21", "pregunta", cid="c1", ts=1.0)
        self.assertIsNone(self.t.answer_for("c1"))
        self.t.append("jinx", "respuesta", cid="c1", ts=2.0)
        self.assertEqual(self.t.answer_for("c1")["text"], "respuesta")

    def test_context_maps_roles_and_trims_to_hist_max(self):
        for i in range(30):
            self.t.append("tg", "u%d" % i, ts=float(i))
        ctx = self.t.context()
        self.assertEqual(len(ctx), HIST_MAX)
        self.assertEqual(ctx[-1], {"role": "user", "content": "u29"})
        self.t.append("jinx", "a", cid="x", ts=100.0)
        self.assertEqual(self.t.context()[-1], {"role": "assistant", "content": "a"})

    def test_malformed_lines_are_skipped(self):
        self.t.append("tg", "ok", ts=1.0)
        with open(self.t.path, "a", encoding="utf-8") as f:
            f.write("{no json\n\n[1,2]\n")
        self.assertEqual([d["text"] for d in self.t.read()], ["ok"])

    def test_merge_adds_missing_dedupes_and_orders_by_ts(self):
        self.t.append("tg", "b", ts=20.0)
        incoming = [
            {"id": "c1:s22", "ts": 10.0, "src": "s22", "text": "a", "cid": "c1", "mirrored": False},
            {"id": "c1:s22", "ts": 10.0, "src": "s22", "text": "a", "cid": "c1", "mirrored": False},
            {"id": "", "ts": 1.0, "src": "s22", "text": "sin id"},
        ]
        added = self.t.merge(incoming)
        self.assertEqual([d["id"] for d in added], ["c1:s22"])
        self.assertEqual([d["text"] for d in self.t.read()], ["a", "b"])

    def test_merge_never_unmirrors_and_adopts_a_mirrored_flag(self):
        self.t.append("s21", "a", cid="c1", ts=1.0, mirrored=False)
        self.t.append("s21", "b", cid="c2", ts=2.0, mirrored=True)
        added = self.t.merge([
            {"id": "c1:s21", "ts": 1.0, "src": "s21", "text": "a", "cid": "c1", "mirrored": True},
            {"id": "c2:s21", "ts": 2.0, "src": "s21", "text": "b", "cid": "c2", "mirrored": False},
        ])
        self.assertEqual(added, [])
        by = {d["id"]: d["mirrored"] for d in self.t.read()}
        self.assertEqual(by, {"c1:s21": True, "c2:s21": True})

    def test_since_returns_only_unknown_lines(self):
        a = self.t.append("tg", "a", ts=1.0)
        self.t.append("tg", "b", ts=2.0)
        self.assertEqual([d["text"] for d in self.t.since({a["id"]})], ["b"])

    def test_mark_mirrored_and_unmirrored(self):
        a = self.t.append("s21", "a", cid="c", ts=1.0)
        self.assertEqual(len(self.t.unmirrored()), 1)
        self.t.mark_mirrored({a["id"]})
        self.assertEqual(self.t.unmirrored(), [])

    def test_concurrent_appends_lose_nothing(self):
        def work(n):
            for i in range(25):
                self.t.append("tg", "%d-%d" % (n, i))
        threads = [threading.Thread(target=work, args=(n,)) for n in range(4)]
        [x.start() for x in threads]
        [x.join() for x in threads]
        self.assertEqual(len(self.t.read()), 100)


if __name__ == "__main__":
    unittest.main()
