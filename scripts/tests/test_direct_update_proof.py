#!/usr/bin/env python3
"""Structural tests for windows-direct-update-proof.yml (0.3.1 -> 0.3.2 without the release-download chain for the new build)."""
import re, unittest
from pathlib import Path
import yaml

WF = Path(__file__).resolve().parents[2] / ".github/workflows/windows-direct-update-proof.yml"


def problems(text):
    bad = []
    doc = yaml.safe_load(text)
    on = doc[True]
    if on["push"]["branches"] != ["release/0.3.2-directproof"] or on["push"]["paths"] != [".github/directproof-inputs.json", ".github/workflows/windows-direct-update-proof.yml"]: bad.append("trigger")
    if doc["permissions"] != {"contents": "read", "actions": "read"}: bad.append("permissions must be contents+actions read")
    for forbidden in ("gh release create", "gh release delete", "gh release edit", "--latest", "latest.json", "latest-rc.json", "git push", "git tag", "contents: write", "CYS_UPDATE_MANIFEST_URL"):
        if forbidden in text: bad.append("forbidden: " + forbidden)
    if re.search(r"cysd\.exe'\)\)\.VersionInfo|\$fwd\b", text): bad.append("cysd.exe must be judged by hash, not ProductVersion (forwarders carry no version resource)")
    if re.search(r"taskkill[^\n]*cysd|Stop-Process[^\n]*cysd|Stop-Process -Name cysd", text): bad.append("a kill of cysd was added")
    if text.count("gh release download") != 2 or "-p '*windows-x64-setup.exe'" not in text or "SHA256SUMS.txt" not in text: bad.append("only the base installer (+ its SHA256SUMS) may be fetched from a release")
    for need in ("expected_sha", "head_sha -ne $env:EXPECTED_SHA", "conclusion -ne 'success'", "actions/download-artifact@v4", "cys-windows-x64-nsis", "run-id: ${{ env.BUILD_RUN_ID }}",
                 "$delay = [Math]::Min($delay * 2, 120)", "TotalMinutes -lt 10",
                 "@('/P', '/R', '/UPDATE', '/ARGS')", "updater.rs:808-815", "OLD_DAEMON_RUNNING_AT_INSTALL", "$inst.ExitCode -ne 0",
                 "(1) FAILED", "(1) FAILED — no cysd.prev*.exe", "CYSD_FORWARDER_OK", "cysd.prev*.exe", "7-Zip\\7z.exe", "& $sevenZip e -y", "INSTALLED_EQUALS_INSTALLER", "if ($have -ne $want)", "-eq $cysdSha", "(2) FAILED", "KILL_LOG_OK", "$seen.ContainsKey('cysd.exe')",
                 "foreach ($image in 'wave-app.exe','cys-app.exe','cys.exe')", "(3) FAILED", "OLD_DAEMON_EXITED", "TotalSeconds -lt 120",
                 "upd-seat-a", "upd-seat-b", "waved.exe", "DIRECT_UPDATE_PROOF_PASS", "processes_timeline.txt", "CopyFromScreen", "KILL_LOG_EXISTS"):
        if need not in text: bad.append("missing: " + need)
    steps = doc["jobs"]["direct-update-proof"]["steps"]
    cap = [s for s in steps if s.get("name", "").startswith("Capture evidence")]
    if len(cap) != 1 or cap[0].get("if") != "always()": bad.append("capture step must be always()")
    up = [s for s in steps if str(s.get("uses", "")).startswith("actions/upload-artifact")]
    if not up or "steps.scan.outcome == 'success'" not in str(up[-1].get("if")) or not str(up[-1].get("if")).startswith("always()") or up[-1]["with"]["path"] != "evidence": bad.append("artifact upload must be always() and gated on the scan")
    names = [s.get("name", "") for s in steps]
    sc = [i for i, n in enumerate(names) if n.startswith("Secret-scan the evidence bundle")]
    if len(sc) != 1 or steps[sc[0]].get("id") != "scan" or steps[sc[0]].get("if") != "always()" or "exit 1" not in steps[sc[0]]["run"] or "names only" not in steps[sc[0]]["run"]: bad.append("secret scan step")
    elif steps.index(up[-1]) < sc[0]: bad.append("scan must precede the upload")
    cap_run = cap[0]["run"] if cap else ""
    if "$allow = @(" not in cap_run or "ALLOWED_LOG_COPIED" not in cap_run: bad.append("capture must use an exact-path allow-list")
    if re.search(r"foreach \(\$root in|-notmatch 'EBWebView|Get-ChildItem[^\n]*-Recurse -File[^\n]*\$root", cap_run): bad.append("no directory sweep / deny-list")
    return bad


