"""ops approvals/approve: el gateway pasa al puente de Hermes las preguntas de Jinx y la respuesta de Alan."""
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(__file__))
import jinx_chat
from jinx_chat import Deps, handle
from jinx_thread import Thread
from test_jinx_chat import FakeMirror

ITEM = {"run_id": "run_abc", "request_id": "r1", "command": "rm -rf x", "description": "delete",
        "choices": ["once", "deny"], "ts": 1.0}


class ApprovalsOpTest(unittest.TestCase):
    def deps(self, bridge):
        return Deps(Thread(tempfile.mkdtemp()), None, FakeMirror(), "", "", dict, None, approvals=bridge)

    def test_approvals_passes_through_and_never_fails(self):
        asked = []
        ev = list(handle({"op": "approvals"}, self.deps(lambda r: asked.append(r) or {"ev": "approvals", "items": [ITEM]})))
        self.assertEqual(ev, [{"ev": "approvals", "items": [ITEM]}])
        self.assertEqual(asked, [{"op": "approvals"}])

        def down(r):
            raise RuntimeError("no ssh")
        self.assertEqual(list(handle({"op": "approvals"}, self.deps(down))), [{"ev": "approvals", "items": []}])
        self.assertEqual(list(handle({"op": "approvals"}, self.deps(None))), [{"ev": "approvals", "items": []}])

    def test_approve_sends_only_the_known_fields(self):
        asked = []
        req = {"op": "approve", "run_id": "run_abc", "request_id": "r1", "choice": "once", "extra": "x"}
        ev = list(handle(req, self.deps(lambda r: asked.append(r) or {"ev": "approved", "choice": "once"})))
        self.assertEqual(ev, [{"ev": "approved", "choice": "once"}])
        self.assertEqual(asked, [{"op": "approve", "run_id": "run_abc", "request_id": "r1", "choice": "once"}])

    def test_approve_refuses_bad_shapes_without_asking(self):
        asked = []
        bridge = lambda r: asked.append(r) or {"ev": "approved"}
        for req in ({"choice": "yes", "run_id": "r", "request_id": ""}, {"choice": "once", "request_id": ""},
                    {"choice": "once", "run_id": 5, "request_id": ""}):
            self.assertEqual(list(handle(dict(req, op="approve"), self.deps(bridge)))[0]["ev"], "error")
        self.assertEqual(asked, [])

    def test_a_bridge_error_comes_back_as_said(self):
        ev = list(handle({"op": "approve", "run_id": "run_abc", "request_id": "r1", "choice": "deny"},
                         self.deps(lambda r: {"ev": "error", "msg": "esa aprobación ya no está pendiente"})))
        self.assertEqual(ev, [{"ev": "error", "msg": "esa aprobación ya no está pendiente"}])

    def test_a_send_may_wait_for_alan_but_other_ops_stay_short(self):
        seen = []

        class P:
            stdout = '{"ev":"done","text":"ok"}\n'
            stderr = ""
        orig = jinx_chat.subprocess.run
        jinx_chat.subprocess.run = lambda *a, **k: seen.append(k["timeout"]) or P()
        try:
            jinx_chat.ssh_runner({"op": "send", "text": "x"})
            jinx_chat.ssh_runner({"op": "approvals"})
        finally:
            jinx_chat.subprocess.run = orig
        self.assertEqual(seen, [jinx_chat.SEND_WAIT, 85])


if __name__ == "__main__":
    unittest.main()
