import importlib.util
from pathlib import Path
import unittest
import os
import subprocess
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

    def test_success_requires_an_executed_test(self):
        name = next(iter(module.CASES))
        self.assertEqual(module.classify(name, 0, "test result: ok. 1 passed; 0 failed;"), "PASS")
        with self.assertRaises(AssertionError):
            module.classify(name, 0, "test result: ok. 0 passed; 0 failed;")


if __name__ == "__main__":
    unittest.main()
