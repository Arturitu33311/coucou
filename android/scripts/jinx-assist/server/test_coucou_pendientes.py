import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(__file__))
import coucou_pendientes as cp


class FakeStore:
    """Lo mínimo de pendientes_store que usa el gateway."""

    def __init__(self):
        self.items = [
            {"id": "a1", "titulo": "Pagar luz", "due": "2026-10-20", "categoria": "casa", "source": "manual",
             "status": "pendiente", "updated_at": "2026-10-13T10:00:00", "nota": "secreta", "prioridad": 1},
            {"id": "b2", "titulo": "Vieja", "due": None, "categoria": "otro", "source": "teams",
             "status": "hecho", "updated_at": "2026-01-01T10:00:00"},
            {"id": "c3", "titulo": "Reciente cerrada", "due": None, "categoria": "otro", "source": "coucou",
             "status": "descartado", "updated_at": "2026-10-12T10:00:00"},
        ]
        self.calls = []

    def load(self):
        return self.items

    def add(self, titulo, **kw):
        self.calls.append(("add", titulo, kw))
        return {"id": "new1", "titulo": titulo}

    def resolve(self, ref, accion="hecho"):
        self.calls.append(("resolve", ref, accion))
        if ref == "zzz":
            return {"error": "no such pendiente"}
        return {"id": ref, "status": accion}


class ListTest(unittest.TestCase):
    def test_list_returns_only_the_fields_the_phone_needs(self):
        out = cp.handle({"op": "list"}, FakeStore(), now_iso="2026-10-13T12:00:00")
        self.assertTrue(out["ok"])
        first = [i for i in out["items"] if i["id"] == "a1"][0]
        self.assertEqual(set(first), {"id", "titulo", "due", "categoria", "source", "status", "updated_at"})
        self.assertNotIn("nota", first)

    def test_list_keeps_open_ones_and_recently_closed_but_drops_old_closed(self):
        out = cp.handle({"op": "list"}, FakeStore(), now_iso="2026-10-13T12:00:00")
        ids = {i["id"] for i in out["items"]}
        self.assertEqual(ids, {"a1", "c3"})


class AddTest(unittest.TestCase):
    def test_add_goes_through_her_store_as_a_coucou_pendiente(self):
        st = FakeStore()
        out = cp.handle({"op": "add", "titulo": "Llamar a mamá", "due": "2026-10-14", "nota": "Coucou · 09:00",
                         "source_id": "task-1"}, st)
        self.assertEqual(out, {"ok": True, "id": "new1"})
        _, titulo, kw = st.calls[0]
        self.assertEqual(titulo, "Llamar a mamá")
        self.assertEqual(kw["source"], "coucou")
        self.assertEqual(kw["source_id"], "task-1")
        self.assertEqual(kw["due"], "2026-10-14")
        self.assertEqual(kw["categoria"], "personal")

    def test_add_rejects_bad_input_without_touching_the_store(self):
        st = FakeStore()
        bad = [
            {"op": "add"},
            {"op": "add", "titulo": "   ", "source_id": "x"},
            {"op": "add", "titulo": "x" * 500, "source_id": "x"},
            {"op": "add", "titulo": "ok", "source_id": "../etc"},
            {"op": "add", "titulo": "ok", "source_id": "x", "due": "mañana"},
            {"op": "add", "titulo": "ok", "source_id": "x", "nota": "n" * 1000},
        ]
        for req in bad:
            out = cp.handle(req, st)
            self.assertFalse(out["ok"], req)
        self.assertEqual(st.calls, [])


class ResolveTest(unittest.TestCase):
    def test_resolve_accepts_only_known_actions(self):
        st = FakeStore()
        for a in ("hecho", "descartado", "reabrir"):
            self.assertTrue(cp.handle({"op": "resolve", "id": "a1", "accion": a}, st)["ok"])
        self.assertFalse(cp.handle({"op": "resolve", "id": "a1", "accion": "borrar"}, st)["ok"])
        self.assertFalse(cp.handle({"op": "resolve", "id": "a1; rm", "accion": "hecho"}, st)["ok"])

    def test_store_error_is_reported_not_raised(self):
        out = cp.handle({"op": "resolve", "id": "zzz", "accion": "hecho"}, FakeStore())
        self.assertFalse(out["ok"])
        self.assertIn("no such", out["error"])


class GarbageTest(unittest.TestCase):
    def test_unknown_op_and_non_dict_are_errors(self):
        self.assertFalse(cp.handle({"op": "format"}, FakeStore())["ok"])
        self.assertFalse(cp.handle([], FakeStore())["ok"])

    def test_store_exceptions_become_error_replies(self):
        class Boom(FakeStore):
            def load(self):
                raise OSError("disco")

        out = cp.handle({"op": "list"}, Boom())
        self.assertFalse(out["ok"])


if __name__ == "__main__":
    unittest.main()
