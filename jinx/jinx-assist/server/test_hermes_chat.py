import json
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(__file__))
import jinx_hermes_chat as hc

hc.BRIDGE_LOG = os.path.join(tempfile.mkdtemp(), "bridge.log")


class FakeApi:
    """Simula lo que el script pide a la API de Hermes (sin red)."""

    def __init__(self, sessions=None, messages=None, chat_reply="hola desde Hermes", fail_chat=False,
                 stream_events=None, approve_status=None):
        self.sessions = sessions if sessions is not None else [
            {"id": "old_tg", "source": "telegram", "last_active": "100", "message_count": 3},
            {"id": "new_tg", "source": "telegram", "last_active": "900", "message_count": 120},
            {"id": "cron_1", "source": "cron", "last_active": "1000", "message_count": 2},
            {"id": "arch_tg", "source": "telegram", "last_active": "950", "archived": True, "message_count": 5},
        ]
        self.messages = messages or []
        self.chat_reply = chat_reply
        self.fail_chat = fail_chat
        # None: este Hermes no tiene el endpoint de stream (404), y send usa el /chat de siempre.
        self.stream_events = stream_events
        self.approve_status = approve_status
        self.calls = []

    def stream(self, path, body):
        self.calls.append(("STREAM", path, body))
        if self.stream_events is None:
            raise hc.ApiError("HTTP 404")
        for ev in self.stream_events:
            if callable(ev):
                ev()
                continue
            yield ev

    def get(self, path):
        self.calls.append(("GET", path))
        if path.startswith("/api/sessions?"):
            return {"sessions": self.sessions}
        if "/messages" in path:
            return {"data": self.messages, "pagination": {"offset": 0}}
        raise AssertionError(path)

    def post(self, path, body):
        self.calls.append(("POST", path, body))
        if path.startswith("/v1/runs/"):
            if self.approve_status:
                raise hc.ApiError("HTTP %s" % self.approve_status)
            return {"object": "hermes.run.approval_response"}
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


def tmp_store(clock=None):
    d = tempfile.mkdtemp()
    return hc.ApprovalStore(os.path.join(d, "approvals.json"), clock=clock or (lambda: 1000.0))


APPROVAL = ("approval.request", {"event": "approval.request", "run_id": "run_abc", "request_id": "r1",
                                 "command": "rm -rf /tmp/x", "description": "delete files",
                                 "choices": ["once", "session", "always", "deny"]})
DONE = [("assistant.completed", {"run_id": "run_abc", "content": "listo, borrado", "session_id": "new_tg"}),
        ("run.completed", {"run_id": "run_abc"}), ("done", {"run_id": "run_abc"})]


class SseTest(unittest.TestCase):
    def test_frames_become_named_events_and_keepalives_are_skipped(self):
        raw = [b": keepalive\n", b"\n", b"event: approval.request\n", b'data: {"run_id":"run_1"}\n', b"\n",
               b'data: {"event":"x","a":1}\n', b"\n", b"data: not json\n", b"\n"]
        self.assertEqual(list(hc.sse_events(raw)), [("keepalive", {}), ("approval.request", {"run_id": "run_1"}),
                                                     ("x", {"event": "x", "a": 1})])


