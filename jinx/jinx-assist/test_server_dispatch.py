import importlib.machinery
import importlib.util
import os
import unittest

path = os.path.join(os.path.dirname(__file__), "server-dispatch")
loader = importlib.machinery.SourceFileLoader("server_dispatch", path)
spec = importlib.util.spec_from_loader("server_dispatch", loader)
mod = importlib.util.module_from_spec(spec)
loader.exec_module(mod)

HOME = "/home/alan"


class ServerRouteTest(unittest.TestCase):
    def test_jinx_chat_goes_to_the_gateway(self):
        self.assertEqual(mod.route("jinx-chat", HOME, exists=lambda p: True), [HOME + "/bin/jinx-chat"])

    def test_without_the_gateway_nothing_runs(self):
        self.assertIsNone(mod.route("jinx-chat", HOME, exists=lambda p: False))

    def test_everything_else_is_the_pendientes_script_as_before(self):
        for cmd in ("coucou-pendientes", "", None, "rm -rf /", "jinx-chat; rm -rf /"):
            self.assertEqual(mod.route(cmd, HOME, exists=lambda p: True), list(mod.PENDIENTES), cmd)


if __name__ == "__main__":
    unittest.main()
