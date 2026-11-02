import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).parent))
import speech_tags as st  # noqa: E402


class Strip(unittest.TestCase):
    def test_basic(self):
        self.assertEqual(st.strip_tags("[warmly] Tienes lab en 15 minutos."), "Tienes lab en 15 minutos.")

    def test_stacked_and_midline_with_tidy_spaces(self):
        self.assertEqual(st.strip_tags("[tired] [softly] Hoy no fue tu día [short pause] , ¿no?"), "Hoy no fue tu día, ¿no?")
        self.assertEqual(st.strip_tags("[sighs, amused] Bueno… [pause] vence a las 17:30"), "Bueno… vence a las 17:30")

    def test_fullwidth_brackets_are_tags_too(self):
        self.assertEqual(st.strip_tags("【warmly】 hola"), "hola")

    def test_never_touches_other_brackets(self):
        for keep in ("Fuente [BUSCAR] pendiente", "Pon aquí [ALAN: tu experiencia]", "mira [esto](http://x.y)",
                     "marca [[DISPARADOR]] interna", "lista [1] y [2]", "[Importante] en mayúscula"):
            self.assertEqual(st.strip_tags(keep), keep)

    def test_keeps_line_breaks(self):
        self.assertEqual(st.strip_tags("[warmly] uno\n[softly] dos"), "uno\ndos")

    def test_empty_and_plain(self):
        self.assertEqual(st.strip_tags(""), "")
        self.assertEqual(st.strip_tags("sin tags"), "sin tags")


class Split(unittest.TestCase):
    def test_speech_only_when_tagged(self):
        self.assertEqual(st.split("[warmly] hola"), ("hola", "[warmly] hola"))
        self.assertEqual(st.split("hola"), ("hola", None))

    def test_ensure_tag_defaults_by_kind(self):
        self.assertEqual(st.ensure_tag("hola", "alert"), "[urgently] hola")
        self.assertEqual(st.ensure_tag("hola", "warn"), "[concerned] hola")
        self.assertEqual(st.ensure_tag("hola"), "[warmly] hola")
        self.assertEqual(st.ensure_tag("[sighs] hola", "alert"), "[sighs] hola")
        self.assertEqual(st.ensure_tag("【softly】 hola"), "[softly] hola")


if __name__ == "__main__":
    unittest.main()
