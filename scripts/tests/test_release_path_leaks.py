import importlib.util
import tarfile
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).resolve().parents[1] / "check-release-path-leaks.py"
SPEC = importlib.util.spec_from_file_location("release_path_gate", SCRIPT)
GATE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(GATE)


class ReleasePathGateTests(unittest.TestCase):
    def test_scans_every_file_and_detects_denied_root(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            payload = root / "payload"
            payload.mkdir()
            (payload / "clean").write_bytes(b"portable")
            (payload / "binary").write_bytes(b"prefix /checkout/private suffix")
            scanned, hits, _ = GATE.scan_tree(payload, [b"/checkout/private"])
            self.assertEqual(scanned, 2)
            self.assertEqual(hits, ["binary"])

    def test_asset_extraction_failure_is_not_a_pass(self):
        with tempfile.TemporaryDirectory() as tmp:
            missing = Path(tmp) / "missing.tar.gz"
            with self.assertRaises(ValueError):
                GATE.check_asset("mac-tar", missing, [b"/Users/"])

    def test_windows_path_match_ignores_case(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "binary.exe").write_bytes(b"D:\\A\\WORK\\REPO\\SRC")
            scanned, hits, _ = GATE.scan_tree(root, [b"d:\\a\\work\\repo"], case_insensitive=True)
            self.assertEqual(scanned, 1)
            self.assertEqual(hits, ["binary.exe"])

    def test_only_actual_username_home_paths_are_denied(self):
        patterns = GATE.needles("/build/work", "runner")
        self.assertIn(b"/Users/runner/", patterns)
        self.assertIn(b"/home/runner/", patterns)
        self.assertIn(b"C:\\Users\\runner\\", patterns)
        self.assertNotIn(b"/Users/", patterns)
        self.assertNotIn(b"runner", patterns)

    def test_marker_diagnostics_count_classes_without_echoing_marker_values(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "binary").write_bytes(b"/Users/runner/private")
            scanned, hits, categories = GATE.scan_tree(root, {
                "mac-home": [b"/Users/runner/"], "checkout-root": [b"/checkout/private"],
            })
            self.assertEqual(scanned, 1)
            self.assertEqual(hits, ["binary"])
            self.assertEqual(categories, {"mac-home": 1})


if __name__ == "__main__":
    unittest.main()
