"""C12 --fix revive must start the canonical `waved`, `cysd` (forwarder) only when waved is absent (0.3.2)."""
import importlib.util, os, unittest
from unittest import mock

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("javis_preflight", os.path.join(HERE, "..", "javis_preflight.py"))
pf = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pf)


def run_c12(found):
    pings = iter([1, 0])  # first ping down, then up after revive
    spawned = []

    def which(name):
        return found.get(name)

    def fake_run(cmd, **kw):
        return mock.Mock(returncode=next(pings))

    p = pf.Preflight(fix=True, skips=set())
    with mock.patch.object(pf.shutil, "which", which), \
         mock.patch.object(pf.subprocess, "run", fake_run), \
         mock.patch.object(pf.subprocess, "Popen", lambda cmd, **kw: spawned.append(cmd[0])), \
         mock.patch.object(pf.time, "sleep", lambda *_: None), \
         mock.patch("builtins.open", mock.mock_open()):
        p.c12_daemon()
    return spawned


class DaemonRevive(unittest.TestCase):
    def test_prefers_waved_over_forwarder(self):
        self.assertEqual(run_c12({"cys": "/b/cys", "waved": "/b/waved", "cysd": "/b/cysd"}), ["/b/waved"])

    def test_falls_back_to_forwarder_only_when_waved_absent(self):
        self.assertEqual(run_c12({"cys": "/b/cys", "cysd": "/b/cysd"}), ["/b/cysd"])


if __name__ == "__main__":
    unittest.main()
