import importlib.util
import os
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))


def load():
    spec = importlib.util.spec_from_file_location("catos_under_test", os.path.join(HERE, "catos_server.py"))
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


class HostAndOriginTest(unittest.TestCase):
    def test_host_rule(self):
        m = load()
        ok = m.host_allowed
        self.assertTrue(ok("100.92.254.125:8899", 8899))
        self.assertTrue(ok("server:8899", 8899))
        self.assertTrue(ok("localhost:8899", 8899))
        self.assertTrue(ok("debian.tail1234.ts.net:8899", 8899))
        self.assertFalse(ok("evil.example:8899", 8899))
        self.assertFalse(ok("100.92.254.125.evil.example:8899", 8899))
        self.assertFalse(ok("evil.ts.net.evil.example:8899", 8899))
        self.assertFalse(ok("100.92.254.125:9999", 8899))
        self.assertFalse(ok("", 8899))
        self.assertFalse(ok(None, 8899))

    def test_a_cross_site_post_is_refused_and_the_page_itself_or_curl_is_not(self):
        m = load()
        self.assertFalse(m.origin_allowed("https://evil.example", "100.92.254.125:8899"))
        self.assertFalse(m.origin_allowed("null", "100.92.254.125:8899"))
        self.assertTrue(m.origin_allowed("http://100.92.254.125:8899", "100.92.254.125:8899"))
        self.assertTrue(m.origin_allowed(None, "100.92.254.125:8899"))


if __name__ == "__main__":
    unittest.main()
