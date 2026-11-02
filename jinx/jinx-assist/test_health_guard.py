import importlib.machinery
import importlib.util
import pathlib
import unittest

p = pathlib.Path(__file__).with_name("jinx-health-guard")
spec = importlib.util.spec_from_loader("guard", importlib.machinery.SourceFileLoader("guard", str(p)))
g = importlib.util.module_from_spec(spec)
spec.loader.exec_module(g)

NOW = 1_000_000.0


def unit(**kw):
    base = {"active": "active", "enabled": "enabled", "nrestarts": 0, "port_ok": None}
    base.update(kw)
    return base


class Decide(unittest.TestCase):
    def test_healthy(self):
        self.assertEqual(g.decide(unit(), {}, NOW)[0], "ok")

    def test_failed_is_restarted(self):
        self.assertEqual(g.decide(unit(active="failed"), {}, NOW)[0], "restart")

    def test_stopped_by_hand_is_left_alone(self):
        self.assertEqual(g.decide(unit(active="inactive"), {}, NOW)[0], "ok")

    def test_disabled_is_skipped(self):
        self.assertEqual(g.decide(unit(active="failed", enabled="disabled"), {}, NOW)[0], "skip")

    def test_crash_loop_alerts_instead_of_restarting(self):
        action, _ = g.decide(unit(active="failed", nrestarts=40), {"nrestarts": 10}, NOW)
        self.assertEqual(action, "alert-loop")

    def test_dead_port_needs_two_runs(self):
        u = unit(port_ok=False)
        self.assertEqual(g.decide(u, {}, NOW)[0], "ok")
        self.assertEqual(g.decide(u, {"dead_port": True}, NOW)[0], "restart")

    def test_restart_budget(self):
        prev = {"restarts": [NOW - 60, NOW - 120, NOW - 180]}
        self.assertEqual(g.decide(unit(active="failed"), prev, NOW)[0], "alert-limit")
        old = {"restarts": [NOW - 4000, NOW - 5000, NOW - 6000]}
        self.assertEqual(g.decide(unit(active="failed"), old, NOW)[0], "restart")


if __name__ == "__main__":
    unittest.main()
