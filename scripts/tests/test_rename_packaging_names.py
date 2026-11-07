"""0.3.2 rename packaging contract: canonical wave/waved/wave-app ship with the cys/cysd/cys-app forwarders.

Static file reads only (no build, no install, no process) — safe under the C9 host boundary.
Design: HB_rename_design_before_code.md rows 12-15 (canonical executables + legacy entrypoints).
"""
import json
import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
SIDECARS = ["wave", "waved", "cys", "cysd", "cys-app"]          # externalBin (root crate binaries)
WINDOWS_EXES = ["waved", "wave", "cysd", "cys", "wave-app", "cys-app"]  # every name the installer may find locked


def read(relative):
    return (ROOT / relative).read_text(encoding="utf-8")


class TauriBundleNames(unittest.TestCase):
    def test_external_bins_and_main_binary_name(self):
        config = json.loads(read("src-tauri/tauri.conf.json"))
        self.assertEqual(config.get("mainBinaryName"), "wave-app")
        self.assertEqual(sorted(config["bundle"]["externalBin"]), sorted(f"binaries/{n}" for n in SIDECARS))


class BundlePrepNames(unittest.TestCase):
    def setUp(self):
        self.script = read("scripts/bundle-prep.sh")

    def test_builds_every_sidecar_for_target_and_host(self):
        builds = [line for line in self.script.splitlines() if re.match(r"\s*cargo build --release", line)]
        self.assertGreaterEqual(len(builds), 2, "target and host build lines")
        for line in builds[:2]:
            for name in SIDECARS:
                self.assertRegex(line, rf"--bin {re.escape(name)}(\s|$)", f"{name} missing in: {line.strip()}")

    def test_copies_every_sidecar_with_target_triple(self):
        loop = re.search(r"for name in ([^;\n]+); do\n\s*cp \"\$bindir/\$name\$exe\" \"src-tauri/binaries/\$name-\$triple\$exe\"", self.script)
        self.assertIsNotNone(loop, "one copy loop places every sidecar as <name>-<triple>[.exe]")
        self.assertEqual(sorted(loop.group(1).split()), sorted(SIDECARS))

    def test_pack_manifest_uses_the_canonical_cli(self):
        self.assertIn('pack-manifest > src-tauri/resources/pack-manifest.json', self.script)
        self.assertRegex(self.script, r'manifest_cli="\$bindir/wave\$exe"')
        self.assertRegex(self.script, r'manifest_cli="target/release/wave\$host_exe"')
        self.assertNotIn("manifest_cys", self.script)


class NsisLockedFileSwap(unittest.TestCase):
    def setUp(self):
        self.hooks = read("src-tauri/nsis-hooks.nsh")

    def test_every_name_rename_swaps_through_the_prev_chain(self):
        for name in WINDOWS_EXES:
            for suffix in ("prev", "prev2", "prev3"):
                self.assertIn(f'Rename "$INSTDIR\\{name}.exe" "$INSTDIR\\{name}.{suffix}.exe"', self.hooks,
                              f"{name}.{suffix}")

    def test_gui_images_are_closed_before_install(self):
        preinstall = self.hooks.split("!macroend")[0]
        for name in ("wave-app", "cys-app"):
            self.assertIn(f"taskkill /F /T /IM {name}.exe", preinstall, name)

    def test_uninstall_stops_every_name_and_lame_duck(self):
        uninstall = self.hooks[self.hooks.index("NSIS_HOOK_PREUNINSTALL"):]
        for name in WINDOWS_EXES:
            for image in (f"{name}.exe", f"{name}.prev.exe", f"{name}.prev2.exe", f"{name}.prev3.exe"):
                self.assertIn(f"taskkill /F /T /IM {image}", uninstall, image)


class WixNames(unittest.TestCase):
    def test_both_msi_definitions_ship_canonical_and_legacy_cli(self):
        for path in ("dist-win/cys.wxs", "dist-win/cys-x64.wxs"):
            names = set(re.findall(r'<File Id="[^"]+" Name="([^"]+)"', read(path)))
            self.assertTrue({"wave.exe", "waved.exe", "cys.exe", "cysd.exe"} <= names, f"{path}: {sorted(names)}")


INSTALLED_EXES = ["wave-app.exe", "wave.exe", "waved.exe", "cys.exe", "cysd.exe", "cys-app.exe"]
MACOS_EXES = ["wave-app", "wave", "waved", "cys", "cysd", "cys-app"]


class WindowsBuildWorkflowNames(unittest.TestCase):
    def setUp(self):
        self.flow = read(".github/workflows/windows-build.yml")

    def test_install_anchor_is_the_canonical_daemon(self):
        self.assertNotIn("'Wave Terminal\\cysd.exe'", self.flow)
        self.assertIn("'Wave Terminal\\waved.exe'", self.flow)

    def test_listing_step_requires_canonical_and_legacy_names(self):
        step = self.flow[self.flow.index("Install dir listing"):]
        step = step[:step.index("      - name:", 1)]
        for name in INSTALLED_EXES:
            self.assertIn(f"'{name}'", step, name)

    def test_cleanup_also_stops_canonical_processes(self):
        self.assertNotRegex(self.flow, r"Get-Process cys,cysd\b")
        self.assertNotIn("/TN cysd", self.flow)


class ReleaseWorkflowNames(unittest.TestCase):
    def test_daemon_tests_and_harness_use_the_canonical_binaries(self):
        flow = read(".github/workflows/release.yml")
        self.assertNotIn("cargo test --bin cysd", flow)   # the cysd forwarder has no daemon tests
        self.assertIn("cargo test --bin waved", flow)
        self.assertIn('PHOENIX_HARNESS_CYSD="$PWD/target/debug/waved"', flow)


