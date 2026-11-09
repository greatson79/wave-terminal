"""Run the GUI's real Rust launch helper against a disposable bundled Python copy."""
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


def snapshot(root):
    return {str(p.relative_to(root)): (
        "link", os.readlink(p)) if p.is_symlink() else (
        "file", hashlib.sha256(p.read_bytes()).hexdigest()) if p.is_file() else (
        "dir", "") for p in root.rglob("*")}


@unittest.skipUnless(sys.platform == "darwin", "macOS bundled Python seal")
class BundlePythonSealTests(unittest.TestCase):
    def test_gui_python_launch_preserves_every_bundle_entry(self):
        source = Path(os.environ.get("WAVE_TEST_PYTHON_RUNTIME", ROOT / "src-tauri/runtime/python"))
        self.assertTrue((source / "bin/python3").is_file(), "prepare the bundled runtime first")
        with tempfile.TemporaryDirectory(prefix="bundle-seal-") as tmp:
            base = Path(tmp)
            bundle = base / "Wave Terminal.app"
            runtime = bundle / "Contents/Resources/runtime/python"
            shutil.copytree(source, runtime, symlinks=True)
            python = runtime / "bin/python3"
            fixture = runtime / "lib/python3.12/seal_fixture.py"
            fixture.write_text("VALUE = 1\n")
            env = dict(os.environ, HOME=str(base / "home"), WAVE_TEST_PYTHON=str(python))
            env.setdefault("CARGO_HOME", str(Path.home() / ".cargo"))
            env.setdefault("RUSTUP_HOME", str(Path.home() / ".rustup"))
            Path(env["HOME"]).mkdir()
            env.pop("PYTHONPYCACHEPREFIX", None)
            # A stale existing cache must stay unchanged, and uncached stdlib imports must add nothing.
            subprocess.run([str(python), "-B", "-c",
                            "import py_compile, sys; py_compile.compile(sys.argv[1], doraise=True)",
                            str(fixture)], env=env, check=True, capture_output=True)
            fixture.write_text("VALUE = 12345\n")
            before = snapshot(bundle)
            result = subprocess.run([
                "cargo", "test", "--offline", "--package", "wave-app",
                "tests::bundled_python_launch_probe", "--", "--ignored", "--exact", "--nocapture"],
                cwd=ROOT, env=env, capture_output=True, text=True, timeout=600)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("test tests::bundled_python_launch_probe ... ok", result.stdout)
            self.assertIn("1 passed; 0 failed", result.stdout)
            after = snapshot(bundle)
            added = sorted(after.keys() - before.keys())
            removed = sorted(before.keys() - after.keys())
            changed = sorted(k for k in before.keys() & after.keys() if before[k] != after[k])
            print(f"bundle entries before={len(before)} after={len(after)}; "
                  f"added={len(added)} changed={len(changed)} removed={len(removed)}", flush=True)
            self.assertEqual((added, changed, removed), ([], [], []),
                             f"added={added}, changed={changed}, removed={removed}")


if __name__ == "__main__":
    unittest.main()
