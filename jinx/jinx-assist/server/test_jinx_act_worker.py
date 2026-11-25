import pathlib
import sys
import time
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).parent))
import jinx_act_worker as w  # noqa: E402

NOW = time.mktime((2026, 11, 2, 15, 30, 0, 0, 0, -1))  # a Monday afternoon, local time


def act(action, ref="pend:abc", **msg):
    return {"id": 1, "action": action, "ref": ref, "by": "s22",
            "msg": {"text": "Renovar la tarjeta", "kind": "info", "source": "pendientes", **msg}}


class Process(unittest.TestCase):
    def setUp(self):
        self.resolved, self.scheduled = [], []
        self.resolve = lambda pid, acc: (self.resolved.append((pid, acc)) or {"ok": True})
        self.schedule = lambda when, msg: self.scheduled.append((when, msg))

    def test_done_resolves_the_pendiente_as_hecho(self):
        out = w.process(act("done"), NOW, self.resolve, self.schedule)
        self.assertEqual(self.resolved, [("abc", "hecho")])
        self.assertEqual(out, "pend:abc -> hecho")

    def test_dismiss_marks_it_descartado(self):
        w.process(act("dismiss"), NOW, self.resolve, self.schedule)
        self.assertEqual(self.resolved, [("abc", "descartado")])

    def test_snooze_comes_back_in_an_hour_with_the_same_notice(self):
        w.process(act("snooze"), NOW, self.resolve, self.schedule)
        self.assertEqual(self.resolved, [])
        when, msg = self.scheduled[0]
        self.assertEqual(when, NOW + 3600)
        self.assertEqual(msg["text"], "Renovar la tarjeta")

    def test_tomorrow_is_nine_the_next_morning(self):
        w.process(act("tomorrow"), NOW, self.resolve, self.schedule)
        t = time.localtime(self.scheduled[0][0])
        self.assertEqual((t.tm_mday, t.tm_hour, t.tm_min), (3, 9, 0))

    def test_a_notice_without_a_pendiente_just_acknowledges(self):
        self.assertEqual(w.process(act("done", ref=""), NOW, self.resolve, self.schedule), "acknowledged")
        self.assertEqual(w.process(act("done", ref="venc:1"), NOW, self.resolve, self.schedule), "acknowledged")
        self.assertEqual((self.resolved, self.scheduled), ([], []))

    def test_snooze_works_even_without_a_pendiente(self):
        w.process(act("snooze", ref=""), NOW, self.resolve, self.schedule)
        self.assertEqual(len(self.scheduled), 1)

    def test_a_failed_store_raises_so_the_press_is_retried(self):
        with self.assertRaises(RuntimeError):
            w.process(act("done"), NOW, lambda p, a: {"ok": False, "error": "store down"}, self.schedule)


class Resend(unittest.TestCase):
    MSG = {"text": "Renovar la tarjeta", "kind": "info", "source": "pendientes",
           "actions": [{"id": "done", "label": "Hecho"}], "ref": "pend:abc"}

    def test_delivered_notice_keeps_its_buttons_and_says_it_is_a_reminder(self):
        calls, tg = [], []
        w.resend(self.MSG, lambda *a, **k: (calls.append((a, k)) or (True, "s22")), tg.append)
        self.assertEqual(calls[0][0][0], "⏰ Te lo recuerdo: Renovar la tarjeta")
        self.assertEqual(calls[0][1], {"actions": self.MSG["actions"], "ref": "pend:abc"})
        self.assertEqual(tg, [])

    def test_unwatched_counts_as_delivered_and_goes_to_telegram_once(self):
        tg = []
        w.resend(self.MSG, lambda *a, **k: (False, "nadie mirando (vga)"), tg.append)   # must not raise: no retry
        self.assertEqual(tg, ["⏰ Te lo recuerdo: Renovar la tarjeta"])

    def test_a_real_failure_raises_so_it_is_retried(self):
        with self.assertRaises(RuntimeError):
            w.resend(self.MSG, lambda *a, **k: (False, "Connection refused"), lambda t: None)


class Resends(unittest.TestCase):
    def test_split_due_and_waiting(self):
        q = [{"at": NOW - 5, "msg": {}}, {"at": NOW + 600, "msg": {}}, {"at": NOW, "msg": {}}]
        due, waiting = w.due_resends(q, NOW)
        self.assertEqual((len(due), len(waiting)), (2, 1))


if __name__ == "__main__":
    unittest.main()
