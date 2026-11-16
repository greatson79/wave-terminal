"""0.3.6 C4: javis_phoenix_harness._resolve_cysd never prefers the original cys.app; Wave's own waved comes first.
Pure: os.path.exists / realpath / shutil_which are stubbed (no host access)."""
import importlib.util
import os
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
HARNESS = os.path.normpath(os.path.join(HERE, "..", "javis_phoenix_harness.py"))
WAVE = "/Applications/Wave Terminal.app/Contents/MacOS/waved"
CYS = "/Applications/" + "cys.app/Contents/MacOS/cysd"


def load():
    spec = importlib.util.spec_from_file_location("harness_resolve_t", HARNESS)
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    return m


class ResolveCysd(unittest.TestCase):
    def run_resolve(self, existing, which=None):
        m = load()
        os.environ.pop("PHOENIX_HARNESS_CYSD", None)
        real_exists, real_realpath = os.path.exists, os.path.realpath
        os.path.exists = lambda p: p in existing
        os.path.realpath = lambda p: p
        m.shutil_which = lambda n: (which or {}).get(n)
        try:
            return m._resolve_cysd()
        finally:
            os.path.exists, os.path.realpath = real_exists, real_realpath

    def test_wave_bundle_waved_wins_over_leftover_cys_app(self):
        self.assertEqual(self.run_resolve({WAVE, CYS}), WAVE)

    def test_leftover_cys_app_is_never_used(self):
        self.assertEqual(self.run_resolve({CYS}, {"waved": "/usr/local/bin/waved"}), "/usr/local/bin/waved")

    def test_falls_back_to_path_when_no_bundle(self):
        self.assertEqual(self.run_resolve(set(), {"cysd": "/opt/x/cysd"}), "/opt/x/cysd")


if __name__ == "__main__":
    unittest.main()
