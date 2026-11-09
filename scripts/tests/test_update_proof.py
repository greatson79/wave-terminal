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
                     "channel latest.json points at the rc", "CHANNEL_UNCHANGED",
                     "(1) FAILED", "CYSD_FORWARDER_OK", "cysd.prev*.exe", "(2) FAILED", "KILL_LOG_OK", "(3) FAILED", "OLD_DAEMON_EXITED", "old-daemon.json"):
        if required not in text: bad.append("missing: " + required)
    if re.search(r"CYS_UPDATE_MANIFEST_URL\s*=\s*\"[^\"]*releases/latest/", text): bad.append("update source is the channel")
    if re.search(r"CYS_UPDATE_MANIFEST_URL\s*=\s*\"[^\"]*/latest\.json", text): bad.append("update source is latest.json")
    if "$seen.ContainsKey('cysd.exe')" not in text: bad.append("kill log must reject a cysd.exe line")
    if re.search(r"taskkill[^\n]*cysd", text): bad.append("proof must never kill cysd.exe itself")
    if "foreach ($image in 'wave-app.exe','cys-app.exe','cys.exe')" not in text: bad.append("kill log must require wave-app/cys-app/cys lines")
    return bad


class UpdateProof(unittest.TestCase):
    def test_workflow_is_read_only_and_points_at_the_rc_manifest(self):
        self.assertEqual(proof_violations(WF.read_text(encoding="utf-8")), [])

    def test_negative_controls_the_checker_catches_each_violation(self):
        text = WF.read_text(encoding="utf-8")
        self.assertTrue(proof_violations(text + "\n# gh release edit v1 --latest\n"))
        self.assertTrue(proof_violations(text.replace("latest-rc.json", "latest.json")))
        self.assertTrue(proof_violations(text.replace("isPrerelease", "isDraft")))
        self.assertTrue(proof_violations(text.replace("$seen.ContainsKey('cysd.exe')", "$false")))        # cysd.exe line no longer rejected
        self.assertTrue(proof_violations(text.replace("OLD_DAEMON_EXITED", "OLD_DAEMON_SEEN")))           # old daemon exit not proven
        self.assertTrue(proof_violations(text.replace("(1) FAILED", "(1) NOTE")))                          # swap failure not fatal
        self.assertTrue(proof_violations(text + "\n# nsExec taskkill /F /IM cysd.exe\n"))                 # re-adding the kill
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


def resolve_step(text):
    doc = yaml.safe_load(text)
    return next(st for st in doc["jobs"]["update-proof"]["steps"] if st.get("name", "").startswith("Resolve inputs"))


def validate_inputs_file(text, cfg):
    """Apply the Resolve-inputs step's own three regexes (extracted from the workflow) to an updproof-inputs.json document."""
    run = resolve_step(text)["run"]
    pats = re.findall(r"\$(?:rc|base|repo) -notmatch '([^']+)'", run)
    if len(pats) != 3: return ["resolve step must validate rc, base and repo"]
    bad = []
    for key, pat in zip(("rc_tag", "base_tag", "repo"), pats):
        if not re.search(pat, str(cfg.get(key, ""))): bad.append(key)
    return bad


class UpdateProofTrigger(unittest.TestCase):
    TEXT = WF.read_text(encoding="utf-8")

    def test_push_trigger_on_the_proof_branch_and_dispatch_kept(self):
        on = yaml.safe_load(self.TEXT)[True]
        self.assertEqual(on["push"]["branches"], ["release/0.3.2-updproof"])
        self.assertEqual(on["push"]["paths"], [".github/updproof-inputs.json", ".github/workflows/windows-update-proof.yml"])
        self.assertIn("workflow_dispatch", on)
        self.assertEqual(yaml.safe_load(self.TEXT)["permissions"], {"contents": "read"})
        self.assertEqual(proof_violations(self.TEXT), [])

    def test_inputs_file_is_read_and_dispatch_inputs_override_it(self):
        run = resolve_step(self.TEXT)["run"]
        self.assertIn(".github/updproof-inputs.json", run)
        for key in ("IN_RC", "IN_BASE", "IN_REPO"):   # dispatch value first, file second
            self.assertRegex(run, r"if \(\$env:%s\) \{ \$env:%s \} elseif \(\$cfg\)" % (key, key))
        self.assertIn("GITHUB_ENV", run)
        self.assertNotIn("RC_TAG: ${{", self.TEXT, "job env must not pin the dispatch inputs (empty on push)")

    def test_inputs_file_validation_and_negative_controls(self):
        good = {"rc_tag": "v0.3.2-rc.2", "base_tag": "v0.3.1", "repo": "greatson79/wave-terminal"}
        self.assertEqual(validate_inputs_file(self.TEXT, good), [])
        for key, value in (("rc_tag", "v0.3.2"), ("rc_tag", "v0.3.2-rc"), ("rc_tag", "latest"), ("rc_tag", "v0.3.2-rc.1; x"),
                           ("base_tag", "0.3.1"), ("base_tag", ""), ("repo", "no-slash"), ("repo", "a/b/c"), ("repo", "")):
            with self.subTest(key=key, value=value):
                self.assertEqual(validate_inputs_file(self.TEXT, {**good, key: value}), [key])
        loosened = self.TEXT.replace(r"'^v\d+\.\d+\.\d+-rc\.\d+$'", "'.*'", 1)
        self.assertNotEqual(loosened, self.TEXT)
        self.assertEqual(validate_inputs_file(loosened, {**good, "rc_tag": "v0.3.2"}), [], "checker is sensitive: a loosened regex lets a bad tag through")


if __name__ == "__main__":
    unittest.main()
