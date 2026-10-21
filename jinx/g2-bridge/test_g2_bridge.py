import json
import os
import sys
import threading
import unittest
import urllib.error
import urllib.request
from http.server import ThreadingHTTPServer

sys.path.insert(0, os.path.dirname(__file__))
import g2_bridge as b
import g2_stt

KEY = "k" * 20


class FakeGateway:
    def __init__(self, text="Hola Alan", fail=False):
        self.calls = []
        self.text = text
        self.fail = fail

    def __call__(self, req):
        self.calls.append(req)
        if req["op"] == "send":
            if self.fail:
                return [{"ev": "ack"}, {"ev": "error", "error": "modelo caído"}]
            return [{"ev": "ack"}, {"ev": "done", "mode": "hermes", "text": self.text, "mirrored": True,
                                    "held": True, "mood": "happy"}]
        return [{"ev": "delivered", "mirrored": True}]


class Clock:
    t = 1000.0

    def __call__(self):
        return self.t


class Server:
    def __init__(self, gw, stt=None, notices=None, now=None):
        self.clock = now or Clock()
        self.gw = gw
        self.jobs = b.Jobs(gw, now=self.clock)
        kw = {}
        if stt:
            kw["stt"] = stt
        if notices:
            kw["notices"] = notices
        self.httpd = ThreadingHTTPServer(("127.0.0.1", 0), b.make_handler(b.App(self.jobs, KEY, **kw)))
        threading.Thread(target=self.httpd.serve_forever, daemon=True).start()
        self.base = "http://127.0.0.1:%d" % self.httpd.server_address[1]

    def close(self):
        self.httpd.shutdown()
        self.httpd.server_close()

    def call(self, method, path, body=None, key=KEY, raw=None, headers=None):
        data = raw if raw is not None else (json.dumps(body).encode() if body is not None else None)
        h = dict(headers or {})
        if key:
            h["Authorization"] = "Bearer " + key
        req = urllib.request.Request(self.base + path, data=data, method=method, headers=h)
        try:
            with urllib.request.urlopen(req, timeout=10) as r:
                return r.status, r.read(), r.headers
        except urllib.error.HTTPError as e:
            return e.code, e.read(), e.headers

    def json(self, *a, **k):
        code, body, _ = self.call(*a, **k)
        return code, json.loads(body)


