import importlib.machinery
import importlib.util
import pathlib
import unittest

p = pathlib.Path(__file__).with_name("jinx-notice-actions")
spec = importlib.util.spec_from_loader("na", importlib.machinery.SourceFileLoader("na", str(p)))
na = importlib.util.module_from_spec(spec)
spec.loader.exec_module(na)

NOW = 1_000_000.0
ACTS = [{"id": "done", "label": "Hecho"}, {"id": "tomorrow", "label": "Mañana"}, {"id": "dismiss", "label": "Descartar"}]


def msg(**kw):
    base = {"id": 5, "text": "Renovar la tarjeta", "kind": "info", "actions": ACTS, "ts": NOW - 30}
    base.update(kw)
    return base


class Show(unittest.TestCase):
    def test_shows_a_fresh_notice_with_buttons_on_the_pc(self):
        self.assertEqual(na.should_show(msg(), "pc", NOW, 0, False), (True, "ok"))

    def test_only_when_the_pc_is_in_use(self):
        self.assertFalse(na.should_show(msg(), "s22", NOW, 0, False)[0])
        self.assertFalse(na.should_show(msg(), "vga", NOW, 0, False)[0])

    def test_plain_acted_stale_and_crowded_are_skipped(self):
        self.assertEqual(na.should_show(msg(actions=[]), "pc", NOW, 0, False)[1], "no buttons")
        self.assertEqual(na.should_show(msg(acted={"action": "done"}), "pc", NOW, 0, False)[1], "already acted on")
        self.assertEqual(na.should_show(msg(ts=NOW - 7 * 3600), "pc", NOW, 0, False)[1], "stale")
        self.assertEqual(na.should_show(msg(), "pc", NOW, na.MAX_OPEN, False)[1], "too many open")

    def test_the_off_switch_wins(self):
        self.assertEqual(na.should_show(msg(), "pc", NOW, 0, True), (False, "off"))


class Command(unittest.TestCase):
    def test_one_action_flag_per_button_and_text_last(self):
        cmd = na.build_command(msg())
        self.assertEqual(cmd[0], "notify-send")
        self.assertEqual([cmd[i + 1] for i, c in enumerate(cmd) if c == "-A"],
                         ["done=Hecho", "tomorrow=Mañana", "dismiss=Descartar"])
        self.assertEqual(cmd[-2:], ["Jinx", "Renovar la tarjeta"])

    def test_alerts_are_critical(self):
        cmd = na.build_command(msg(kind="alert"))
        self.assertEqual(cmd[cmd.index("-u") + 1], "critical")

    def test_long_text_is_cut(self):
        self.assertEqual(len(na.build_command(msg(text="x" * 900))[-1]), 300)


class Choice(unittest.TestCase):
    def test_the_pressed_button_is_returned(self):
        self.assertEqual(na.chosen_action("done\n", msg()), "done")
        self.assertEqual(na.chosen_action("tomorrow", msg()), "tomorrow")

    def test_dismissed_or_foreign_output_is_nothing(self):
        self.assertIsNone(na.chosen_action("", msg()))
        self.assertIsNone(na.chosen_action("\n", msg()))
        self.assertIsNone(na.chosen_action("rm_rf", msg()))      # never an id the notice did not offer


if __name__ == "__main__":
    unittest.main()
