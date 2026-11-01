import json
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(__file__))
import g2_bridge as b
import g2_lite as L


class FakeLLM:
    def __init__(self, script):
        self.script = list(script)
        self.calls = []

    def chat(self, messages, tools):
        self.calls.append([dict(m) for m in messages])
        return self.script.pop(0)


def call(name, **args):
    return {"id": "t1", "type": "function", "function": {"name": name, "arguments": json.dumps(args)}}


class FakeHA:
    def __init__(self):
        self.done = []

    def find(self, q, dom=None):
        return [{"entity_id": "light.salon", "name": "Luz salón", "state": "off"}]

    def state(self, e):
        return {"entity_id": e, "state": "off"}

    def call(self, d, s, e, data=None):
        ok, clean = L.check_call(d, s, e, data)
        if not ok:
            raise L.HaError(clean)
        self.done.append((d, s, e, clean))
        return "hecho"


class PolicyTest(unittest.TestCase):
    def test_allowed_calls(self):
        self.assertEqual(L.check_call("light", "turn_on", "light.salon", {"brightness_pct": 40}),
                         (True, {"brightness_pct": 40}))
        self.assertTrue(L.check_call("climate", "set_temperature", "climate.sala", {"temperature": 22})[0])
        self.assertTrue(L.check_call("light", "turn_on", "light.x", {"rgb_color": [1, 2, 3]})[0])

    def test_sensitive_domains_and_services_are_refused(self):
        for dom, svc in (("lock", "unlock"), ("alarm_control_panel", "alarm_disarm"), ("cover", "open_cover"),
                         ("automation", "trigger"), ("script", "turn_on"), ("homeassistant", "restart"),
                         ("shell_command", "x"), ("light", "reload")):
            ok, why = L.check_call(dom, svc, dom + ".x", {})
            self.assertFalse(ok, (dom, svc))

    def test_entity_must_be_one_of_the_domain(self):
        for e in ("all", "light.a,light.b", "switch.x", "light.", "light.a b", None, "light.UPPER", ["light.a"]):
            self.assertFalse(L.check_call("light", "turn_on", e, {})[0], e)

    def test_data_is_bounded_and_known(self):
        for d in ({"brightness_pct": 101}, {"brightness_pct": True}, {"brightness_pct": "50"}, {"rgb_color": [1, 2]},
                  {"rgb_color": [1, 2, 300]}, {"entity_id": "all"}, {"script": "x"}):
            self.assertFalse(L.check_call("light", "turn_on", "light.x", d)[0], d)
        self.assertFalse(L.check_call("climate", "set_temperature", "climate.x", {"temperature": 40})[0])


class HaTest(unittest.TestCase):
    def make(self, states):
        self.sent = []
        t = [0.0]

        def opener(req, timeout=0):
            self.sent.append((req.get_method(), req.full_url, req.data, req.headers.get("Authorization")))

            class R:
                def __enter__(s):
                    return s

                def __exit__(s, *a):
                    pass

                def read(s):
                    return json.dumps(states if req.get_method() == "GET" else []).encode()
            return R()
        self.t = t
        return L.HomeAssistant("http://ha:8123/", lambda: "TOK", opener, now=lambda: t[0])

    STATES = [
        {"entity_id": "light.salon", "state": "off", "attributes": {"friendly_name": "Luz del Salón"}},
        {"entity_id": "lock.puerta", "state": "locked", "attributes": {"friendly_name": "Puerta"}},
        {"entity_id": "sensor.temp", "state": "21", "attributes": {"friendly_name": "Temperatura sala"}},
        {"entity_id": "automation.x", "state": "on", "attributes": {"friendly_name": "Auto"}},
    ]

    def test_find_ignores_accents_and_hides_unrelated_domains(self):
        ha = self.make(self.STATES)
        self.assertEqual([e["entity_id"] for e in ha.find("luz salon")], ["light.salon"])
        self.assertEqual([e["entity_id"] for e in ha.find("puerta")], ["lock.puerta"])  # readable, not controllable
        self.assertEqual(ha.find("auto"), [])

    def test_states_are_cached_for_30_s(self):
        ha = self.make(self.STATES)
        ha.find("luz")
        ha.find("luz")
        self.assertEqual(len(self.sent), 1)
        self.t[0] = 31
        ha.find("luz")
        self.assertEqual(len(self.sent), 2)

    def test_call_posts_the_checked_payload_with_the_token(self):
        ha = self.make(self.STATES)
        ha.call("light", "turn_on", "light.salon", {"brightness_pct": 30})
        m, url, data, auth = self.sent[-1]
        self.assertEqual((m, url, auth), ("POST", "http://ha:8123/api/services/light/turn_on", "Bearer TOK"))
        self.assertEqual(json.loads(data), {"brightness_pct": 30, "entity_id": "light.salon"})

    def test_refused_call_never_reaches_the_network(self):
        ha = self.make(self.STATES)
        with self.assertRaises(L.HaError):
            ha.call("lock", "unlock", "lock.puerta")
        self.assertEqual(self.sent, [])

    def test_no_token_is_a_clear_error(self):
        ha = L.HomeAssistant("http://ha", lambda: "")
        with self.assertRaises(L.HaError):
            ha.states()


