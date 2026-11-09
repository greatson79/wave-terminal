#!/usr/bin/env python3
"""Structural tests for the 0.3.1 -> 0.3.2 update proof (workflow + rc manifest script + release.yml tag exclusion).
Theo's rc conditions: prerelease, never latest, latest.json never the source/target, deleted afterwards (by hand, by Pulse)."""
import importlib.util, json, re, subprocess, sys, tempfile, unittest
from pathlib import Path
import yaml

ROOT = Path(__file__).resolve().parents[2]
WF = ROOT / ".github/workflows/windows-update-proof.yml"
REL = ROOT / ".github/workflows/release.yml"
spec = importlib.util.spec_from_file_location("make_rc_manifest", ROOT / "scripts/make-rc-manifest.py")
mrc = importlib.util.module_from_spec(spec); spec.loader.exec_module(mrc)


def release_excludes_rc(text):
    doc = yaml.safe_load(text)
    tags = (doc.get(True) or doc.get("on"))["push"]["tags"]
    return "v*" in tags and "!v*-rc.*" in tags


def proof_violations(text):
    """Everything the proof workflow must NOT do or MUST do."""
    bad = []
    yaml.safe_load(text)
    for forbidden in ("gh release create", "gh release delete", "gh release edit", "--latest", "git tag", "git push", "contents: write"):
        if forbidden in text: bad.append("forbidden: " + forbidden)
    for required in ("isPrerelease", "CYS_UPDATE_MANIFEST_URL", "latest-rc.json", "CYS_AUTOTEST_PATCH_INSTALL", "wave-install-kill.log",
                     "exit=128", "channel latest.json points at the rc", "CHANNEL_UNCHANGED", "(a) FAILED"):
        if required not in text: bad.append("missing: " + required)
    if re.search(r"CYS_UPDATE_MANIFEST_URL\s*=\s*\"[^\"]*releases/latest/", text): bad.append("update source is the channel")
    if re.search(r"CYS_UPDATE_MANIFEST_URL\s*=\s*\"[^\"]*/latest\.json", text): bad.append("update source is latest.json")
    if "@('cysd.exe','cys.exe') | Where-Object { $seen[$_] -ne 128 }" not in text: bad.append("daemon/CLI kill must be exit 128")
    return bad


class UpdateProof(unittest.TestCase):
    def test_workflow_is_read_only_and_points_at_the_rc_manifest(self):
        self.assertEqual(proof_violations(WF.read_text(encoding="utf-8")), [])

    def test_negative_controls_the_checker_catches_each_violation(self):
        text = WF.read_text(encoding="utf-8")
        self.assertTrue(proof_violations(text + "\n# gh release edit v1 --latest\n"))
        self.assertTrue(proof_violations(text.replace("latest-rc.json", "latest.json")))
        self.assertTrue(proof_violations(text.replace("isPrerelease", "isDraft")))
        self.assertTrue(proof_violations(text.replace("-ne 128 }", "-ne 0 }")))
        self.assertTrue(proof_violations(text.replace("CHANNEL_UNCHANGED", "")))

    def test_release_yml_does_not_build_on_rc_tags(self):
        self.assertTrue(release_excludes_rc(REL.read_text(encoding="utf-8")))
        self.assertFalse(release_excludes_rc(REL.read_text(encoding="utf-8").replace("      - '!v*-rc.*'", "")), "negative control")

    def test_manifest_script(self):
        with tempfile.TemporaryDirectory() as tmp:
            sig = Path(tmp) / "x.sig"; sig.write_text("SIGTEXT\n")
            doc = mrc.build("v0.3.2-rc.1", "greatson79/wave-terminal", {"windows-x86_64": str(sig)})
            self.assertEqual(doc["version"], "0.3.2")
            url = doc["platforms"]["windows-x86_64"]["url"]
            self.assertEqual(url, "https://github.com/greatson79/wave-terminal/releases/download/v0.3.2-rc.1/wave-terminal-0.3.2-windows-x64-setup.exe")
            r = subprocess.run([sys.executable, str(ROOT / "scripts/make-rc-manifest.py"), "v0.3.2-rc.1", "greatson79/wave-terminal", tmp, "windows-x86_64=" + str(sig)], capture_output=True, text=True)
            self.assertEqual(r.returncode, 0, r.stderr)
            self.assertTrue((Path(tmp) / "latest-rc.json").exists()); self.assertFalse((Path(tmp) / "latest.json").exists())
            for bad_tag in ("v0.3.2", "v0.3.2-rc", "v0.3.2-rc.1.json", "0.3.2-rc.1", "v0.3.2-beta.1"):
                with self.subTest(tag=bad_tag), self.assertRaises(SystemExit):
                    mrc.build(bad_tag, "greatson79/wave-terminal", {"windows-x86_64": str(sig)})
            with self.assertRaises(SystemExit): mrc.build("v0.3.2-rc.1", "greatson79/wave-terminal", {})
            with self.assertRaises(SystemExit): mrc.build("v0.3.2-rc.1", "greatson79/wave-terminal", {"linux": str(sig)})
            sig.write_text("")
            with self.assertRaises(SystemExit): mrc.build("v0.3.2-rc.1", "greatson79/wave-terminal", {"windows-x86_64": str(sig)})


if __name__ == "__main__":
    unittest.main()
