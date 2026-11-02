import importlib.machinery
import importlib.util
import pathlib
import unittest

p = pathlib.Path(__file__).with_name("jinx-voice-alert")
spec = importlib.util.spec_from_loader("va", importlib.machinery.SourceFileLoader("va", str(p)))
va = importlib.util.module_from_spec(spec)
spec.loader.exec_module(va)

NOW = 1_000_000.0


def ctx(**kw):
    base = {"conf": dict(va.DEFAULTS), "now": NOW, "hour": 14, "active_device": "pc", "relay": True,
            "spoken_today": 0, "recent": {}}
    base.update(kw)
    return base


def msg(**kw):
    base = {"id": 1, "text": "Se cayó el servicio", "kind": "warn", "source": "monitor", "ts": NOW - 5}
    base.update(kw)
    return base


class Quiet(unittest.TestCase):
    def test_wraps_midnight(self):
        for h in (23, 0, 3, 6):
            self.assertTrue(va.in_quiet_hours(h, 23, 7), h)
        for h in (7, 12, 22):
            self.assertFalse(va.in_quiet_hours(h, 23, 7), h)


class Decide(unittest.TestCase):
    def test_speaks_a_warning_on_the_relay(self):
        self.assertEqual(va.decide(msg(), ctx()), (True, "ok"))

    def test_needs_the_relay_and_the_pc(self):
        self.assertFalse(va.decide(msg(), ctx(relay=False))[0])
        self.assertFalse(va.decide(msg(), ctx(active_device="s22"))[0])

    def test_plain_info_is_silent_but_reminders_speak(self):
        self.assertFalse(va.decide(msg(kind="info", text="Terminó de bajar una serie", source="torrent"), ctx())[0])
        self.assertTrue(va.decide(msg(kind="info", text="Tienes lab en 15 minutos", source="x"), ctx())[0])
        self.assertTrue(va.decide(msg(kind="info", text="hola", source="recordatorio_clases"), ctx())[0])

    def test_quiet_hours_stale_daily_max_and_repeat(self):
        self.assertEqual(va.decide(msg(), ctx(hour=2))[1], "quiet hours")
        self.assertEqual(va.decide(msg(ts=NOW - 700), ctx())[1], "stale")
        self.assertEqual(va.decide(msg(), ctx(spoken_today=8))[1], "daily maximum reached")
        self.assertEqual(va.decide(msg(), ctx(recent={"Se cayó el servicio": NOW - 60}))[1], "repeat")
        self.assertTrue(va.decide(msg(), ctx(recent={"Se cayó el servicio": NOW - 700}))[0])

    def test_real_reminders_from_the_inbox_are_recognized(self):
        for text in ("⏰ HOY cierra «practica 7 tiro parabólico» de FISICA I", "🔴 FISICA I: «practica 7» se pasó",
                     "Oye, la del MCU es mañana a las 6 am"):
            self.assertTrue(va.eligible_kind(msg(kind="info", text=text, source="")), text)
        self.assertFalse(va.eligible_kind(msg(kind="info", text="Todavía traes ganas de que te cuente una historia", source="")))

    def test_cooldown_between_spoken_notices_but_not_for_alerts(self):
        recent = ctx(last_spoken=NOW - 300)
        self.assertEqual(va.decide(msg(kind="warn"), recent)[1], "cooldown after the last spoken notice")
        self.assertTrue(va.decide(msg(kind="alert"), recent)[0])
        self.assertTrue(va.decide(msg(kind="warn"), ctx(last_spoken=NOW - 1300))[0])

    def test_switched_off(self):
        c = ctx()
        c["conf"]["enabled"] = False
        self.assertEqual(va.decide(msg(), c), (False, "off"))

    def test_config_can_change_the_limits(self):
        c = ctx(spoken_today=8)
        c["conf"]["daily_max"] = 12
        self.assertTrue(va.decide(msg(), c)[0])


if __name__ == "__main__":
    unittest.main()