class LiteTest(unittest.TestCase):
    def make(self, script, now=None):
        self.llm = FakeLLM(script)
        self.ha = FakeHA()
        self.handed = []
        return L.Lite(self.llm, self.ha, lambda t: self.handed.append(t) or "pasado", now=now or (lambda: 100.0))

    def test_plain_answer_is_cleaned_and_short(self):
        lite = self.make([{"content": "**Hola** Alan, `listo`."}])
        self.assertEqual(lite.reply("hola"), "Hola Alan, listo.")
        self.assertEqual(self.llm.calls[0][0]["role"], "system")

    def test_uses_tools_then_answers(self):
        lite = self.make([
            {"content": "", "tool_calls": [call("ha_buscar", consulta="luz salon")]},
            {"content": "", "tool_calls": [call("ha_accion", dominio="light", servicio="turn_on",
                                                entity_id="light.salon", datos={"brightness_pct": 50})]},
            {"content": "Luz del salón al 50%."}])
        self.assertEqual(lite.reply("pon la luz del salón al 50"), "Luz del salón al 50%.")
        self.assertEqual(self.ha.done, [("light", "turn_on", "light.salon", {"brightness_pct": 50})])
        last = self.llm.calls[-1]
        self.assertEqual(last[-1]["role"], "tool")

    def test_a_refused_action_comes_back_to_the_model_as_an_error(self):
        lite = self.make([
            {"content": "", "tool_calls": [call("ha_accion", dominio="lock", servicio="unlock", entity_id="lock.puerta")]},
            {"content": "La cerradura la abres tú a mano."}])
        self.assertIn("a mano", lite.reply("abre la puerta"))
        self.assertEqual(self.ha.done, [])
        self.assertTrue(self.llm.calls[-1][-1]["content"].startswith("error:"))

    def test_hand_off(self):
        lite = self.make([{"content": "", "tool_calls": [call("pasar_a_jinx", peticion="busca mi correo")]},
                          {"content": "Se lo paso a Jinx."}])
        lite.reply("busca mi correo")
        self.assertEqual(self.handed, ["busca mi correo"])

    def test_broken_tool_arguments_do_not_crash(self):
        bad = {"id": "x", "function": {"name": "ha_estado", "arguments": "{no json"}}
        lite = self.make([{"content": "", "tool_calls": [bad]}, {"content": "no sé"}])
        self.assertEqual(lite.reply("x"), "no sé")

    def test_endless_tool_calls_stop(self):
        lite = self.make([{"content": "", "tool_calls": [call("ha_buscar", consulta="x")]}] * 10)
        with self.assertRaises(L.LiteError):
            lite.reply("x")
        self.assertEqual(len(self.llm.calls), L.MAX_ROUNDS)

    def test_short_memory_that_expires(self):
        t = [100.0]
        lite = self.make([{"content": "uno"}, {"content": "dos"}, {"content": "tres"}], now=lambda: t[0])
        lite.reply("a")
        lite.reply("b")
        self.assertEqual([m["content"] for m in self.llm.calls[1] if m["role"] != "system"], ["a", "uno", "b"])
        t[0] += L.HISTORY_IDLE + 1
        lite.reply("c")
        self.assertEqual([m["content"] for m in self.llm.calls[2] if m["role"] != "system"], ["c"])

    def test_history_is_bounded(self):
        lite = self.make([{"content": "r"}] * 20)
        for i in range(20):
            lite.reply("m%d" % i)
        self.assertLessEqual(len(lite.history), L.HISTORY_TURNS)


class BridgeLiteTest(unittest.TestCase):
    def jobs(self, lite, gw=None):
        calls = []

        def gateway(req):
            calls.append(req)
            if req["op"] == "send":
                return [{"ev": "done", "mode": "hermes", "text": "larga", "mirrored": True, "held": True}]
            return [{"ev": "delivered"}]
        j = b.Jobs(gw or gateway, lite=lite, spawn=lambda fn: fn())
        j.calls = calls
        return j

    def test_lite_answers_without_touching_the_shared_thread(self):
        class Lite:
            def reply(self, text):
                return "rápida"
        j = self.jobs(Lite())
        cid = j.submit("hola", None, "lite")
        self.assertEqual(j.wait(cid, 1)["text"], "rápida")
        self.assertEqual(j.calls, [])
        j.now = lambda: 10 ** 9
        self.assertEqual(j.sweep(), [])  # nothing held, nothing to deliver

    def test_lite_failure_falls_back_to_the_full_jinx(self):
        class Lite:
            def reply(self, text):
                raise L.LiteError("caído")
        j = self.jobs(Lite())
        cid = j.submit("hola", None, "lite")
        r = j.wait(cid, 1)
        self.assertEqual((r["text"], r["mode"]), ("larga", "hermes"))

    def test_no_lite_configured_uses_the_full_jinx(self):
        j = self.jobs(None)
        self.assertEqual(j.wait(j.submit("hola", None, "lite"), 1)["text"], "larga")

    def test_hand_off_goes_to_hermes_and_is_delivered_at_once(self):
        j = self.jobs(None)
        msg = j.hand_off("busca mi correo")
        self.assertIn("Pasado a Jinx", msg)
        ops = [c["op"] for c in j.calls]
        self.assertEqual(ops, ["send", "deliver"])
        self.assertEqual(j.calls[0]["src"], "g2")


if __name__ == "__main__":
    unittest.main()
