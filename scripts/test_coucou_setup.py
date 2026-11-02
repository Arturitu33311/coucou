import importlib.machinery
import importlib.util
import pathlib
import tempfile
import unittest

p = pathlib.Path(__file__).with_name("coucou-setup")
spec = importlib.util.spec_from_loader("cs", importlib.machinery.SourceFileLoader("cs", str(p)))
cs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cs)


class Phones(unittest.TestCase):
    CONF = "# c\ns21  phone  100.78.102.31 8022 termux\ns22 s22 100.100.137.97 8022 termux  # x\nbad line\n"

    def test_parse(self):
        rows = cs.parse_phones(self.CONF)
        self.assertEqual([r["name"] for r in rows], ["s21", "s22"])
        self.assertEqual(rows[1]["port"], 8022)

    def test_stale_suggests_the_online_unassigned_android(self):
        phones = cs.parse_phones(self.CONF)
        peers = [
            {"name": "celular", "ip": "100.78.102.31", "os": "android", "online": False},
            {"name": "s22-old", "ip": "100.100.137.97", "os": "android", "online": False},
            {"name": "s22-new", "ip": "100.118.153.16", "os": "android", "online": True},
        ]
        msgs = cs.stale_phone_suggestions(phones, peers)
        self.assertEqual(len(msgs), 2)
        self.assertIn("sin reemplazo evidente", msgs[0])  # s21: no peer named like it
        self.assertIn("s22-new (100.118.153.16)", msgs[1])

    def test_online_phone_is_not_stale(self):
        phones = cs.parse_phones(self.CONF)
        peers = [{"name": "a", "ip": "100.78.102.31", "os": "android", "online": True},
                 {"name": "b", "ip": "100.100.137.97", "os": "android", "online": True}]
        self.assertEqual(cs.stale_phone_suggestions(phones, peers), [])


class Drift(unittest.TestCase):
    def test_missing_differs_same(self):
        with tempfile.TemporaryDirectory() as r, tempfile.TemporaryDirectory() as b:
            r, b = pathlib.Path(r), pathlib.Path(b)
            for f in ("a", "b", "c"):
                (r / f).write_text("x")
            (b / "a").write_text("x")
            (b / "b").write_text("y")
            self.assertEqual(cs.drift(["a", "b", "c"], r, b), [("b", "differs"), ("c", "missing")])


class Store(unittest.TestCase):
    def test_only_the_store_counts(self):
        ok = "package:com.agupta07505.smartisland  installer=com.android.vending"
        self.assertTrue(cs.store_installed(ok))
        self.assertFalse(cs.store_installed("package:com.agupta07505.smartisland  installer=null"))
        self.assertFalse(cs.store_installed("package:com.agupta07505.smartisland  installer=com.google.android.packageinstaller"))
        self.assertFalse(cs.store_installed(""))


class Relay(unittest.TestCase):
    def test_states(self):
        self.assertTrue(cs.relay_active("coucou_audiolink", True))
        self.assertFalse(cs.relay_active("coucou_audiolink", False))  # hold: no capture
        self.assertFalse(cs.relay_active("alsa_output.pci", True))


if __name__ == "__main__":
    unittest.main()
