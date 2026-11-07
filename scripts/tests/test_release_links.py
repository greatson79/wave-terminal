import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

GATE = Path(__file__).resolve().parents[1] / "check-release-links.py"
NEEDLE = "cys" + "insight"


class ReleaseLinkTests(unittest.TestCase):
    def check_fixture(self, approved, actual, git=True):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "scripts").mkdir()
            (root / "scripts/release-path-allowlist.json").write_text(json.dumps({
                "LICENSE": {"reason": "MIT copyright", "line_sha256": [hashlib.sha256(approved.encode()).hexdigest()]}
            }))
            (root / "LICENSE").write_text(actual + "\n")
            if git:
                subprocess.run(["git", "init", "-q", str(root)], check=True)
                subprocess.run(["git", "-C", str(root), "add", "LICENSE"], check=True)
            return subprocess.run([sys.executable, str(GATE)], cwd=root, capture_output=True, text=True)

    def test_exact_license_line_allowed(self):
        self.assertEqual(self.check_fixture(NEEDLE, NEEDLE).returncode, 0)

    def test_changed_license_line_denied(self):
        self.assertEqual(self.check_fixture(NEEDLE, NEEDLE + ".com/download").returncode, 1)

    def test_case_insensitive_unapproved_reference_denied(self):
        self.assertEqual(self.check_fixture("MIT", NEEDLE.upper()).returncode, 1)

    def test_failed_git_scan_is_not_clean(self):
        self.assertEqual(self.check_fixture(NEEDLE, NEEDLE, git=False).returncode, 2)


if __name__ == "__main__":
    unittest.main()
