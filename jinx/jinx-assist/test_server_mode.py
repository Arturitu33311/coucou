"""El gateway en el servidor: sin ~/bin/jinx ni bóveda del portátil, secretos de la bóveda del servidor."""
import io
import json
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(__file__))
import jinx_chat


class _Resp(io.BytesIO):
    def __enter__(self):
        return self

    def __exit__(self, *a):
        return False


class _P:
    def __init__(self, rc, out):
        self.returncode, self.stdout = rc, out


class ServerModeTest(unittest.TestCase):
    def test_load_jinx_is_none_without_the_laptop_module(self):
        with tempfile.TemporaryDirectory() as home:
            old = jinx_chat.HOME
            jinx_chat.HOME = home
            try:
                self.assertIsNone(jinx_chat._load_jinx())
            finally:
                jinx_chat.HOME = old

    def test_build_real_deps_uses_server_mode_when_there_is_no_laptop_module(self):
        with tempfile.TemporaryDirectory() as base:
            old_load, old_srv = jinx_chat._load_jinx, jinx_chat.build_server_deps
            jinx_chat._load_jinx = lambda: None
            jinx_chat.build_server_deps = lambda b, d: ("server", b, d)
            try:
                self.assertEqual(jinx_chat.build_real_deps(base, True), ("server", base, True))
            finally:
                jinx_chat._load_jinx, jinx_chat.build_server_deps = old_load, old_srv

    def test_server_deps_ask_the_vault_only_on_first_real_use_and_serve_history(self):
        calls = []
        with tempfile.TemporaryDirectory() as base:
            deps = jinx_chat.build_server_deps(base, secret=lambda n: calls.append(n) or "x")
            self.assertEqual(calls, [])
            evs = list(jinx_chat.run_line('{"op":"history","n":1}', deps))
            self.assertEqual([e["ev"] for e in evs], ["history"])
            self.assertEqual(deps.secrets()["chat"], "x")
            self.assertEqual(sorted(set(calls)), ["OPENROUTER_API_KEY", "TELEGRAM_ALLOWED_CHAT", "TELEGRAM_BOT_TOKEN"])

    def test_server_secret_maps_the_chat_name_to_home_channel(self):
        asked = []

        def run(argv, **kw):
            asked.append(argv[-1])
            return _P(0, "12345\n") if argv[-1] == "TELEGRAM_HOME_CHANNEL" else _P(1, "")

        self.assertEqual(jinx_chat.server_secret("TELEGRAM_ALLOWED_CHAT", run=run, cmd="hs"), "12345")
        self.assertEqual(asked, ["TELEGRAM_ALLOWED_CHAT", "TELEGRAM_HOME_CHANNEL"])
        self.assertEqual(jinx_chat.server_secret("NOPE", run=lambda *a, **k: _P(1, ""), cmd="hs"), "")

    def test_fallback_brain_answers_when_hermes_is_down(self):
        sent = {}

        def opener(req, timeout=0):
            sent["auth"] = req.get_header("Authorization")
            return _Resp(json.dumps({"choices": [{"message": {"content": "hola"}}]}).encode())

        class Down:
            def reply(self, m):
                raise RuntimeError("Hermes caído")

        fb = jinx_chat.ServerFallbackBrain(lambda: "sk-secret", lambda: "ALMA", opener=opener)
        self.assertEqual(jinx_chat.ChainBrain([Down(), fb]).reply([{"role": "user", "content": "hi"}]), ("or", "hola"))
        self.assertEqual(sent["auth"], "Bearer sk-secret")

    def test_fallback_brain_never_leaks_the_key_and_needs_one(self):
        def boom(req, timeout=0):
            raise OSError("fallo con sk-secret dentro")

        with self.assertRaises(RuntimeError) as cm:
            jinx_chat.ServerFallbackBrain(lambda: "sk-secret", lambda: "", opener=boom).reply([])
        self.assertNotIn("sk-secret", str(cm.exception))
        with self.assertRaises(RuntimeError):
            jinx_chat.ServerFallbackBrain(lambda: "", lambda: "").reply([])


class PinnedMirrorTest(unittest.TestCase):
    class Conn:
        def __init__(self, ip, plan, log):
            self.ip, self.plan, self.log = ip, plan, log

        def request(self, *a):
            self.log.append(self.ip)
            out = self.plan.get(self.ip)
            if isinstance(out, Exception):
                raise out

        def getresponse(self):
            body = json.dumps(self.plan[self.ip]).encode()

            class R:
                def read(self_inner):
                    return body
            return R()

        def close(self):
            pass

    def mirror(self, plan, ips=("1.1.1.1", "2.2.2.2")):
        log = []
        m = jinx_chat.TelegramMirror("123456:" + "A" * 30, "42", ips=ips,
                                     conn_factory=lambda ip: self.Conn(ip, plan, log))
        return m, log

    def test_skips_an_unreachable_ip_and_stops_at_the_first_answer(self):
        m, log = self.mirror({"1.1.1.1": OSError("No route to host"), "2.2.2.2": {"ok": True}})
        self.assertTrue(m.send("hola"))
        self.assertEqual(log, ["1.1.1.1", "2.2.2.2"])

    def test_all_routes_down_is_false_and_records_why(self):
        m, log = self.mirror({"1.1.1.1": OSError("x"), "2.2.2.2": OSError("y"), None: OSError("z")})
        self.assertFalse(m.send("hola"))
        self.assertEqual(log, ["1.1.1.1", "2.2.2.2", None])
        self.assertEqual(m.last_error, "OSError")

    def test_a_bad_chat_stops_without_trying_other_ips(self):
        m, log = self.mirror({"1.1.1.1": {"ok": False, "error_code": 400, "description": "chat not found"}})
        self.assertFalse(m.send("hola"))
        self.assertEqual(log, ["1.1.1.1"])

    def test_ips_come_from_the_hermes_env_file_then_defaults(self):
        with tempfile.TemporaryDirectory() as d:
            f = os.path.join(d, ".env")
            with open(f, "w") as fh:
                fh.write("A=1\nTELEGRAM_FALLBACK_IPS=\"9.9.9.9, 8.8.8.8\"\n")
            self.assertEqual(jinx_chat.telegram_ips(env={}, env_file=f), ["9.9.9.9", "8.8.8.8"])
            self.assertEqual(jinx_chat.telegram_ips(env={}, env_file=os.path.join(d, "nope")),
                             list(jinx_chat.TG_DEFAULT_IPS))
            self.assertEqual(jinx_chat.telegram_ips(env={"TELEGRAM_FALLBACK_IPS": "7.7.7.7"}, env_file=f), ["7.7.7.7"])


if __name__ == "__main__":
    unittest.main()