def hash_check(installed_cysd, installer_cysd, prev_hashes):
    """Python mirror of the workflow's check (1): FULL sha256 equality with the installer's own cysd.exe, prev present and different."""
    return installed_cysd == installer_cysd and len(prev_hashes) > 0 and all(p != installed_cysd for p in prev_hashes)


class DirectProof(unittest.TestCase):
    TEXT = WF.read_text(encoding="utf-8")

    def test_workflow_meets_the_bar(self):
        self.assertEqual(problems(self.TEXT), [])

    def test_negative_controls(self):
        t = self.TEXT
        for label, bad in (("release edit", t + "\n# gh release edit v1 --latest\n"), ("latest.json", t + "\n# latest.json\n"),
                           ("write perms", t.replace("  contents: read\n", "  contents: write\n", 1)),
                           ("cysd kill", t + "\n# taskkill /F /IM cysd.exe\n"),
                           ("no cysd line check", t.replace("$seen.ContainsKey('cysd.exe')", "$false")),
                           ("no old daemon exit", t.replace("OLD_DAEMON_EXITED", "OLD_DAEMON_SEEN")),
                           ("installer exit ignored", t.replace("$inst.ExitCode -ne 0", "$false")),
                           ("wrong updater args", t.replace("@('/P', '/R', '/UPDATE', '/ARGS')", "@('/S')")),
                           ("sha not bound", t.replace("head_sha -ne $env:EXPECTED_SHA", "$false")),
                           ("capture not always", t.replace("exes)\n        if: always()\n", "exes)\n", 1)),
                           ("version check back", t.replace("if ($have -ne $want)", "if ((Get-Item (Join-Path $dir 'cysd.exe')).VersionInfo.ProductVersion -notmatch '^0')")),
                           ("hash mismatch passes", t.replace("if ($have -ne $want)", "if ($false)")),
                           ("prev not required", t.replace("no cysd.prev*.exe", "x")),
                           ("deny-list sweep back", t.replace("$allow = @(", "$roots = @(")),
                           ("upload not gated", t.replace("steps.scan.outcome == 'success'", "true")),
                           ("scan does not fail", t.replace("exit 1", "exit 0")),
                           ("sweep reintroduced", t.replace("$allow = @(", "foreach ($root in @(1)) {}\n          $allow = @(", 1)),
                           ("extra release fetch", t + "\n# gh release download v0.3.2-rc.1\n")):
            with self.subTest(label=label):
                self.assertNotEqual(bad, t)
                self.assertTrue(problems(bad), label)


class HashCheckModel(unittest.TestCase):
    NEW = "05AEE44035E0F749" + "0" * 48          # the 0.3.2 forwarder (full 64-hex sha256)
    OLD = "9F" + "1" * 62                          # the 0.3.1 daemon image (cysd.prev.exe)

    def test_new_forwarder_passes_and_the_031_image_fails(self):
        self.assertTrue(hash_check(self.NEW, self.NEW, [self.OLD]))
        self.assertFalse(hash_check(self.OLD, self.NEW, [self.OLD]), "the 0.3.1 image (cysd.prev.exe hash) installed as cysd.exe must FAIL")
        self.assertFalse(hash_check(self.NEW, self.NEW, []), "no cysd.prev*.exe must fail")
        self.assertFalse(hash_check(self.NEW, self.NEW, [self.NEW]), "prev identical to the new image must fail")
        self.assertFalse(hash_check(self.NEW[:16] + "f" * 48, self.NEW, [self.OLD]), "a 16-hex prefix match is not enough: full hash compared")

    def test_workflow_compares_the_full_hash(self):
        text = WF.read_text(encoding="utf-8")
        self.assertIn("sha=$cysdSha", text); self.assertNotIn("$cysdSha.Substring", text); self.assertNotIn("$want.Substring", text)
        self.assertIn("if ($have -ne $want)", text); self.assertIn("-eq $cysdSha", text)


