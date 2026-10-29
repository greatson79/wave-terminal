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
            scanned, hits, _, _, _ = GATE.scan_tree(payload, [b"/checkout/private"])
            self.assertEqual(scanned, 2)
            self.assertEqual(hits, ["binary"])

    def test_asset_extraction_failure_is_not_a_pass(self):
        with tempfile.TemporaryDirectory() as tmp:
            missing = Path(tmp) / "missing.tar.gz"
            with self.assertRaises(ValueError):
                GATE.check_asset("mac-tar", missing, GATE.needle_groups("/ci/work", "runner", "kylechoi"))

    def test_windows_path_match_ignores_case(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "binary.exe").write_bytes(b"D:\\A\\WORK\\REPO\\SRC")
            scanned, hits, _, _, _ = GATE.scan_tree(root, [b"d:\\a\\work\\repo"], case_insensitive=True)
            self.assertEqual(scanned, 1)
            self.assertEqual(hits, ["binary.exe"])

    def test_private_identity_is_denied_and_ci_paths_are_diagnostic_only(self):
        patterns = GATE.needle_groups("D:\\a\\repo", "runneradmin", "kylechoi")
        self.assertIn(b"kylechoi", patterns["owner-identity"])
        self.assertIn(b"/Users/" + b"kylechoi/", patterns["owner-local-home"])
        self.assertIn(b"/Users/" + b"runneradmin/", patterns["ci-home"])
        self.assertIn(b"D:\\a\\repo", patterns["ci-checkout"])
        self.assertNotIn(b"/Users/", [marker for group in patterns.values() for marker in group])

    def test_marker_diagnostics_count_classes_without_echoing_marker_values(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "binary").write_bytes(b"/Users/runner/private")
            scanned, hits, categories, components, samples = GATE.scan_tree(root, {
                "ci-home": [b"/Users/runner/"], "ci-checkout": [b"/checkout/private"],
            })
            self.assertEqual(scanned, 1)
            self.assertEqual(hits, ["binary"])
            self.assertEqual(categories, {"ci-files": 1, "ci-home": 1, "private-files": 0, "unreadable": 0})
            self.assertEqual(components, {"binary": 1})
            self.assertEqual(samples, {"binary": "binary"})

    def test_private_owner_markers_are_detected_case_insensitively_by_windows_gate(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "binary.exe").write_bytes(b"C:\\Users\\KYLECHOI\\profile")
            scanned, hits, categories, _, _ = GATE.scan_tree(root, {
                "owner-identity": [b"kylechoi"], "ci-home": [b"C:\\Users\\runneradmin\\"]
            }, case_insensitive=True)
            self.assertEqual(scanned, 1)
            self.assertEqual(hits, ["binary.exe"])
            self.assertEqual(categories, {"owner-identity": 1, "private-files": 1, "ci-files": 0, "unreadable": 0})


if __name__ == "__main__":
    unittest.main()
