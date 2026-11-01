"""La herramienta de Jinx: valida, pide, espera el resultado y dice la verdad cuando algo falla."""
import json
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(__file__))
import jinx_island as ji


class FakeServer:
    def __init__(self, post=(200, {"ok": True, "id": 7}), result=(200, {"ok": True, "done": True, "result": {"ok": True, "message": "hecho"}})):
        self.post, self.result, self.calls = post, result, []

    def __call__(self, method, path, body=None, timeout=30):
        self.calls.append((method, path, body))
        return self.post if method == "POST" else self.result


class RunTest(unittest.TestCase):
    def test_a_good_order_goes_to_the_server_clean_and_returns_the_pcs_answer(self):
        srv = FakeServer()
        ok, out = ji.run("music", {"action": "next", "sneaky": "x"}, call=srv)
        self.assertEqual((ok, out), (True, {"ok": True, "message": "hecho"}))
        self.assertEqual(srv.calls[0], ("POST", "/api/island/command", {"action": "music", "args": {"action": "next"}}))
        self.assertTrue(srv.calls[1][1].startswith("/api/island/result?id=7&wait="))

    def test_something_off_the_list_never_reaches_the_server(self):
        srv = FakeServer()
        ok, out = ji.run("approve", {"id": 1}, call=srv)
        self.assertFalse(ok)
        self.assertEqual(srv.calls, [])
        self.assertIn("acción desconocida", out["message"])

    def test_a_pc_that_is_off_is_said_plainly(self):
        srv = FakeServer(post=(409, {"error": "pc_offline", "message": "el PC no está conectado ahora mismo"}))
        ok, out = ji.run("state", {}, call=srv)
        self.assertEqual((ok, out["message"]), (False, "el PC no está conectado ahora mismo"))

    def test_a_pc_that_does_not_answer_in_time_is_not_reported_as_done(self):
        srv = FakeServer(result=(200, {"ok": True, "done": False, "result": None}))
        ok, out = ji.run("hub", {"tool": "tasks"}, call=srv)
        self.assertFalse(ok)
        self.assertIn("no contestó a tiempo", out["message"])

    def test_the_data_of_a_state_question_comes_back(self):
        srv = FakeServer(result=(200, {"ok": True, "done": True, "result": {"ok": True, "message": "", "data": {"pills": ["Jinx"]}}}))
        ok, out = ji.run("state", {}, call=srv)
        self.assertEqual(out["data"], {"pills": ["Jinx"]})

    def test_a_server_that_is_down_is_an_answer_not_a_crash(self):
        def boom(req, timeout):
            raise OSError("refused")
        code, body = ji._call("GET", "/x", opener=boom)
        self.assertEqual(code, 0)
        self.assertIn("no pude hablar", body["error"])

    def test_the_command_line_prints_one_json_line_and_an_exit_code(self):
        import io
        from contextlib import redirect_stdout
        buf = io.StringIO()
        with redirect_stdout(buf):
            self.assertEqual(ji.main(["x", "music", "not json"]), 1)
            self.assertEqual(ji.main(["x"]), 1)
        lines = buf.getvalue().strip().splitlines()
        self.assertEqual(len(lines), 2)
        for line in lines:
            self.assertFalse(json.loads(line)["ok"])


if __name__ == "__main__":
    unittest.main()