class BridgeTest(unittest.TestCase):
    def setUp(self):
        self.s = Server(FakeGateway())

    def tearDown(self):
        self.s.close()

    def test_auth(self):
        self.assertEqual(self.s.json("GET", "/health", key=None)[0], 200)
        self.assertEqual(self.s.json("POST", "/api/g2/ask", {"text": "x"}, key=None)[0], 401)
        self.assertEqual(self.s.json("POST", "/api/g2/ask", {"text": "x"}, key="mala")[0], 401)
        code, _, _ = self.s.call("POST", "/api/g2/ask", {"text": "x"}, key=None, headers={"X-G2-Key": KEY})
        self.assertEqual(code, 200)

    def test_empty_key_never_authorizes(self):
        self.assertFalse(b.authorized({"Authorization": "Bearer "}, ""))

    def test_ask_goes_through_gateway_as_g2(self):
        code, d = self.s.json("POST", "/api/g2/ask", {"text": "  qué   hora es ", "cid": "abc-1"})
        self.assertEqual((code, d["status"], d["text"], d["mode"], d["mood"]), (200, "done", "Hola Alan", "hermes", "happy"))
        self.assertEqual(self.s.gw.calls[0], {"op": "send", "text": "qué hora es", "cid": "abc-1", "src": "g2"})
        self.assertEqual(self.s.json("POST", "/api/g2/ask", {"text": ""})[0], 400)

    def test_same_cid_asks_once(self):
        self.s.json("POST", "/api/g2/ask", {"text": "a", "cid": "dup"})
        self.s.json("POST", "/api/g2/ask", {"text": "a", "cid": "dup"})
        self.assertEqual(len([c for c in self.s.gw.calls if c["op"] == "send"]), 1)

    def test_bad_cid_is_replaced(self):
        _, d = self.s.json("POST", "/api/g2/ask", {"text": "a", "cid": "x y;rm"})
        self.assertTrue(d["id"].startswith("g2"))

    def test_gateway_error_is_reported(self):
        s = Server(FakeGateway(fail=True))
        try:
            _, d = s.json("POST", "/api/g2/ask", {"text": "a"})
            self.assertEqual((d["status"], d["error"]), ("error", "modelo caído"))
        finally:
            s.close()

    def test_long_reply_is_cut_for_glasses(self):
        s = Server(FakeGateway(text="z" * 5000))
        try:
            _, d = s.json("POST", "/api/g2/ask", {"text": "a"})
            self.assertEqual(len(d["text"]), b.REPLY_MAX)
        finally:
            s.close()

    def test_pending_then_answer(self):
        gate = threading.Event()

        class Slow(FakeGateway):
            def __call__(self, req):
                gate.wait(5)
                return FakeGateway.__call__(self, req)
        old = b.ASK_WAIT
        b.ASK_WAIT = 0
        s = Server(Slow())
        try:
            _, d = s.json("POST", "/api/g2/ask", {"text": "a", "cid": "slow1"})
            self.assertEqual(d["status"], "pending")
            gate.set()
            _, d = s.json("GET", "/api/g2/answer?id=slow1&wait=5")
            self.assertEqual((d["status"], d["text"]), ("done", "Hola Alan"))
            self.assertEqual(s.json("GET", "/api/g2/answer?id=nope")[0], 404)
        finally:
            b.ASK_WAIT = old
            s.close()

    def test_uncollected_answer_is_delivered_once_after_grace(self):
        gate = threading.Event()
        gw = FakeGateway()
        orig = gw.__call__

        class G(FakeGateway):
            def __call__(inner, req):
                if req["op"] == "send":
                    gate.wait(5)
                return FakeGateway.__call__(inner, req)
        old = b.ASK_WAIT
        b.ASK_WAIT = 0
        s = Server(G())
        try:
            s.json("POST", "/api/g2/ask", {"text": "a", "cid": "lost1"})
            gate.set()
            s.jobs.wait("lost1", 5)
            self.assertEqual(s.jobs.sweep(), [])  # todavía hay margen
            s.clock.t += b.GRACE + 1
            self.assertEqual(s.jobs.sweep(), ["lost1"])
            self.assertEqual(s.jobs.sweep(), [])  # una sola vez
            self.assertIn({"op": "deliver", "cid": "lost1"}, s.gw.calls)
        finally:
            b.ASK_WAIT = old
            s.close()

    def test_collected_answer_is_never_redelivered(self):
        self.s.json("POST", "/api/g2/ask", {"text": "a", "cid": "ok1"})
        self.s.clock.t += b.GRACE + 1
        self.assertEqual(self.s.jobs.sweep(), [])
        self.assertNotIn("deliver", [c["op"] for c in self.s.gw.calls])

    def test_openai_endpoint(self):
        code, d = self.s.json("POST", "/v1/chat/completions", {
            "model": "jinx", "messages": [{"role": "system", "content": "s"},
                                          {"role": "user", "content": [{"type": "text", "text": "hola"}]}]})
        self.assertEqual(code, 200)
        self.assertEqual(d["choices"][0]["message"], {"role": "assistant", "content": "Hola Alan"})
        self.assertEqual(self.s.gw.calls[0]["text"], "hola")
        self.assertEqual(self.s.json("POST", "/v1/chat/completions", {"messages": []})[0], 400)
        self.assertEqual(self.s.json("GET", "/v1/models")[1]["data"][0]["id"], "jinx")

    def test_openai_stream(self):
        code, raw, h = self.s.call("POST", "/v1/chat/completions",
                                   {"stream": True, "messages": [{"role": "user", "content": "hola"}]})
        self.assertEqual(code, 200)
        self.assertIn("text/event-stream", h["Content-Type"])
        text = raw.decode()
        self.assertIn('"content": "Hola Alan"', text)
        self.assertTrue(text.rstrip().endswith("data: [DONE]"))

    def test_openai_pending_says_still_thinking(self):
        gate = threading.Event()

        class Slow(FakeGateway):
            def __call__(self, req):
                gate.wait(5)
                return FakeGateway.__call__(self, req)
        old = b.OPENAI_WAIT
        b.OPENAI_WAIT = 0
        s = Server(Slow())
        try:
            _, d = s.json("POST", "/v1/chat/completions", {"messages": [{"role": "user", "content": "hola"}]})
            self.assertEqual(d["choices"][0]["message"]["content"], b.STILL_THINKING)
        finally:
            gate.set()
            b.OPENAI_WAIT = old
            s.close()


