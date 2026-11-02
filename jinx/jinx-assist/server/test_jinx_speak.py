import pathlib
import sys
import unittest
import urllib.error

sys.path.insert(0, str(pathlib.Path(__file__).parent))
import jinx_speak as js  # noqa: E402


def http(code):
    return urllib.error.HTTPError("u", code, "x", {}, None)


class Prepare(unittest.TestCase):
    def test_untagged_gets_default_by_kind(self):
        self.assertEqual(js.prepare("Lab en 15 minutos", "warn"), "[concerned] Lab en 15 minutos")

    def test_tagged_is_kept(self):
        self.assertEqual(js.prepare("[softly] Hola Alan", "alert"), "[softly] Hola Alan")

    def test_noise_removed(self):
        self.assertEqual(js.prepare("⚠️ Mira https://x.y/z **ya**", "info"), "[warmly] Mira ya")

    def test_age_prefix_removed(self):
        self.assertEqual(js.prepare("🕓 3 h · El briefing está listo", "info"), "[warmly] El briefing está listo")

    def test_cap_cuts_at_a_sentence_and_ignores_tags(self):
        long = "[warmly] " + " ".join(["uno dos tres cuatro cinco."] * 10)
        out = js.prepare(long, "info")
        self.assertLessEqual(js.spoken_words(out), js.MAX_WORDS)
        self.assertTrue(out.endswith("cinco."))
        self.assertTrue(out.startswith("[warmly]"))


class Chain(unittest.TestCase):
    def test_v4_with_settings_first(self):
        calls = []

        def post(key, text, model, settings):
            calls.append((model, settings is not None, text))
            return b"x" * 2000
        mp3, model, st = js.synthesize("[warmly] hola", "k", post)
        self.assertEqual((model, st), (js.MODEL_V4, True))
        self.assertEqual(len(calls), 1)

    def test_settings_refused_retries_without(self):
        def post(key, text, model, settings):
            if settings:
                raise http(422)
            return b"x" * 2000
        _, model, st = js.synthesize("[warmly] hola", "k", post)
        self.assertEqual((model, st), (js.MODEL_V4, False))

    def test_v4_unavailable_falls_to_flash_without_tags(self):
        seen = []

        def post(key, text, model, settings):
            seen.append((model, text))
            if model == js.MODEL_V4:
                raise http(404)
            return b"x" * 2000
        _, model, _ = js.synthesize("[warmly] hola Alan", "k", post)
        self.assertEqual(model, js.MODEL_FLASH)
        self.assertEqual(seen[-1], (js.MODEL_FLASH, "hola Alan"))

    def test_bad_key_or_quota_stops_at_once(self):
        calls = []

        def post(key, text, model, settings):
            calls.append(model)
            raise http(401)
        with self.assertRaises(urllib.error.HTTPError):
            js.synthesize("[warmly] hola", "k", post)
        self.assertEqual(len(calls), 1)


if __name__ == "__main__":
    unittest.main()
