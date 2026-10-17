import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(__file__))
from jinx_thread import Thread
import jinx_chat
from jinx_chat import Deps, handle, run_line


class FakeBrain:
    def __init__(self, text="respuesta", fail=False):
        self.calls = []
        self.text = text
        self.fail = fail

    def reply(self, messages):
        self.calls.append(messages)
        if self.fail:
            raise RuntimeError("modelo caído")
        return "or", self.text


class FakeMirror:
    def __init__(self, ok=True):
        self.ok = ok
        self.sent = []

    def send(self, text):
        self.sent.append(text)
        return self.ok


class GatewayTest(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.thread = Thread(self.dir.name)
        self.brain = FakeBrain()
        self.mirror = FakeMirror()
        self.deps = Deps(self.thread, self.brain, self.mirror, "CORE", "MEM",
                         lambda: {"openrouter_key": "k", "telegram_token": "t", "chat": "42"},
                         self.dir.name)

    def tearDown(self):
        self.dir.cleanup()

    def events(self, req):
        return list(handle(req, self.deps))

    def test_send_answers_stores_both_lines_and_mirrors(self):
        ev = self.events({"op": "send", "text": "hola", "cid": "c1", "src": "s21"})
        self.assertEqual(ev[0], {"ev": "ack"})
        self.assertEqual(ev[-1], {"ev": "done", "mode": "or", "text": "respuesta", "mirrored": True})
        ids = [d["id"] for d in self.thread.read()]
        self.assertEqual(ids, ["c1:s21", "c1:jinx"])
        self.assertEqual(self.mirror.sent, ["hola", "⚠️ respuesta"])
        self.assertEqual(self.thread.unmirrored(), [])

    def test_a_reply_from_hermes_goes_to_telegram_without_a_mark(self):
        self.brain.reply = lambda messages: ("hermes", "la real")
        self.events({"op": "send", "text": "hola", "cid": "c9", "src": "s22"})
        self.assertEqual(self.mirror.sent, ["hola", "la real"])
        self.assertNotIn("via", self.thread.read()[-1])

    def test_the_backup_mark_survives_a_merge(self):
        from jinx_thread import Thread
        other = Thread(tempfile.mkdtemp())
        other.merge([{"id": "x:jinx", "ts": 1.0, "src": "jinx", "text": "t", "cid": "x", "via": "or"},
                     {"id": "y:jinx", "ts": 2.0, "src": "jinx", "text": "t", "cid": "y", "via": "bogus"}])
        self.assertEqual([d.get("via") for d in other.read()], ["or", None])

    def test_someone_watching_gets_the_reply_on_their_island_not_telegram(self):
        got = []
        self.deps.presence = lambda t, k, src: got.append((t, k, src)) or True
        self.brain.reply = lambda messages: ("hermes", "la real")
        ev = self.events({"op": "send", "text": "hola", "cid": "c7", "src": "s22"})
        self.assertEqual(got, [("la real", "info", "jinx-chat")])
        self.assertEqual(self.mirror.sent, [])
        self.assertTrue(ev[-1]["mirrored"])
        self.assertEqual(self.thread.unmirrored(), [])  # handled: never repeated to Telegram later

    def test_nobody_watching_or_no_presence_sends_it_to_telegram(self):
        self.deps.presence = lambda t, k, src: False
        self.brain.reply = lambda messages: ("hermes", "la real")
        self.events({"op": "send", "text": "hola", "cid": "c8", "src": "s22"})
        self.assertEqual(self.mirror.sent, ["hola", "la real"])

    def test_a_backup_reply_carries_only_the_warning_emoji(self):
        got = []
        self.deps.presence = lambda t, k, src: got.append(t) or True
        self.events({"op": "send", "text": "hola", "cid": "c6", "src": "s22"})
        self.assertEqual(got, ["⚠️ respuesta"])

    def test_same_cid_twice_does_not_call_the_model_again(self):
        self.events({"op": "send", "text": "hola", "cid": "c1", "src": "s21"})
        ev = self.events({"op": "send", "text": "hola", "cid": "c1", "src": "s21"})
        self.assertEqual(len(self.brain.calls), 1)
        self.assertEqual(ev[-1], {"ev": "done", "mode": "cached", "text": "respuesta", "mirrored": True})
        self.assertEqual(len(self.thread.read()), 2)

    def test_context_given_to_the_model_includes_telegram_lines(self):
        self.thread.append("tg", "dime algo desde telegram", ts=1.0, mirrored=True)
        self.events({"op": "send", "text": "y ahora", "cid": "c2", "src": "s21"})
        sent = self.brain.calls[0]
        self.assertEqual([m["content"] for m in sent], ["dime algo desde telegram", "y ahora"])

    def test_model_failure_gives_error_and_no_jinx_line(self):
        self.brain.fail = True
        ev = self.events({"op": "send", "text": "hola", "cid": "c1", "src": "s21"})
        self.assertEqual(ev[-1]["ev"], "error")
        self.assertIn("modelo caído", ev[-1]["msg"])
        self.assertEqual([d["id"] for d in self.thread.read()], ["c1:s21"])

    def test_mirror_failure_still_answers_and_leaves_lines_unmirrored(self):
        self.mirror.ok = False
        ev = self.events({"op": "send", "text": "hola", "cid": "c1", "src": "s21"})
        self.assertEqual(ev[-1]["ev"], "done")
        self.assertEqual(len(self.thread.unmirrored()), 2)

    def test_next_send_mirrors_what_was_left_behind(self):
        self.mirror.ok = False
        self.events({"op": "send", "text": "uno", "cid": "c1", "src": "s21"})
        self.mirror.ok = True
        self.mirror.sent.clear()
        self.events({"op": "send", "text": "dos", "cid": "c2", "src": "s21"})
        self.assertEqual(self.thread.unmirrored(), [])
        self.assertEqual(self.mirror.sent[:2], ["uno", "⚠️ respuesta"])

    def test_telegram_text_is_cut_but_thread_keeps_it_whole(self):
        long = "x" * 10000
        self.events({"op": "send", "text": long, "cid": "c1", "src": "s21"})
        self.assertEqual(len(self.mirror.sent[0]), 4000)
        self.assertEqual(self.thread.read()[0]["text"], long)

    def test_send_without_text_or_cid_is_an_error(self):
        self.assertEqual(self.events({"op": "send", "cid": "c"})[-1]["ev"], "error")
        self.assertEqual(self.events({"op": "send", "text": "x"})[-1]["ev"], "error")
        self.assertEqual(self.thread.read(), [])

    def test_the_pc_writes_to_the_same_thread_and_is_mirrored_as_the_pc(self):
        ev = self.events({"op": "send", "text": "desde el escritorio", "cid": "p1", "src": "pc"})
        self.assertEqual(ev[-1]["ev"], "done")
        self.assertEqual([d["src"] for d in self.thread.read()], ["pc", "jinx"])
        self.assertEqual(self.mirror.sent, ["desde el escritorio", "⚠️ respuesta"])

    def test_unknown_src_is_rejected(self):
        ev = self.events({"op": "send", "text": "x", "cid": "c", "src": "evil"})
        self.assertEqual(ev[-1]["ev"], "error")

    def test_history_returns_last_n(self):
        for i in range(5):
            self.thread.append("tg", "m%d" % i, ts=float(i))
        ev = self.events({"op": "history", "n": 2})
        self.assertEqual([d["text"] for d in ev[0]["items"]], ["m3", "m4"])

    def test_merge_adds_lines_returns_missing_and_mirrors_unmirrored(self):
        self.thread.append("tg", "de telegram", ts=1.0, mirrored=True)
        incoming = [{"id": "c9:s22", "ts": 5.0, "src": "s22", "text": "sin red",
                     "cid": "c9", "mirrored": False}]
        ev = self.events({"op": "merge", "lines": incoming, "have": []})[-1]
        self.assertEqual(ev["ev"], "merged")
        self.assertEqual(ev["added"], 1)
        self.assertEqual([d["text"] for d in ev["items"]], ["de telegram", "sin red"])
        self.assertEqual(ev["mirrored_ids"], ["c9:s22"])
        self.assertEqual(self.mirror.sent, ["sin red"])

    def test_concurrent_sends_of_the_same_cid_call_the_model_once(self):
        import threading
        import time

        class SlowBrain(FakeBrain):
            def reply(self, messages):
                time.sleep(0.3)
                return super().reply(messages)

        self.deps.brain = SlowBrain()
        results = []

        def work():
            results.append(list(handle({"op": "send", "text": "hola", "cid": "c1", "src": "s21"}, self.deps)))

        ts = [threading.Thread(target=work) for _ in range(2)]
        [t.start() for t in ts]
        [t.join() for t in ts]
        self.assertEqual(len(self.deps.brain.calls), 1)
        self.assertEqual([r[-1]["text"] for r in results], ["respuesta", "respuesta"])
        self.assertEqual(len(self.thread.read()), 2)

    def test_done_reports_whether_the_lines_reached_telegram(self):
        ev = self.events({"op": "send", "text": "a", "cid": "c1", "src": "s21"})
        self.assertIs(ev[-1]["mirrored"], True)
        self.mirror.ok = False
        ev = self.events({"op": "send", "text": "b", "cid": "c2", "src": "s21"})
        self.assertIs(ev[-1]["mirrored"], False)
        ev = self.events({"op": "send", "text": "b", "cid": "c2", "src": "s21"})
        self.assertEqual(ev[-1]["mode"], "cached")
        self.assertIs(ev[-1]["mirrored"], False)

    def test_merge_reports_every_sent_line_that_is_mirrored_even_if_not_by_this_call(self):
        self.thread.append("s22", "ya espejada", cid="c9", ts=5.0, mirrored=True)
        incoming = [{"id": "c9:s22", "ts": 5.0, "src": "s22", "text": "ya espejada",
                     "cid": "c9", "mirrored": False}]
        ev = self.events({"op": "merge", "lines": incoming, "have": []})[-1]
        self.assertEqual(ev["added"], 0)
        self.assertEqual(ev["mirrored_ids"], ["c9:s22"])

    def test_deferred_mirror_answers_without_waiting_for_telegram(self):
        deferred = []
        self.deps.defer_mirror = lambda: deferred.append(1)
        ev = self.events({"op": "send", "text": "hola", "cid": "c1", "src": "s21"})
        self.assertEqual(ev[-1]["ev"], "done")
        self.assertEqual(self.mirror.sent, [])          # Telegram no se tocó antes de contestar
        self.assertEqual(deferred, [1])                  # se delegó el espejo
        self.assertIs(ev[-1]["mirrored"], True)          # el gateway se hace responsable y reintenta
        self.assertEqual(len(self.thread.unmirrored()), 2)

    def test_core_returns_persona_and_memory(self):
        self.assertEqual(self.events({"op": "core"})[-1],
                         {"ev": "core", "core": "CORE", "memory": "MEM"})

    def test_provision_is_one_shot_and_gated_by_flag_file(self):
        self.assertEqual(self.events({"op": "provision"})[-1]["ev"], "error")
        flag = os.path.join(self.dir.name, "ALLOW_PROVISION")
        open(flag, "w").close()
        ev = self.events({"op": "provision"})[-1]
        self.assertEqual(ev, {"ev": "secrets", "openrouter_key": "k",
                              "telegram_token": "t", "chat": "42"})
        self.assertFalse(os.path.exists(flag))
        self.assertEqual(self.events({"op": "provision"})[-1]["ev"], "error")

    def test_run_line_survives_garbage(self):
        for raw in ["", "   ", "{no", "[1]", '{"op":"nope"}', '{"nop":1}']:
            out = list(run_line(raw, self.deps))
            self.assertEqual(len(out), 1, raw)
            self.assertEqual(out[0]["ev"], "error", raw)
        self.assertEqual(self.thread.read(), [])


class _FakeJx:
    """Lo mínimo del módulo ~/bin/jinx que usa JinxBrain."""
    PREAMBLE = {"or": "PRE-OR", "local": "PRE-LOCAL"}
    CORE_PATH = "/nonexistent-core"
    SOUL_PATH = __file__            # cualquier archivo legible

    def __init__(self, or_fails=False):
        self.probed = 0
        self.or_fails = or_fails

    def load_synced_memory(self):
        return "\n[mem]", "ok"

    def get_api_key(self):
        return "KEY"

    def openrouter_reachable(self, key):
        self.probed += 1
        return True

    def stream_openrouter(self, msgs, key):
        if self.or_fails:
            raise OSError("sin red")
        yield "hola "
        yield "mundo"

    def pick_local_backend(self):
        pass

    def stream_ollama(self, msgs):
        yield "<local>"

    def strip_think(self, pieces):
        return pieces


class BrainTest(unittest.TestCase):
    def test_goes_straight_to_openrouter_without_a_reachability_probe(self):
        jx = _FakeJx()
        mode, text = jinx_chat.JinxBrain(jx).reply([{"role": "user", "content": "x"}])
        self.assertEqual((mode, text), ("or", "hola mundo"))
        self.assertEqual(jx.probed, 0)

    def test_falls_back_to_local_when_openrouter_fails(self):
        jx = _FakeJx(or_fails=True)
        mode, text = jinx_chat.JinxBrain(jx).reply([{"role": "user", "content": "x"}])
        self.assertEqual((mode, text), ("local", "<local>"))


class HermesBrainTest(unittest.TestCase):
    def test_sends_only_the_last_user_message_and_returns_the_real_jinx_answer(self):
        seen = []

        def runner(payload):
            seen.append(payload)
            return {"ev": "done", "text": "respuesta real", "session_id": "s"}

        mode, text = jinx_chat.HermesBrain(runner).reply([
            {"role": "user", "content": "uno"}, {"role": "assistant", "content": "dos"},
            {"role": "user", "content": "tres"}])
        self.assertEqual((mode, text), ("hermes", "respuesta real"))
        self.assertEqual(seen, [{"op": "send", "text": "tres"}])

    def test_an_error_event_or_a_dead_runner_raises_so_the_chain_can_fall_back(self):
        with self.assertRaises(RuntimeError):
            jinx_chat.HermesBrain(lambda p: {"ev": "error", "msg": "sin sesión"}).reply([{"role": "user", "content": "x"}])

        def boom(p):
            raise OSError("ssh cayó")

        with self.assertRaises(OSError):
            jinx_chat.HermesBrain(boom).reply([{"role": "user", "content": "x"}])

    def test_history_returns_items_or_nothing(self):
        h = jinx_chat.HermesBrain(lambda p: {"ev": "history", "items": [{"id": "1", "role": "user", "text": "a", "ts": 1.0}]})
        self.assertEqual(len(h.history(10)), 1)
        self.assertEqual(jinx_chat.HermesBrain(lambda p: {"ev": "error", "msg": "x"}).history(10), [])


class ChainBrainTest(unittest.TestCase):
    class Ok:
        def __init__(self, tag):
            self.tag, self.calls = tag, 0

        def reply(self, messages):
            self.calls += 1
            return self.tag, "de " + self.tag

    class Bad:
        def reply(self, messages):
            raise RuntimeError("cae")

    def test_first_that_answers_wins_and_the_rest_are_not_called(self):
        a, b = self.Ok("a"), self.Ok("b")
        self.assertEqual(jinx_chat.ChainBrain([a, b]).reply([]), ("a", "de a"))
        self.assertEqual(b.calls, 0)

    def test_falls_back_in_order(self):
        b = self.Ok("b")
        self.assertEqual(jinx_chat.ChainBrain([self.Bad(), b]).reply([]), ("b", "de b"))

    def test_all_failing_raises_the_last_error(self):
        with self.assertRaises(RuntimeError):
            jinx_chat.ChainBrain([self.Bad(), self.Bad()]).reply([])


class HermesHistoryMergeTest(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.thread = Thread(self.dir.name)
        items = [
            {"id": "10", "role": "user", "text": "hola desde telegram", "ts": 100.0},
            {"id": "11", "role": "assistant", "text": "qué onda", "ts": 101.0},
            {"id": "12", "role": "user", "text": "desde el teléfono", "ts": 200.0},
            {"id": "13", "role": "assistant", "text": "te oigo", "ts": 201.0},
        ]
        self.hermes = type("H", (), {"history": lambda s, n=40: items})()
        self.deps = Deps(self.thread, None, FakeMirror(), "", "", dict, self.dir.name, hermes=self.hermes)

    def tearDown(self):
        self.dir.cleanup()

    def test_history_brings_the_telegram_conversation_and_skips_what_the_phone_already_has(self):
        self.thread.append("s22", "desde el teléfono", cid="c1", ts=200.0)
        self.thread.append("jinx", "te oigo", cid="c1", ts=201.0)
        ev = list(handle({"op": "history", "n": 50}, self.deps))[0]
        texts = [(d["src"], d["text"]) for d in ev["items"]]
        self.assertIn(("tg", "hola desde telegram"), texts)
        self.assertIn(("jinx", "qué onda"), texts)
        self.assertEqual(sum(1 for _, t in texts if t == "desde el teléfono"), 1)   # no duplicate
        self.assertEqual(sum(1 for _, t in texts if t == "te oigo"), 1)

    def test_pulling_twice_adds_nothing_new(self):
        list(handle({"op": "history", "n": 50}, self.deps))
        n = len(self.thread.read())
        list(handle({"op": "history", "n": 50}, self.deps))
        self.assertEqual(len(self.thread.read()), n)

    def test_imported_lines_are_marked_mirrored_so_they_are_not_echoed_to_telegram(self):
        list(handle({"op": "history", "n": 50}, self.deps))
        self.assertEqual(self.thread.unmirrored(), [])

    def test_a_dead_hermes_does_not_break_history(self):
        def boom(s, n=40):
            raise OSError("ssh cayó")

        self.deps.hermes = type("H", (), {"history": boom})()
        self.thread.append("tg", "local", ts=1.0, mirrored=True)
        ev = list(handle({"op": "history", "n": 10}, self.deps))[0]
        self.assertEqual([d["text"] for d in ev["items"]], ["local"])


class RedactionTest(unittest.TestCase):
    TOKEN = "6528281557:AAH_exampleExampleExampleExample-123"

    def test_redact_removes_bot_tokens_in_any_shape(self):
        for raw in (
            "HTTPError url https://api.telegram.org/bot%s/sendMessage 502" % self.TOKEN,
            "fallo con %s al enviar" % self.TOKEN,
            "bot%s" % self.TOKEN.replace(":", "%3A"),
        ):
            out = jinx_chat.redact(raw)
            self.assertNotIn("AAH_example", out, raw)
            self.assertNotIn("6528281557:", out, raw)

    def test_mirror_error_never_contains_the_token(self):
        def opener(req, timeout=0):
            raise OSError("fallo en https://api.telegram.org/bot%s/sendMessage" % self.TOKEN)
        m = jinx_chat.TelegramMirror(self.TOKEN, "42", opener=opener)
        self.assertFalse(m.send("x"))
        self.assertNotIn("AAH_example", m.last_error)
        self.assertNotIn("6528281557:", m.last_error)

    def test_log_file_is_private_and_redacted(self):
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "gateway.log")
            jinx_chat.append_log(path, "x bot%s y" % self.TOKEN)
            self.assertEqual(os.stat(path).st_mode & 0o777, 0o600)
            self.assertNotIn("AAH_example", open(path).read())


class LazyMirrorTest(unittest.TestCase):
    def test_building_the_mirror_does_not_touch_the_vault(self):
        calls = []
        m = jinx_chat.LazyTelegramMirror(lambda: calls.append("t") or "TOK", lambda: calls.append("c") or "42",
                                         opener=lambda req, timeout=0: (_ for _ in ()).throw(OSError("x")))
        self.assertEqual(calls, [])
        self.assertFalse(m.send("hola"))       # el opener falla, pero ya resolvió secretos
        self.assertEqual(calls, ["t", "c"])
        m.send("otra")
        self.assertEqual(calls, ["t", "c"])    # se resuelven una sola vez


class TelegramMirrorTest(unittest.TestCase):
    def test_posts_sendmessage_and_reports_ok(self):
        seen = {}

        class Resp:
            status = 200

            def read(self):
                return b'{"ok":true}'

            def __enter__(self):
                return self

            def __exit__(self, *a):
                return False

        def opener(req, timeout=0):
            seen["url"] = req.full_url
            seen["body"] = req.data
            return Resp()

        m = jinx_chat.TelegramMirror("TOK", "42", opener=opener)
        self.assertTrue(m.send("hola"))
        self.assertTrue(seen["url"].endswith("/botTOK/sendMessage"))
        self.assertIn(b'"chat_id": "42"', seen["body"])

    def test_network_error_is_false_not_an_exception(self):
        def opener(req, timeout=0):
            raise OSError("sin red")
        self.assertFalse(jinx_chat.TelegramMirror("T", "1", opener=opener).send("x"))

    def test_no_token_means_no_mirror(self):
        self.assertFalse(jinx_chat.TelegramMirror("", "", opener=None).send("x"))


if __name__ == "__main__":
    unittest.main()
