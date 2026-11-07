"""0.3.2 rename: phoenix supervisor labels (M1). Canonical waved job first; legacy only as a status-probe fallback.
Pure: launchctl/schtasks probes are replaced by in-memory stubs (no process, no host access).
"""
import importlib.util
import os
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
PH = os.path.normpath(os.path.join(HERE, "..", "javis_phoenix.py"))


def load(windows):
    env_before = os.environ.pop("PHOENIX_SUPERVISOR_LABEL", None)
    try:
        spec = importlib.util.spec_from_file_location("javis_phoenix_rename_%s" % windows, PH)
        m = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(m)
    finally:
        if env_before is not None:
            os.environ["PHOENIX_SUPERVISOR_LABEL"] = env_before
    m.IS_WINDOWS = windows
    if windows:
        m.SUPERVISOR_LABEL = m.CANONICAL_TASK
        m.LEGACY_SUPERVISOR_LABEL = m.LEGACY_TASK
    return m


def stub(m, managed):
    """managed = set of labels/tasks that are registered."""
    def status(label, running_pid=None):
        return {"label": label, "state": "managed" if label in managed else "unmanaged"}
    m.launchd_status = status
    m._schtasks_status = status


class Labels(unittest.TestCase):
    def test_canonical_labels_are_the_defaults(self):
        m = load(False)
        self.assertEqual(m.LAUNCHD_LABEL, "com.waveainetworks.waved")
        self.assertEqual(m.LEGACY_LAUNCHD_LABEL, "com.cysjavis.cysd")
        self.assertEqual(m.SUPERVISOR_LABEL, "com.waveainetworks.waved")
        self.assertTrue(m._launchd_plist_path().endswith("com.waveainetworks.waved.plist"))
        self.assertEqual((m.CANONICAL_TASK, m.LEGACY_TASK), ("waved", "cysd"))


class StatusFallback(unittest.TestCase):
    def check(self, windows):
        m = load(windows)
        canonical, legacy = m.SUPERVISOR_LABEL, m.LEGACY_SUPERVISOR_LABEL
        stub(m, {canonical})
        self.assertEqual(m.supervisor_status(canonical)["label"], canonical)
        stub(m, {legacy})  # not migrated yet: report the legacy registration, labelled, so ensure adds no second job
        st = m.supervisor_status(canonical)
        self.assertEqual((st["label"], st["state"], st.get("legacy_fallback")), (legacy, "managed", True))
        self.assertEqual(st["canonical"]["state"], "unmanaged")
        stub(m, set())
        self.assertEqual(m.supervisor_status(canonical)["state"], "unmanaged")
        stub(m, {legacy})  # an explicit non-canonical label (smoke override) never falls back
        self.assertEqual(m.supervisor_status("phoenix-win-smoke-tmp")["state"], "unmanaged")

    def test_mac(self):
        self.check(False)

    def test_windows(self):
        self.check(True)


class RestartAimsCanonicalOnly(unittest.TestCase):
    def test_ensure_targets_the_canonical_label(self):
        m = load(False)
        seen = []
        m.launchd_ensure = lambda label, plist, running_pid=None: seen.append((label, plist)) or {"ensured": True}
        m.supervisor_ensure(m.SUPERVISOR_LABEL, m._launchd_plist_path())
        self.assertEqual(seen[0][0], "com.waveainetworks.waved")
        self.assertNotIn("cysjavis", seen[0][1])


if __name__ == "__main__":
    unittest.main()
