import json
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(__file__))
import jinx_hermes_chat as hc


class FakeApi:
    """Simula lo que el script pide a la API de Hermes (sin red)."""

    def __init__(self, sessions=None, messages=None, chat_reply="hola desde Hermes", fail_chat=False):
        self.sessions = sessions if sessions is not None else [
            {"id": "old_tg", "source": "telegram", "last_active": "100", "message_count": 3},
            {"id": "new_tg", "source": "telegram", "last_active": "900", "message_count": 120},
            {"id": "cron_1", "source": "cron", "last_active": "1000", "message_count": 2},
            {"id": "arch_tg", "source": "telegram", "last_active": "950", "archived": True, "message_count": 5},
        ]
        self.messages = messages or []
        self.chat_reply = chat_reply
        self.fail_chat = fail_chat
        self.calls = []

    def get(self, path):
        self.calls.append(("GET", path))
        if path.startswith("/api/sessions?"):
            return {"sessions": self.sessions}
        if "/messages" in path:
            return {"data": self.messages, "pagination": {"offset": 0}}
        raise AssertionError(path)

    def post(self, path, body):
        self.calls.append(("POST", path, body))
        if self.fail_chat:
            raise hc.ApiError("HTTP 500")
        return {"session_id": path.split("/")[3], "message": {"role": "assistant", "content": self.chat_reply}}


class SessionPickTest(unittest.TestCase):
    def test_picks_the_most_recent_live_telegram_session(self):
        self.assertEqual(hc.pick_telegram_session(FakeApi().sessions), ("new_tg", 120))

    def test_ignores_archived_hidden_and_other_sources(self):
        only_bad = [
            {"id": "a", "source": "cron", "last_active": "5"},
            {"id": "b", "source": "telegram", "last_active": "9", "archived": True},
            {"id": "c", "source": "telegram", "last_active": "8", "hidden": True},
        ]
        self.assertIsNone(hc.pick_telegram_session(only_bad))


class SendTest(unittest.TestCase):
    def test_send_posts_the_message_into_the_telegram_session(self):
        api = FakeApi()
        out = hc.handle({"op": "send", "text": "hola jinx", "cid": "c1"}, api)
        self.assertEqual(out, {"ev": "done", "text": "hola desde Hermes", "session_id": "new_tg"})
        post = [c for c in api.calls if c[0] == "POST"][0]
        self.assertEqual(post[1], "/api/sessions/new_tg/chat")
        self.assertEqual(post[2], {"message": "hola jinx"})

    def test_empty_text_is_an_error_and_nothing_is_posted(self):
        api = FakeApi()
        out = hc.handle({"op": "send", "text": "   "}, api)
        self.assertEqual(out["ev"], "error")
        self.assertFalse([c for c in api.calls if c[0] == "POST"])

    def test_no_telegram_session_is_a_clear_error(self):
        out = hc.handle({"op": "send", "text": "x"}, FakeApi(sessions=[]))
        self.assertEqual(out["ev"], "error")
        self.assertIn("sesión", out["msg"])

    def test_api_failure_becomes_an_error_event_not_a_crash(self):
        out = hc.handle({"op": "send", "text": "x"}, FakeApi(fail_chat=True))
        self.assertEqual(out["ev"], "error")
        self.assertIn("500", out["msg"])

    def test_blank_assistant_reply_is_an_error(self):
        out = hc.handle({"op": "send", "text": "x"}, FakeApi(chat_reply="  "))
        self.assertEqual(out["ev"], "error")


class HistoryTest(unittest.TestCase):
    def test_history_reads_the_tail_of_the_session_and_keeps_only_chat_turns(self):
        msgs = [
            {"id": "1", "role": "user", "content": "hola", "timestamp": "10.5"},
            {"id": "2", "role": "assistant", "content": "qué onda", "timestamp": "11.0"},
            {"id": "3", "role": "tool", "content": "{...}", "timestamp": "11.5"},
            {"id": "4", "role": "assistant", "content": "", "tool_calls": [{"x": 1}], "timestamp": "12.0"},
            {"id": "5", "role": "user", "content": [{"type": "text", "text": "multi"}], "timestamp": "13.0"},
        ]
        api = FakeApi(messages=msgs)
        out = hc.handle({"op": "history", "n": 40}, api)
        self.assertEqual(out["ev"], "history")
        self.assertEqual([(m["role"], m["text"]) for m in out["items"]],
                         [("user", "hola"), ("assistant", "qué onda"), ("user", "multi")])
        get = [c for c in api.calls if c[0] == "GET" and "/messages" in c[1]][0]
        self.assertIn("offset=80", get[1])           # 120 messages, last 40

    def test_automatic_system_triggers_are_not_shown_as_things_alan_said(self):
        msgs = [
            {"id": "1", "role": "user", "content": "[[DISPARADOR AUTOMÁTICO DEL SISTEMA — Alan NO escribió nada]]", "timestamp": "1"},
            {"id": "2", "role": "assistant", "content": "respuesta al disparador", "timestamp": "2"},
            {"id": "3", "role": "user", "content": "hola de verdad", "timestamp": "3"},
        ]
        out = hc.handle({"op": "history", "n": 10}, FakeApi(messages=msgs))
        # the trigger is hidden; what Jinx said in answer (her proactive message to Alan) is shown
        self.assertEqual([m["text"] for m in out["items"]], ["respuesta al disparador", "hola de verdad"])

    def test_unknown_op_and_garbage_are_errors(self):
        self.assertEqual(hc.handle({"op": "nope"}, FakeApi())["ev"], "error")
        self.assertEqual(hc.handle({}, FakeApi())["ev"], "error")


class KeyHandlingTest(unittest.TestCase):
    def test_redact_hides_the_key_in_any_message(self):
        self.assertNotIn("SECRETKEY123", hc.redact("fallo con Bearer SECRETKEY123 en la llamada", "SECRETKEY123"))


if __name__ == "__main__":
    unittest.main()
