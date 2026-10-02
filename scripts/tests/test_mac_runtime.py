"""Incomplete bundles must fail before any host tool can mask a missing runtime."""
import os
import pathlib
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
CHECK = ROOT / "scripts/verify-mac-runtime.sh"
TOOLS = ("python/bin/python3", "git/bin/git", "uv/uv", "uv/uvx",
         "node/bin/node", "node/bin/npm", "node/bin/npx")

class MacRuntimeGate(unittest.TestCase):
    def check_runtime(self, root, target="aarch64-apple-darwin"):
        return subprocess.run(["bash", str(CHECK), str(root), target],
                              capture_output=True, text=True)

    def test_placeholder_only_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            (root / ".gitkeep").touch()
            result = self.check_runtime(root)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("python/bin/python3", result.stderr)

    def test_each_missing_tool_is_rejected_before_execution(self):
        for missing in TOOLS:
            with self.subTest(missing=missing), tempfile.TemporaryDirectory() as tmp:
                root = pathlib.Path(tmp)
                for rel in TOOLS:
                    if rel == missing:
                        continue
                    path = root / rel
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text("#!/bin/sh\nexit 0\n")
                    path.chmod(0o755)
                result = self.check_runtime(root)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("missing executable runtime:", result.stderr)
                self.assertIn(missing, result.stderr)

    def test_unsupported_target_is_rejected(self):
        result = self.check_runtime("/unused", "x86_64-pc-windows-msvc")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("unsupported mac target", result.stderr)

    def test_direct_bundle_prepares_missing_runtime_and_stops_on_failure(self):
        # Execute the real entry point in a disposable project; no downloads/builds.
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            (root / "scripts").mkdir()
            (root / "ui").mkdir()
            (root / "scripts/bundle-prep.sh").write_text(
                (ROOT / "scripts/bundle-prep.sh").read_text())
            (root / "scripts/verify-mac-runtime.sh").write_text("exit 1\n")
            (root / "scripts/prep-mac-runtime.sh").write_text(
                "echo prepared > prepared\nexit 17\n")
            (root / "ui/build.sh").write_text("echo built > built\n")
            env = dict(os.environ, CYS_TARGET="aarch64-apple-darwin")
            result = subprocess.run(["sh", str(root / "scripts/bundle-prep.sh")], env=env)
            self.assertEqual(result.returncode, 17)
            self.assertTrue((root / "prepared").exists())
            self.assertFalse((root / "built").exists())

    def test_non_macho_runtime_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp)
            for rel in TOOLS:
                path = root / rel
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("#!/bin/sh\nexit 0\n")
                path.chmod(0o755)
            self.assertNotEqual(self.check_runtime(root).returncode, 0)

if __name__ == "__main__":
    unittest.main()
