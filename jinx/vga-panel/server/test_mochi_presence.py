import importlib.util
import json
import os
import threading
import time
import unittest
import urllib.error
import urllib.request
from http.server import ThreadingHTTPServer

HERE = os.path.dirname(os.path.abspath(__file__))


def load():
    spec = importlib.util.spec_from_file_location("mochi_presence_under_test", os.path.join(HERE, "mochi_presence.py"))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    mod.STATE_PATH = type(mod.STATE_PATH)("/tmp/mochi_presence_test_state.json")   # no tocar el estado real
    mod.state["presence"].clear()
    mod.state["inbox"].clear()
    return mod


class DevicesTest(unittest.TestCase):
    def test_phones_s21_and_s22_and_the_pc_are_known_and_vga_is_the_failsafe(self):
        m = load()
        self.assertEqual(set(m.DEVICES), {"pc", "s21", "s22"})

    def test_the_vga_panel_is_active_when_nobody_else_is_looking(self):
        m = load()
        self.assertEqual(m.active_device(time.time()), "vga")

    def test_the_most_recently_touched_screen_that_is_on_wins(self):
        m = load()
        now = time.time()
        m.state["presence"]["pc"] = {"idle_seconds": 120, "screen_on": True, "ts": now}
        m.state["presence"]["s22"] = {"idle_seconds": 4, "screen_on": True, "ts": now}
        self.assertEqual(m.active_device(now), "s22")

    def test_a_phone_with_its_screen_off_or_long_idle_does_not_count(self):
        m = load()
        now = time.time()
        m.state["presence"]["s22"] = {"idle_seconds": 2, "screen_on": False, "ts": now}
        m.state["presence"]["s21"] = {"idle_seconds": m.IDLE_LIMIT + 5, "screen_on": True, "ts": now}
        self.assertEqual(m.active_device(now), "vga")

    def test_a_silent_device_is_dropped_after_the_timeout(self):
        m = load()
        now = time.time()
        m.state["presence"]["s22"] = {"idle_seconds": 1, "screen_on": True, "ts": now - m.HEARTBEAT_TIMEOUT - 1}
        self.assertEqual(m.active_device(now), "vga")


class HttpTest(unittest.TestCase):
    def setUp(self):
        self.m = load()
        self.srv = ThreadingHTTPServer(("127.0.0.1", 0), self.m.Handler)
        self.port = self.srv.server_address[1]
        self.m.PORT = self.port        # en producción el puerto configurado es el enlazado
        threading.Thread(target=self.srv.serve_forever, daemon=True).start()

    def tearDown(self):
        self.srv.shutdown()

    def post(self, path, body):
        req = urllib.request.Request("http://127.0.0.1:%d%s" % (self.port, path), data=json.dumps(body).encode(),
                                     headers={"Content-Type": "application/json"})
        try:
            with urllib.request.urlopen(req) as r:
                return r.status, json.loads(r.read())
        except urllib.error.HTTPError as e:
            return e.code, json.loads(e.read())

    def get(self, path):
        with urllib.request.urlopen("http://127.0.0.1:%d%s" % (self.port, path)) as r:
            return json.loads(r.read())

    def test_the_s22_can_send_heartbeats_and_becomes_active(self):
        code, out = self.post("/api/presence/heartbeat", {"device": "s22", "idle_seconds": 1, "screen_on": True})
        self.assertEqual(code, 200)
        self.assertEqual(out["active_device"], "s22")

    def test_an_unknown_device_is_refused_with_the_list_of_known_ones(self):
        code, out = self.post("/api/presence/heartbeat", {"device": "tablet", "idle_seconds": 1})
        self.assertEqual(code, 400)
        self.assertIn("s22", out["error"])

    def test_state_lists_every_known_device(self):
        self.post("/api/presence/heartbeat", {"device": "s22", "idle_seconds": 1, "screen_on": True})
        st = self.get("/api/presence/state")
        for d in ("pc", "s21", "s22"):
            self.assertIn(d, st)
        self.assertEqual(st["active_device"], "s22")
        self.assertIsNotNone(st["s22"])

    def test_inbox_reports_the_active_device_the_panel_depends_on(self):
        self.post("/api/mochi/message", {"text": "hola", "kind": "info", "source": "test"})
        out = self.get("/api/mochi/inbox?since_id=0")
        self.assertEqual(out["active_device"], "vga")
        self.assertEqual(len(out["messages"]), 1)


class HostAndCorsTest(unittest.TestCase):
    def setUp(self):
        self.m = load()
        self.srv = ThreadingHTTPServer(("127.0.0.1", 0), self.m.Handler)
        self.port = self.srv.server_address[1]
        self.m.PORT = self.port
        threading.Thread(target=self.srv.serve_forever, daemon=True).start()

    def tearDown(self):
        self.srv.shutdown()

    def request(self, host, origin=None, path="/api/mochi/inbox?since_id=0"):
        import http.client
        c = http.client.HTTPConnection("127.0.0.1", self.port)
        h = {"Host": host}
        if origin:
            h["Origin"] = origin
        c.request("GET", path, headers=h)
        r = c.getresponse()
        return r.status, r.getheader("Access-Control-Allow-Origin")

    def test_a_foreign_host_header_is_refused(self):
        self.assertEqual(self.request("evil.example:%d" % self.port)[0], 421)
        self.assertEqual(self.request("127.0.0.1.evil.example:%d" % self.port)[0], 421)

    def test_loopback_and_tailnet_hosts_are_served(self):
        self.assertEqual(self.request("127.0.0.1:%d" % self.port)[0], 200)
        self.assertEqual(self.request("localhost:%d" % self.port)[0], 200)
        self.assertEqual(self.request("%s:%d" % (self.m.BIND_TAILNET, self.port))[0], 200)

    def test_cors_is_only_for_the_panels_own_origin(self):
        _, allow = self.request("127.0.0.1:%d" % self.port, origin="http://127.0.0.1:8896")
        self.assertEqual(allow, "http://127.0.0.1:8896")
        _, allow = self.request("127.0.0.1:%d" % self.port, origin="https://evil.example")
        self.assertIsNone(allow)
        _, allow = self.request("127.0.0.1:%d" % self.port)          # clientes que no son navegador
        self.assertIsNone(allow)

    def post_with(self, headers):
        import http.client
        c = http.client.HTTPConnection("127.0.0.1", self.port)
        c.request("POST", "/api/mochi/message", body=b'{"text":"hola","kind":"info"}',
                  headers={"Host": "127.0.0.1:%d" % self.port, **headers})
        return c.getresponse().status

    def test_a_cross_site_post_is_refused_even_from_loopback(self):
        # CSRF: una pagina web puede mandar un POST "simple" (text/plain) a 127.0.0.1; el navegador siempre manda Origin.
        self.assertEqual(self.post_with({"Origin": "https://evil.example", "Content-Type": "text/plain"}), 403)
        self.assertEqual(len(self.m.state["inbox"]), 0)

    def test_the_panel_origin_and_non_browser_clients_can_post(self):
        self.assertEqual(self.post_with({"Origin": "http://127.0.0.1:8896", "Content-Type": "application/json"}), 200)
        self.assertEqual(self.post_with({"Content-Type": "application/json"}), 200)


if __name__ == "__main__":
    unittest.main()