def migwait_problems(text):
    bad = []
    for need in ("MARKER_PHASE", "MIGRATION_FAILED", "TotalSeconds -lt 240", "'.cys\\.pending-restore'", "$nextShot = $t + 15", "Shot 'migration-end'", "marker_final.json",
                 "last marker phase=", "last screenshot=", "PROC ", "Timeline \"migration t="):
        if need not in text: bad.append("missing: " + need)
    i = [text.find(x) for x in ("MIGRATED new daemon", "# (3b) only now", "OLD_DAEMON_EXITED pid=")]
    if min(i) < 0 or not (i[0] < i[1] < i[2]): bad.append("order: new daemon, then seats, then old daemon exit")
    return bad


class MigrationWait(unittest.TestCase):
    TEXT = WF.read_text(encoding="utf-8")

    def test_migration_wait_records_marker_and_screens_and_precedes_seats(self):
        self.assertEqual(migwait_problems(self.TEXT), [])

    def test_negative_controls(self):
        t = self.TEXT
        for label, bad in (("no failure diagnostics", t.replace("MIGRATION_FAILED", "X")), ("no marker phases", t.replace("MARKER_PHASE", "X")),
                           ("short wait", t.replace("TotalSeconds -lt 240", "TotalSeconds -lt 20")), ("seats before migration", t.replace("MIGRATED new daemon", "X")),
                           ("no periodic screenshots", t.replace("$nextShot = $t + 15", "$nextShot = $t + 9999")), ("marker not kept", t.replace("marker_final.json", "x.json"))):
            with self.subTest(label=label):
                self.assertNotEqual(bad, t)
                self.assertTrue(migwait_problems(bad), label)


def logon_problems(text):
    bad = []
    doc = yaml.safe_load(text)
    steps = doc["jobs"]["direct-update-proof"]["steps"]
    s3 = [s for s in steps if s.get("name", "").startswith("(3) restart")]
    s4 = [s for s in steps if s.get("name", "").startswith("(4) LOGON_SIM")]
    if len(s3) != 1 or len(s4) != 1: return ["steps (3) and (4) must exist once"]
    r3, r4 = s3[0]["run"], s4[0]["run"]
    if s4[0].get("if") != "always()": bad.append("LOGON_SIM must be always()")
    if steps.index(s4[0]) != steps.index(s3[0]) + 1 and steps.index(s4[0]) < steps.index(s3[0]): bad.append("LOGON_SIM must follow (3)")
    if "if:" in str(s3[0].get("continue-on-error", "")) or s3[0].get("continue-on-error") or s4[0].get("continue-on-error"): bad.append("no continue-on-error: the job result must reflect (3)")
    for need in ("function ExpandDaemon", "SetCursorPos(150, 49)", "mouse_event(0x0002", "mouse_event(0x0004", "daemon-expanded", "UIAutomationClient", "DAEMON_INFO_UIA"):
        if need not in r3: bad.append("(3) missing: " + need)
    i = [r3.find(x) for x in ("ExpandDaemon 'daemon-expanded'", "if (-not $migrated) {")]
    if min(i) < 0 or not i[0] < i[1]: bad.append("(3) must expand the daemon details BEFORE the migration verdict (timeout and success)")
    for need in ("old-daemon.json", "$old.created", "Stop-Process -Id $tg.pid", "schtasks /Query /TN cysd", "schtasks /Run /TN cysd", "waved.exe", "-match '^0\\.3\\.2'", "wave-app.exe", "& $wave list",
                 "daemon-expanded-logon", "logon-sidebar", "LOGON_SIM_OK", "LOGON_SIM_FAIL", "exit 1", "forwarder_seen"):
        if need not in r4: bad.append("(4) missing: " + need)
    if re.search(r"Stop-Process\s+-Name|taskkill|Get-Process\s+(cysd|waved)[^\n]*Stop-Process", r4): bad.append("(4) may end only recorded pids (and wave-app by name)")
    return bad