class VoiceAndNoticesTest(unittest.TestCase):
    def test_voice_runs_stt_then_asks(self):
        s = Server(FakeGateway(), stt=lambda pcm: "dime algo")
        try:
            code, d = s.json("POST", "/api/g2/voice", raw=b"\0" * 20000)
            self.assertEqual((code, d["heard"], d["status"]), (200, "dime algo", "done"))
            self.assertEqual(s.gw.calls[0]["text"], "dime algo")
        finally:
            s.close()

    def test_voice_errors(self):
        def stt(pcm):
            raise g2_stt.SttUnavailable("no hay motor")
        s = Server(FakeGateway(), stt=stt)
        try:
            self.assertEqual(s.json("POST", "/api/g2/voice", raw=b"\0" * 20000)[0], 503)
        finally:
            s.close()
        s = Server(FakeGateway(), stt=lambda pcm: "")
        try:
            self.assertEqual(s.json("POST", "/api/g2/voice", raw=b"\0" * 20000)[1]["status"], "empty")
            self.assertEqual(s.gw.calls, [])  # nada entendido: Jinx no se entera
        finally:
            s.close()

    def test_notices_hide_chat_replies_and_never_ack(self):
        calls = []

        def fetch(path):
            calls.append(path)
            return {"messages": [
                {"id": 4, "text": "Claude Code terminó", "kind": "info", "source": "agentes", "ts": 1000},
                {"id": 5, "text": "respuesta a otra", "kind": "info", "source": "jinx-chat", "ts": 1000},
                {"id": "x", "text": "roto"}]}
        found = b.notices_since(3, fetch, now=lambda: 1010)
        self.assertEqual([n["id"] for n in found], [4])
        # solo GET del inbox, y con all=1: los que otro dispositivo ya aceptó también cuentan
        self.assertEqual(calls, ["/api/mochi/inbox?since_id=3&all=1"])

    def test_old_notices_are_not_shown(self):
        fetch = lambda path: {"messages": [{"id": 1, "text": "viejo", "ts": 100}, {"id": 2, "text": "nuevo", "ts": 5000},
                                          {"id": 3, "text": "sin hora"}]}
        found = b.notices_since(0, fetch, now=lambda: 5100)
        self.assertEqual([n["id"] for n in found], [2, 3])

    def test_notices_endpoint(self):
        s = Server(FakeGateway(), notices=lambda since: [{"id": since + 1, "text": "hola"}])
        try:
            self.assertEqual(s.json("GET", "/api/g2/notices?since=7")[1], {"notices": [{"id": 8, "text": "hola"}]})
        finally:
            s.close()

    def test_notices_survive_presence_down(self):
        def boom(since):
            raise OSError("caído")
        s = Server(FakeGateway(), notices=boom)
        try:
            self.assertEqual(s.json("GET", "/api/g2/notices?since=0")[1], {"notices": []})
        finally:
            s.close()


class SttTest(unittest.TestCase):
    PCM = b"\0\1" * 16000

    def test_checks_audio(self):
        with self.assertRaises(g2_stt.SttBadAudio):
            g2_stt.check_pcm(b"\0" * 10)
        with self.assertRaises(g2_stt.SttBadAudio):
            g2_stt.check_pcm(b"\0" * (g2_stt.MAX_BYTES + 2))
        with self.assertRaises(g2_stt.SttBadAudio):
            g2_stt.check_pcm(b"\0" * 20001)

    def test_command_engine(self):
        seen = {}

        class R:
            returncode = 0
            stdout = "  hola   mundo \n"

        def run(argv, **kw):
            seen["argv"] = argv
            seen["exists"] = os.path.exists(argv[-1])
            return R()
        out = g2_stt.transcribe(self.PCM, env={"JINX_G2_STT_CMD": "stt -l es {wav}"}, run=run)
        self.assertEqual(out, "hola mundo")
        self.assertEqual(seen["argv"][:3], ["stt", "-l", "es"])
        self.assertTrue(seen["exists"])

    def test_no_engine_is_a_clear_error(self):
        with self.assertRaises(g2_stt.SttUnavailable):
            g2_stt.transcribe(self.PCM, env={}, whisper_loader=lambda n: g2_stt._load_faster_whisper("nada"))

    def test_whisper_engine(self):
        class Seg:
            def __init__(self, t):
                self.text = t

        class M:
            def transcribe(self, wav, **kw):
                return [Seg(" hola"), Seg("mundo ")], None
        self.assertEqual(g2_stt.transcribe(self.PCM, env={}, whisper_loader=lambda n: M()), "hola mundo")


class KeyTest(unittest.TestCase):
    def test_key_is_created_0600_and_stable(self):
        import tempfile
        p = os.path.join(tempfile.mkdtemp(), "sub", "key")
        k = b.make_key(p)
        self.assertGreaterEqual(len(k), 32)
        self.assertEqual(oct(os.stat(p).st_mode & 0o777), "0o600")
        self.assertEqual(b.make_key(p), k)


if __name__ == "__main__":
    unittest.main()
