#!/usr/bin/env python3
"""Unix socket path length pre-check in cys-dept (UTF-8 BYTES incl. NUL: macOS/BSD 104, Linux 108; Windows pipes unchecked).
Cross-checks the pack's limits + Korean message against the Rust helper in src/lib.rs so they cannot drift.
CYS_LIB_RS=<file> points at another src/lib.rs (default: this repo's)."""
import os
import platform
import re
import subprocess
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[3]
DEPT = Path(__file__).resolve().parents[1] / "cys-dept"
LIB_RS = Path(os.environ.get("CYS_LIB_RS") or REPO / "src" / "lib.rs")
BASH = os.environ.get("CYS_TEST_BASH", "bash")
DARWIN = platform.system() == "Darwin"
CAP = 104 if DARWIN else 108


def pack_consts():
    s = DEPT.read_text(encoding="utf-8")
    return (int(re.search(r"^DEPT_SOCK_CAP_BSD=(\d+)", s, re.M).group(1)), int(re.search(r"^DEPT_SOCK_CAP_OTHER=(\d+)", s, re.M).group(1)),
            re.search(r"^DEPT_SOCK_TOO_LONG='([^']+)'", s, re.M).group(1))


def name_for_total(home, total):
    """A dept name making $home/.local/state/cys-dept-<name>/cys.sock exactly `total` UTF-8 bytes (Korean + ASCII mix)."""
    fixed = len((home + "/.local/state/cys-dept-").encode()) + len("/cys.sock")
    room = total - fixed
    assert room >= 1, "home too long for this total"
    korean, rest = divmod(room, 3)
    return "미" * korean + "a" * rest


class SockLimit(unittest.TestCase):
    def run_verb(self, home, *args, role="cso"):
        env = {**os.environ, "HOME": home, "CYS_DEPTS_JSON": home + "/depts.json", "CYS_PACK_DIR": home + "/pack"}
        env.pop("CYS_ROLE", None)
        return subprocess.run([BASH, str(DEPT), *args], env=env, capture_output=True, text=True, timeout=60, stdin=subprocess.DEVNULL)

    def check(self, home, name):
        script = DEPT.read_text(encoding="utf-8").split('cmd="${1:?usage:', 1)[0] + '\ndept_sock_check "$1"; echo PROCEEDS\n'
        with tempfile.TemporaryDirectory() as tmp:
            f = Path(tmp) / "t.sh"; f.write_text(script)
            return subprocess.run([BASH, str(f), name], env={**os.environ, "HOME": home}, capture_output=True, text=True, timeout=30)

    def test_pack_limits_and_message_equal_the_rust_helper(self):
        rs = LIB_RS.read_text(encoding="utf-8")
        self.assertTrue("SOCKET_PATH_TOO_LONG_MSG" in rs, "Rust helper missing (the sock diff must be merged with this change): " + str(LIB_RS))
        msg = re.search(r'pub const SOCKET_PATH_TOO_LONG_MSG: &str = "([^"]+)";', rs).group(1)
        caps = re.search(r"\{\s*(\d+)\s*\}\s*else\s*\{\s*(\d+)\s*\}", rs[rs.index("unix_sun_path_capacity"):])
        bsd, other, pack_msg = pack_consts()
        self.assertEqual((bsd, other), (int(caps.group(1)), int(caps.group(2))), "pack/Rust capacity drifted")
        self.assertEqual(pack_msg, msg, "pack/Rust Korean message drifted")
        # same rule: len + NUL <= capacity  <=>  len < capacity
        self.assertRegex(rs, r"pub const fn socket_path_fits\(len: usize, capacity: usize\) -> bool \{\s*len < capacity\s*\}")

    @unittest.skipIf(os.name == "nt", "Windows named pipes have no path-length check")
    def test_limit_minus_one_byte_proceeds_and_limit_byte_is_refused(self):
        home = "/tmp/h"
        ok = self.check(home, name_for_total(home, CAP - 1))
        self.assertIn("PROCEEDS", ok.stdout, ok.stderr)
        bad = self.check(home, name_for_total(home, CAP))
        self.assertNotIn("PROCEEDS", bad.stdout)
        self.assertEqual(bad.returncode, 6)
        self.assertEqual(bad.stderr.strip(), pack_consts()[2])

    @unittest.skipIf(os.name == "nt", "Windows named pipes have no path-length check")
    def test_char_count_below_limit_but_bytes_above_is_refused(self):
        home = "/tmp/" + "최" * 12   # 12 Korean chars (36 bytes) — char count looks short, bytes are not
        name = "미" * 11
        total = len((home + "/.local/state/cys-dept-" + name + "/cys.sock").encode())
        self.assertGreaterEqual(total, 100)
        r = self.check(home, name)
        if total >= CAP:
            self.assertNotIn("PROCEEDS", r.stdout); self.assertEqual(r.returncode, 6)
        else:
            self.assertIn("PROCEEDS", r.stdout)

    @unittest.skipIf(os.name == "nt", "Windows named pipes have no path-length check")
    def test_long_korean_home_and_dept_are_refused_and_nothing_is_created(self):
        with tempfile.TemporaryDirectory(dir="/tmp", prefix="sl-") as tmp:
            home = tmp + "/" + "최" * 22
            os.mkdir(home)
            before = sorted(os.listdir(home))
            name = "개" * 14
            for args in (("launch", name), ("allocate",), ("create", "any-key")):
                with self.subTest(verb=args[0]):
                    r = self.run_verb(home, *args)
                    self.assertEqual(r.returncode, 6, r.stdout + r.stderr)
                    self.assertEqual(r.stderr.strip(), pack_consts()[2])
                    self.assertEqual(sorted(os.listdir(home)), before, "refusal must happen before anything is created")
                    self.assertFalse(Path(home, "depts.json").exists())


if __name__ == "__main__":
    unittest.main()
