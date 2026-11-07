#!/usr/bin/env python3
"""Native Windows python prints CRLF: the last field `read` from it must not keep a trailing CR."""
import os
from pathlib import Path
import re
import subprocess
import unittest

DEPT = Path(__file__).resolve().parents[1] / "cys-dept"
BASH = os.environ.get("CYS_TEST_BASH", "bash")
SOURCE = DEPT.read_text(encoding="utf-8")


def run(program):
    result = subprocess.run([BASH, "-c", program], capture_output=True, text=True, encoding="utf-8", timeout=10)
    assert result.returncode == 0, result.stderr
    return result.stdout


class CrlfTests(unittest.TestCase):
    def test_create_catalog_cwd_has_no_trailing_cr(self):
        block = re.search(r"^ *IFS=\$'\\t' read -r disp acctkey.*?(?=^ *# 부서별 config dir)", SOURCE, re.S | re.M).group(0)
        program = ("native_path(){ printf '%s\\n' \"$1\"; }\n"
                   "python3(){ printf 'd\\ta\\t/acct\\tm\\t/폴더/내 팀 폴더\\r\\n'; }\n"
                   "CAT=c key=k\n" + block + "\ncase \"$cwd\" in *$'\\r') echo HAS_CR;; *) echo \"NO_CR:$cwd\";; esac\n")
        self.assertEqual(run(program).strip(), "NO_CR:/폴더/내 팀 폴더")

    def test_daemon_identity_started_at_has_no_trailing_cr(self):
        lines = re.findall(r"^ *IFS=\$'\\t' read -r pid started_at <<<.*$", SOURCE, re.M)
        self.assertEqual(len(lines), 3)
        for line in lines:
            with self.subTest(line=line.strip()[:60]):
                program = ("daemon_identity(){ printf '101\\t1793966400\\r\\n'; }\nsock=s\n" + line +
                           "\ncase \"$started_at\" in *$'\\r') echo HAS_CR;; *) echo \"NO_CR:$started_at\";; esac\n")
                self.assertEqual(run(program).strip(), "NO_CR:1793966400")

    def test_reap_snapshot_row_has_no_trailing_cr(self):
        head = re.search(r"^ *while IFS=\$'\\t' read -r nm sk rat; do\n.*?\n", SOURCE, re.M).group(0)
        program = ("printf 'n\\tsk\\t1793966400\\r\\n' | {\n" + head +
                   "case \"$rat\" in *$'\\r') echo HAS_CR;; *) echo \"NO_CR:$rat\";; esac\ndone\n}\n")
        self.assertEqual(run(program).strip(), "NO_CR:1793966400")


if __name__ == "__main__":
    unittest.main()