class LogonSim(unittest.TestCase):
    TEXT = WF.read_text(encoding="utf-8")

    def test_logon_sim_and_daemon_expand(self):
        self.assertEqual(logon_problems(self.TEXT), [])

    def test_negative_controls(self):
        t = self.TEXT
        for label, bad in (("not always", t.replace('        if: always()\n        shell: pwsh\n        run: |\n          $ErrorActionPreference = \'Continue\'\n          $dir = Join-Path $env:LOCALAPPDATA \'Wave Terminal\'\n          New-Item', '        shell: pwsh\n        run: |\n          $ErrorActionPreference = \'Continue\'\n          $dir = Join-Path $env:LOCALAPPDATA \'Wave Terminal\'\n          New-Item', 1)),
                           ("no daemon expand on failure", t.replace("          ExpandDaemon 'daemon-expanded'", "          # x", 1)),
                           ("no click", t.replace("SetCursorPos(150, 49)", "SetCursorPos(0, 0)")),
                           ("no schtasks", t.replace("schtasks /Run /TN cysd", "echo skip")),
                           ("no verdict", t.replace("LOGON_SIM_FAIL", "X")),
                           ("kill by name", t.replace("Stop-Process -Id $tg.pid", "taskkill /F /IM waved.exe; Stop-Process -Id $tg.pid")),
                           ("no wave list", t.replace("& $wave list 2>&1", "echo")),
                           ("continue-on-error", t.replace('(3) restart like the updater (/R), 0.3.2 daemon, seats restored, old 0.3.1 daemon exits"\n', '(3) restart like the updater (/R), 0.3.2 daemon, seats restored, old 0.3.1 daemon exits"\n        continue-on-error: true\n', 1))):
            with self.subTest(label=label):
                self.assertNotEqual(bad, t)
                self.assertTrue(logon_problems(bad), label)


def failopen_problems(text):
    bad = []
    doc = yaml.safe_load(text)
    s3 = [s for s in doc["jobs"]["direct-update-proof"]["steps"] if s.get("name", "").startswith("(3) restart")]
    if len(s3) != 1: return ["step (3) missing"]
    r = s3[0]["run"]
    for need in ("MIGRATION_REFUSED_FAILOPEN_OK", "function UiaTexts", "기존 데몬으로 계속 실행", "확인하고 있습니다", "$waitingSeen", "$toastSeen", "$listOk", "$seatsOk", "upd-seat-a", "(dead GUI)",
                 "wave-app is not alive", "if ($reasons.Count -eq 0)", "throw \"MIGRATION_FAILED", "failopen-uia-texts.txt", "$script:uia"):
        if need not in r: bad.append("missing: " + need)
    if "if (-not $toastSeen) { $reasons +=" not in r: bad.append("a missing toast must fail")
    if "if ($waitingSeen) { $reasons +=" not in r: bad.append("the waiting state (dead GUI) must fail")
    if "if (-not $listOk) { $reasons +=" not in r or "if (-not $seatsOk) { $reasons +=" not in r: bad.append("wave list / seats must be required")
    i = [r.find(x) for x in ("ExpandDaemon 'daemon-expanded'", "MIGRATION_REFUSED_FAILOPEN_OK", "throw \"MIGRATION_FAILED")]
    if min(i) < 0 or not (i[0] < i[1] < i[2]): bad.append("order: expand, fail-open verdict, throw")
    if "MIGRATED new daemon" not in r or "OLD_DAEMON_EXITED pid=" not in r: bad.append("pass bar A must stay")
    return bad


class FailOpenBar(unittest.TestCase):
    TEXT = WF.read_text(encoding="utf-8")

    def test_bar_b(self):
        self.assertEqual(failopen_problems(self.TEXT), [])

    def test_negative_controls(self):
        t = self.TEXT
        for label, bad in (("toast not required", t.replace("if (-not $toastSeen) { $reasons +=", "if ($false) { $reasons +=")),
                           ("dead GUI (waiting) passes", t.replace("if ($waitingSeen) { $reasons +=", "if ($false) { $reasons +=")),
                           ("wave list not required", t.replace("if (-not $listOk) { $reasons +=", "if ($false) { $reasons +=")),
                           ("seats not required", t.replace("if (-not $seatsOk) { $reasons +=", "if ($false) { $reasons +=")),
                           ("alive not checked", t.replace("wave-app is not alive", "x")),
                           ("no ok marker", t.replace("MIGRATION_REFUSED_FAILOPEN_OK", "X")),
                           ("A removed", t.replace("OLD_DAEMON_EXITED pid=", "X pid=")),
                           ("always passes", t.replace("if ($reasons.Count -eq 0)", "if ($true)"))):
            with self.subTest(label=label):
                self.assertNotEqual(bad, t)
                self.assertTrue(failopen_problems(bad), label)


if __name__ == "__main__":
    unittest.main()
