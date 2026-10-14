import importlib.machinery
import importlib.util
import os
import unittest

path = os.path.join(os.path.dirname(__file__), "coucou-dispatch")
loader = importlib.machinery.SourceFileLoader("coucou_dispatch", path)
spec = importlib.util.spec_from_loader("coucou_dispatch", loader)
mod = importlib.util.module_from_spec(spec)
loader.exec_module(mod)

HOME = "/home/u"
APP = HOME + "/.local/share/coucou-app/coucou-hook"
OLD = HOME + "/.local/share/coucou/bin/coucou-hook"


class RouteTest(unittest.TestCase):
    def test_jinx_chat_routes_to_the_gateway(self):
        argv, env = mod.route("jinx-chat", HOME, 1000, exists=lambda p: True)
        self.assertEqual(argv, [HOME + "/bin/jinx-chat"])

    def test_coucou_stream_keeps_the_old_command_and_env(self):
        argv, env = mod.route("coucou", HOME, 1000, exists=lambda p: p == APP)
        self.assertEqual(argv, [APP, "--remote"])
        self.assertEqual(env, {"XDG_RUNTIME_DIR": "/run/user/1000"})

    def test_falls_back_to_the_other_hook_path(self):
        argv, env = mod.route("coucou", HOME, 1000, exists=lambda p: p == OLD)
        self.assertEqual(argv, [OLD, "--remote"])

    def test_anything_else_is_the_old_behaviour_never_a_shell(self):
        for cmd in ["", "bash", "ls; rm -rf /", "jinx-chat --x", "coucou-pendientes"]:
            argv, _ = mod.route(cmd, HOME, 1000, exists=lambda p: p == APP)
            self.assertEqual(argv, [APP, "--remote"], cmd)

    def test_no_hook_installed_is_none(self):
        self.assertIsNone(mod.route("coucou", HOME, 1000, exists=lambda p: False))


if __name__ == "__main__":
    unittest.main()