class MacosWorkflowNames(unittest.TestCase):
    def test_listing_step_requires_canonical_and_legacy_names(self):
        flow = read(".github/workflows/macos-runtime-build.yml")
        step = flow[flow.index("App bundle listing"):]
        step = step[:step.index("      - name:", 1)]
        self.assertIn('ls -la "$APP/Contents/MacOS"', step)
        loop = re.search(r"for name in ([^;\n]+); do", step)
        self.assertIsNotNone(loop)
        self.assertEqual(sorted(loop.group(1).split()), sorted(MACOS_EXES))


class CrtStrictNames(unittest.TestCase):
    def test_crt_gate_covers_canonical_and_legacy_binaries(self):
        line = next(l for l in read("scripts/verify_win_crt.py").splitlines() if l.startswith("STRICT_BINARIES"))
        for name in INSTALLED_EXES:
            self.assertIn(f'"{name}"', line, name)


class PhoenixWinSmokeNames(unittest.TestCase):
    """T5 drives the installed canonical daemon: 0.3.2 registers the scheduler task 'waved' (src/bin/cys.rs TASK)."""
    def setUp(self):
        self.smoke = read("cysjavis-pack/bin/javis_phoenix_win_smoke.py")

    def test_real_task_and_image_names_are_canonical(self):
        self.assertNotIn('task = "cysd"', self.smoke)
        self.assertIn('task = "waved"', self.smoke)
        self.assertNotIn('supervisor_status("cysd")', self.smoke)
        self.assertNotIn('"/IM", "cysd.exe"', self.smoke)
        self.assertIn('"/IM", "waved.exe"', self.smoke)

    def test_daemon_binary_resolves_to_the_canonical_sibling(self):
        body = self.smoke[self.smoke.index("def resolve_bins"):self.smoke.index("def run()")]
        self.assertIn('"waved.exe"', body)
        self.assertLess(body.index('"waved.exe"'), body.index('"cysd.exe"') if '"cysd.exe"' in body else len(body))


class UpdateVerifyTestFilter(unittest.TestCase):
    def test_filter_targets_the_binary_that_holds_the_test_and_guards_zero_matches(self):
        flow = read(".github/workflows/windows-update-verify.yml")
        m = re.search(r"cargo test --bin (\S+) statusline_params_full_schema", flow)
        self.assertIsNotNone(m)
        bins = dict(re.findall(r'\[\[bin\]\]\s*name = "([^"]+)"\s*path = "([^"]+)"', read("Cargo.toml")))
        self.assertIn("fn statusline_params_full_schema", read(bins[m.group(1)]), f"--bin {m.group(1)} has no such test")
        # cargo test exits 0 when a filter matches nothing: the step must require at least one passed test.
        after = flow[m.start():m.start() + 600]
        self.assertRegex(after, r"\[1-9\]\[0-9\]\* passed")


class DaemonStopFallbackNames(unittest.TestCase):
    """M3: the stop fallback (identify failed) must also reach the running canonical daemon; exact path tails only."""
    def test_pkill_fallback_covers_waved_and_cysd(self):
        src = read("src/bin/cys.rs")
        self.assertIn('for tail in ["MacOS/waved", "MacOS/cysd"] {', src)
        self.assertIn('.args(["-TERM", "-f", tail])', src)
        self.assertNotIn('.args(["-TERM", "-f", "MacOS/cysd"])', src)  # no single legacy-only call left


class PhoenixHarnessNames(unittest.TestCase):
    """M4: the release phoenix gate runs target/debug/waved; orphan detection must see it."""
    def setUp(self):
        self.harness = read("cysjavis-pack/bin/javis_phoenix_harness.py")

    def test_orphan_probe_uses_exact_canonical_and_legacy_names(self):
        self.assertNotIn('["pgrep", "-x", "cysd"]', self.harness)
        self.assertIn('for name in ("waved", "cysd")', self.harness[self.harness.index("pgrep") - 400:self.harness.index("pgrep") + 200])

    def test_default_binary_prefers_waved(self):
        body = self.harness[self.harness.index('override = os.environ.get("PHOENIX_HARNESS_CYSD")'):]
        body = body[:body.index("def shutil_which")]
        self.assertLess(body.index('"waved"'), body.index('"cysd"'))


class WinSingleOwnerGateNames(unittest.TestCase):
    """M6: G9 gate (windows-update-verify) checks the canonical daemon image and scheduler task."""
    def test_g9_uses_canonical_names(self):
        g9 = read("scripts/gates-v024/win-g9-single-owner.ps1")
        for legacy in ("'cysd.exe'", "Name='cysd.exe'", "/TN cysd ", "GetTask('cysd')", "\\cysd\\b"):
            self.assertNotIn(legacy, g9, legacy)
        for canonical in ("'waved.exe'", "Name='waved.exe'", "/TN waved ", "GetTask('waved')", "\\waved\\b", "'wave.exe'"):
            self.assertIn(canonical, g9, canonical)


class UpdateVerifyInstalledApp(unittest.TestCase):
    """Q2: the update proof reads the canonical GUI and still requires the legacy GUI forwarder."""
    def test_installed_app_is_wave_app_and_forwarder_exists(self):
        flow = read(".github/workflows/windows-update-verify.yml")
        self.assertIn("$installed = Join-Path $env:LOCALAPPDATA 'Wave Terminal\\wave-app.exe'", flow)
        forwarder = [l for l in flow.splitlines() if "'Wave Terminal\\cys-app.exe'" in l]
        self.assertTrue(forwarder and all("Test-Path" in l and "throw" in l for l in forwarder), forwarder)


if __name__ == "__main__":
    unittest.main()
