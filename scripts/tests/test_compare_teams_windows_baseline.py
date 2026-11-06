import importlib.util
from pathlib import Path
import unittest
import os
import subprocess
import sys
import tempfile

spec = importlib.util.spec_from_file_location("baseline_compare", Path(__file__).with_name("compare_teams_windows_baseline.py"))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class ClassificationTests(unittest.TestCase):
    def test_only_executed_expected_assertion_is_a_known_failure(self):
        for name, marker in module.CASES.items():
            output = f"test {name} ... FAILED\n{marker}\ntest result: FAILED. 0 passed; 1 failed;"
            self.assertEqual(module.classify(name, 101, output), "EXPECTED_ASSERTION_FAILURE")
            for malformed in ["could not compile", output.replace(marker, "different failure"), output.replace("1 failed", "0 failed")]:
                with self.assertRaises(AssertionError):
                    module.classify(name, 101, malformed)

    def test_actual_captured_rust_failure_is_classified(self):
        name = "pack::tests::install_from_iter_equivalent_to_install"
        marker = module.CASES[name]
        with tempfile.TemporaryDirectory(prefix="baseline-parser-") as directory:
            source = Path(directory) / "case.rs"
            binary = Path(directory) / ("case.exe" if os.name == "nt" else "case")
            source.write_text('mod pack { mod tests { #[test] fn install_from_iter_equivalent_to_install() { panic!("' + marker + '"); } } }', encoding="utf-8")
            subprocess.run(["rustc", "--test", str(source), "-o", str(binary)], check=True, capture_output=True)
            result = subprocess.run([str(binary), "--exact", name, "--test-threads=1"], capture_output=True, text=True, encoding="utf-8")
            self.assertEqual(result.returncode, 101)
            self.assertIn("failures:", result.stdout)
            self.assertEqual(module.classify(name, result.returncode, result.stdout), "EXPECTED_ASSERTION_FAILURE")

    def test_cli_prints_korean_as_utf8_with_cp1252_stdout(self):
        script = Path(module.__file__).resolve()
        wrapper = r'''import runpy, subprocess, sys
from pathlib import Path
from unittest.mock import patch
script, root = Path(sys.argv[1]), Path(sys.argv[2])
namespace = runpy.run_path(str(script))
assert sys.stdout.encoding == "cp1252"
def captured_case(command, **kwargs):
    name = command[5]
    marker = namespace["CASES"][name]
    output = f"test {name} ... FAILED\n{marker}\ntest result: FAILED. 0 passed; 1 failed;"
    return subprocess.CompletedProcess(command, 101, stdout=output)
def git_output(command, **kwargs):
    return namespace["BASELINE"] if "rev-parse" in command else ""
sys.argv = [str(script), "--baseline", str(root / "기준"), "--candidate", str(root / "후보"), "--output", str(root / "내 팀 폴더")]
with patch("subprocess.run", captured_case), patch("subprocess.check_output", git_output):
    runpy.run_path(str(script), run_name="__main__")
'''
        with tempfile.TemporaryDirectory(prefix="baseline-encoding-") as directory:
            environment = dict(os.environ, PYTHONIOENCODING="cp1252", PYTHONUTF8="0")
            result = subprocess.run([sys.executable, "-c", wrapper, str(script), directory],
                                    env=environment, capture_output=True, timeout=30)
            self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", errors="replace"))
            output = result.stdout.decode("utf-8")
            for marker in module.CASES.values():
                self.assertIn(marker, output)
            self.assertIn("내 팀 폴더", output)
            self.assertTrue((Path(directory) / "내 팀 폴더" / "comparison.json").is_file())

    def test_success_requires_an_executed_test(self):
        name = next(iter(module.CASES))
        self.assertEqual(module.classify(name, 0, "test result: ok. 1 passed; 0 failed;"), "PASS")
        with self.assertRaises(AssertionError):
            module.classify(name, 0, "test result: ok. 0 passed; 0 failed;")


if __name__ == "__main__":
    unittest.main()
