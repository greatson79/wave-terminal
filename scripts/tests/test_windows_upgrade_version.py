"""Keep upgrade smoke expectations tied to the artifact that was built."""
import unittest
from pathlib import Path
import yaml

ROOT = Path(__file__).resolve().parents[2]


class UpgradeVersion(unittest.TestCase):
    def test_build_artifact_carries_expected_version(self):
        jobs = yaml.safe_load((ROOT / '.github/workflows/windows-build.yml').read_text())['jobs']
        upload = next(s for s in jobs['build']['steps'] if s.get('name') == 'Upload NSIS installer artifact')
        self.assertIn('expected-version.txt', upload['with']['path'])
        producer = next(s for s in jobs['build']['steps'] if s.get('name') == 'Record expected build version')
        self.assertIn('src-tauri/tauri.conf.json', producer['run'])
        self.assertIn('expected-version.txt', producer['run'])

    def test_upgrade_checks_exact_version_without_skipping(self):
        job = yaml.safe_load((ROOT / '.github/workflows/windows-build.yml').read_text())['jobs']['upgrade-paths']
        run = next(s['run'] for s in job['steps'] if 'run' in s)
        self.assertIn("Get-Content -LiteralPath 'new/expected-version.txt'", run)
        self.assertIn('$installed[0].DisplayVersion -ne $expectedVersion', run)
        self.assertNotIn("'0\\.3\\.6'", run)
        self.assertNotIn('continue-on-error', job)
        self.assertNotIn('if', job)

    def test_latest_released_version_is_an_upgrade_base(self):
        job = yaml.safe_load((ROOT / '.github/workflows/windows-build.yml').read_text())['jobs']['upgrade-paths']
        self.assertEqual(job['strategy']['matrix']['base'], [f'0.3.{n}' for n in range(7)])


if __name__ == '__main__':
    unittest.main()
