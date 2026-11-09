#!/usr/bin/env python3
"""CI network hardening: every setup-bun has a guarded retry twin, every network curl retries (windows-build, macos-runtime-build, prep-mac-runtime)."""
import re, unittest
from pathlib import Path
import yaml

ROOT = Path(__file__).resolve().parents[2]
WORKFLOWS = [ROOT / ".github/workflows/windows-build.yml", ROOT / ".github/workflows/macos-runtime-build.yml",
             ROOT / ".github/workflows/release.yml", ROOT / ".github/workflows/pack-release.yml"]
CURL_FILES = [WORKFLOWS[0], ROOT / "scripts/prep-mac-runtime.sh", WORKFLOWS[2]]


def bun_problems(text):
    bad = []
    for job_name, job in yaml.safe_load(text)["jobs"].items():
        steps = job.get("steps", [])
        for i, step in enumerate(steps):
            if not str(step.get("uses", "")).startswith("oven-sh/setup-bun"): continue
            if step.get("if", "").startswith("steps.") and "failure" in step.get("if", ""): continue  # a twin, checked via its primary
            sid = step.get("id")
            if not sid or step.get("continue-on-error") is not True: bad.append("%s step %d: needs id + continue-on-error: true" % (job_name, i)); continue
            twin = steps[i + 1] if i + 1 < len(steps) else {}
            if (twin.get("uses"), twin.get("with")) != (step["uses"], step.get("with")) or twin.get("if") != "steps.%s.outcome == 'failure'" % sid:
                bad.append("%s step %d: no identical guarded retry twin" % (job_name, i))
    return bad


def curl_problems(text):
    bad = []
    for n, line in enumerate(text.splitlines(), 1):
        code = line.strip()
        if code.startswith("#") or not re.match(r"^curl\s", code): continue
        if "--retry-all-errors" not in code or not re.search(r"--retry 5\b", code) or not re.search(r"\s-f\w*\b", code):
            bad.append("line %d: %s" % (n, code[:90]))
    return bad


class NetRetry(unittest.TestCase):
    def test_every_setup_bun_has_a_guarded_retry_twin(self):
        for wf in WORKFLOWS:
            with self.subTest(wf=wf.name):
                text = wf.read_text(encoding="utf-8")
                self.assertGreaterEqual(text.count("oven-sh/setup-bun"), 2)
                self.assertEqual(bun_problems(text), [])

    def test_every_curl_download_retries(self):
        for f in CURL_FILES:
            with self.subTest(f=f.name):
                text = f.read_text(encoding="utf-8")
                self.assertTrue(re.search(r"(?m)^\s*curl\s", text))
                self.assertEqual(curl_problems(text), [])

    def test_negative_controls(self):
        win = WORKFLOWS[0].read_text(encoding="utf-8"); mac = WORKFLOWS[1].read_text(encoding="utf-8")
        self.assertTrue(bun_problems(win.replace("        continue-on-error: true", "        #", 1)))
        self.assertTrue(bun_problems(win.replace("if: steps.bun.outcome == 'failure'", "if: always()")))
        self.assertTrue(bun_problems(mac.replace("      - if: steps.bun.outcome == 'failure'\n        uses: oven-sh/setup-bun@v2\n", "")))
        sh = CURL_FILES[1].read_text(encoding="utf-8")
        self.assertTrue(curl_problems(sh.replace("--retry-all-errors ", "", 1)))
        self.assertTrue(curl_problems(win.replace("--retry 5 ", "", 1)))
        # r2: release.yml and pack-release.yml
        rel = WORKFLOWS[2].read_text(encoding="utf-8"); pack = WORKFLOWS[3].read_text(encoding="utf-8")
        self.assertTrue(bun_problems(rel.replace("        continue-on-error: true", "        #", 1)))
        self.assertTrue(bun_problems(rel.replace("if: steps.bun.outcome == 'failure'", "if: always()", 1)))
        self.assertTrue(bun_problems(pack.replace("        continue-on-error: true", "        #", 1)))
        self.assertTrue(bun_problems(pack.replace("if: steps.bun.outcome == 'failure'", "if: always()")))
        self.assertTrue(curl_problems(rel.replace("--retry-all-errors ", "", 1)))

    def test_release_tag_filter_for_rc_is_untouched(self):
        self.assertIn("      - '!v*-rc.*'", WORKFLOWS[2].read_text(encoding="utf-8"))


if __name__ == "__main__":
    unittest.main()
