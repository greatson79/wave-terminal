"""Incomplete bundles must fail before any host tool can mask a missing runtime."""
import os
import json
import shutil
import pathlib
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[2]
CHECK = ROOT / "scripts/verify-mac-runtime.sh"
TOOLS = ("python/bin/python3", "git/bin/git", "uv/uv", "uv/uvx",
         "node/bin/node", "node/bin/npm", "node/bin/npx", "node/bin/corepack")

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

    def test_stale_node_symlinks_are_rejected_before_architecture_checks(self):
        for tool in ("npm", "npx", "corepack"):
            with self.subTest(tool=tool), tempfile.TemporaryDirectory() as tmp:
                root = pathlib.Path(tmp)
                for rel in TOOLS:
                    path = root / rel
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_text("#!/bin/sh\nexit 0\n")
                    path.chmod(0o755)
                entry = root / "node/bin" / tool
                entry.rename(entry.with_suffix(".target"))
                entry.symlink_to(tool + ".target")
                result = self.check_runtime(root)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("node CLI must be a regular shell wrapper:", result.stderr)
                self.assertIn(tool, result.stderr)

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

class NodeEntrypointCopyTests(unittest.TestCase):
    ENTRIES = {
        "npm": "../lib/node_modules/npm/bin/npm-cli.js",
        "npx": "../lib/node_modules/npm/bin/npx-cli.js",
        "corepack": "../lib/node_modules/corepack/dist/corepack.js",
    }

    def make_fixture(self, base):
        root = base / "node staging with spaces"
        (root / "bin").mkdir(parents=True)
        node = shutil.which("node")
        self.assertIsNotNone(node, "Node is required to exercise JavaScript resolution")
        (root / "bin/node").symlink_to(node)
        for name, target in self.ENTRIES.items():
            cli = root / "bin" / target
            cli.parent.mkdir(parents=True, exist_ok=True)
            dependency = "./lib/corepack.cjs" if name == "corepack" else "../lib/cli.js"
            cli.write_text("#!/usr/bin/env node\nrequire(" + json.dumps(dependency) + ");\n")
            cli.chmod(0o755)
            module = cli.parent / dependency
            module.parent.mkdir(parents=True, exist_ok=True)
            module.write_text("console.log(JSON.stringify(process.argv.slice(2))); process.exit(23);\n")
            (root / "bin" / name).symlink_to(target)
        return root

    def copied(self, root, dest):
        # Tauri's observed resource copy dereferences links. Keep the Node executable
        # external to this fixture; exercise the three JavaScript entrypoints only.
        shutil.copytree(root, dest, symlinks=False, ignore=shutil.ignore_patterns("node"))
        (dest / "bin/node").symlink_to((root / "bin/node").resolve())
        return dest

    def execute(self, root, name):
        env = dict(os.environ, PATH=str(root / "bin") + os.pathsep + os.environ['PATH'])
        return subprocess.run([str(root / "bin" / name), "one two", "", "*", "--flag=a b"],
                              capture_output=True, text=True, env=env)

    def test_dereferenced_original_entrypoints_lose_module_location(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = pathlib.Path(tmp)
            root = self.make_fixture(base)
            for name in self.ENTRIES:
                self.assertEqual(self.execute(root, name).returncode, 23)
            copied = self.copied(root, base / "original copied app")
            for name in self.ENTRIES:
                result = self.execute(copied, name)
                self.assertNotEqual(result.returncode, 23)
                self.assertIn("MODULE_NOT_FOUND", result.stderr)

    def test_wrappers_survive_copy_and_preserve_modules_arguments_and_exit(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = pathlib.Path(tmp)
            root = self.make_fixture(base)
            before = {target: (root / "bin" / target).read_bytes()
                      for target in self.ENTRIES.values()}
            for _ in range(2):  # Re-running must leave CLI modules intact too.
                result = subprocess.run(["bash", str(ROOT / "scripts/wrap-mac-node-tools.sh"), str(root)],
                                        capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)
            for name, target in self.ENTRIES.items():
                self.assertFalse((root / "bin" / name).is_symlink())
                self.assertEqual((root / "bin" / target).read_bytes(), before[target])
            copied = self.copied(root, base / "wrapped copied app")
            for name in self.ENTRIES:
                result = self.execute(copied, name)
                self.assertEqual(result.returncode, 23, result.stderr)
                self.assertEqual(json.loads(result.stdout), ["one two", "", "*", "--flag=a b"])

    def test_missing_corepack_target_rejects_before_replacing_any_entrypoint(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = self.make_fixture(pathlib.Path(tmp))
            target = root / "bin" / self.ENTRIES["corepack"]
            target.rename(target.with_suffix(".held"))
            result = subprocess.run(["bash", str(ROOT / "scripts/wrap-mac-node-tools.sh"), str(root)],
                                    capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("missing node CLI target", result.stderr)
            self.assertTrue(all((root / "bin" / name).is_symlink() for name in self.ENTRIES))

if __name__ == "__main__":
    unittest.main()