class StreamSendTest(unittest.TestCase):
    def setUp(self):
        self.announced = []
        self._orig = hc.announce_approval
        hc.announce_approval = lambda item: self.announced.append(item["description"])

    def tearDown(self):
        hc.announce_approval = self._orig

    def test_a_question_is_announced_on_the_device_in_use(self):
        hc.handle({"op": "send", "text": "borra x"}, FakeApi(stream_events=[APPROVAL, *DONE]), tmp_store())
        self.assertEqual(self.announced, ["delete files"])

    def test_the_announcement_never_fails(self):
        def boom(req, timeout):
            raise OSError("no presence")
        self._orig({"description": "x"}, opener=boom)

    def test_send_goes_by_the_stream_and_returns_the_reply(self):
        api = FakeApi(stream_events=DONE)
        out = hc.handle({"op": "send", "text": "hola"}, api, tmp_store())
        self.assertEqual(out, {"ev": "done", "text": "listo, borrado", "session_id": "new_tg"})
        self.assertEqual(api.calls[-1], ("STREAM", "/api/sessions/new_tg/chat/stream", {"message": "hola"}))
        self.assertFalse([c for c in api.calls if c[0] == "POST"])

    def test_a_question_waits_in_the_store_while_the_turn_runs_and_goes_when_it_ends(self):
        store = tmp_store()
        seen = []
        api = FakeApi(stream_events=[APPROVAL, lambda: seen.append(store.list()), *DONE])
        hc.handle({"op": "send", "text": "borra x"}, api, store)
        self.assertEqual(len(seen[0]), 1)
        self.assertEqual(seen[0][0]["command"], "rm -rf /tmp/x")
        self.assertEqual(seen[0][0]["choices"], ["once", "session", "always", "deny"])
        self.assertEqual(store.list(), [])  # the turn is over: nothing left to answer

    def test_a_failed_turn_is_an_error(self):
        api = FakeApi(stream_events=[("run.failed", {"run_id": "run_abc", "error": "boom"}), ("done", {})])
        out = hc.handle({"op": "send", "text": "x"}, api, tmp_store())
        self.assertEqual(out, {"ev": "error", "msg": "boom"})


    def test_a_turn_with_no_question_is_cut_at_the_limit_and_one_with_a_question_waits(self):
        t = [0.0]
        clock = lambda: t[0]
        tick = lambda: t.__setitem__(0, t[0] + hc.TURN_LIMIT + 1)
        api = FakeApi(stream_events=[("keepalive", {}), tick, ("keepalive", {}), *DONE])
        with self.assertRaises(hc.ApiError):
            hc._send_stream(api, "s", "x", tmp_store(), clock=clock)
        t[0] = 0.0
        api = FakeApi(stream_events=[APPROVAL, tick, ("keepalive", {}), *DONE])
        self.assertEqual(hc._send_stream(api, "s", "x", tmp_store(), clock=clock)[0], "listo, borrado")


    def test_a_stream_that_never_opened_goes_by_the_old_chat(self):
        api = FakeApi(stream_events=[])
        api.stream_error = hc.ApiError("HTTP 401")
        orig = api.stream

        def failing(path, body):
            api.calls.append(("STREAM", path, body))
            raise api.stream_error
            yield  # noqa
        api.stream = failing
        out = hc.handle({"op": "send", "text": "x"}, api, tmp_store())
        self.assertEqual(out["text"], "hola desde Hermes")
        self.assertIn("/api/sessions/new_tg/chat", [c[1] for c in api.calls if c[0] == "POST"])

    def test_a_stream_that_broke_after_starting_is_never_resent(self):
        def broken(path, body):
            yield ("run.started", {"run_id": "run_abc"})
            raise hc.ApiError("se cortó el stream")
        api = FakeApi()
        api.stream = broken
        out = hc.handle({"op": "send", "text": "x"}, api, tmp_store())
        self.assertEqual(out["ev"], "error")
        self.assertFalse([c for c in api.calls if c[0] == "POST"])

    def test_without_completed_text_the_deltas_are_the_answer(self):
        api = FakeApi(stream_events=[("assistant.delta", {"delta": "ho"}), ("assistant.delta", {"delta": "la"}),
                                     ("done", {})])
        self.assertEqual(hc.handle({"op": "send", "text": "x"}, api, tmp_store())["text"], "hola")

    def test_a_turn_taken_elsewhere_is_read_back_from_the_session(self):
        msgs = [{"id": "9", "role": "assistant", "content": "la real", "timestamp": str(10 ** 10)}]
        api = FakeApi(stream_events=[("run.started", {}), ("run.queued", {"status": "queued"}), ("done", {})],
                      messages=msgs)
        self.assertEqual(hc.handle({"op": "send", "text": "x"}, api, tmp_store())["text"], "la real")


class ApprovalOpsTest(unittest.TestCase):
    def setUp(self):
        self.store = tmp_store()
        self.store.add(hc.approval_item(APPROVAL[1], clock=lambda: 1000.0))

    def test_approvals_lists_what_waits(self):
        out = hc.handle({"op": "approvals"}, FakeApi(), self.store)
        self.assertEqual(out["ev"], "approvals")
        self.assertEqual([(d["run_id"], d["request_id"]) for d in out["items"]], [("run_abc", "r1")])

    def test_approve_answers_hermes_and_clears_it(self):
        api = FakeApi()
        out = hc.handle({"op": "approve", "run_id": "run_abc", "request_id": "r1", "choice": "once"}, api, self.store)
        self.assertEqual(out, {"ev": "approved", "choice": "once"})
        self.assertIn(("POST", "/v1/runs/run_abc/approval", {"choice": "once", "request_id": "r1"}), api.calls)
        self.assertEqual(self.store.list(), [])

    def test_only_what_is_pending_can_be_answered(self):
        api = FakeApi()
        out = hc.handle({"op": "approve", "run_id": "run_other", "request_id": "r1", "choice": "once"}, api, self.store)
        self.assertEqual(out["ev"], "error")
        self.assertFalse([c for c in api.calls if c[0] == "POST"])

    def test_a_choice_hermes_did_not_offer_is_refused(self):
        self.store.add(hc.approval_item(dict(APPROVAL[1], request_id="r2", choices=["once", "deny"]),
                                        clock=lambda: 1000.0))
        api = FakeApi()
        out = hc.handle({"op": "approve", "run_id": "run_abc", "request_id": "r2", "choice": "always"}, api, self.store)
        self.assertEqual(out["ev"], "error")
        self.assertFalse([c for c in api.calls if c[0] == "POST"])

    def test_bad_ids_never_reach_a_url(self):
        api = FakeApi()
        out = hc.handle({"op": "approve", "run_id": "../x", "request_id": "r1", "choice": "once"}, api, self.store)
        self.assertEqual(out["ev"], "error")
        self.assertFalse([c for c in api.calls if c[0] == "POST"])

    def test_already_answered_elsewhere_is_said_and_cleared(self):
        api = FakeApi(approve_status=409)
        out = hc.handle({"op": "approve", "run_id": "run_abc", "request_id": "r1", "choice": "deny"}, api, self.store)
        self.assertEqual(out["ev"], "error")
        self.assertEqual(self.store.list(), [])

    def test_old_questions_expire(self):
        store = hc.ApprovalStore(self.store.path, clock=lambda: 1000.0 + hc.APPROVAL_LIFE + 1)
        self.assertEqual(store.list(), [])


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
