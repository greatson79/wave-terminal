#!/usr/bin/env python3
"""Isolated fake-daemon contracts for launch-agent reuse/old-daemon edges. No live daemon is contacted.
Run: python3 scripts/test-launch-skew.py target/debug/cys
"""
import json, os, socket, subprocess, sys, tempfile, threading, time
from pathlib import Path

READY = 'ABSOLUTE DIRECTIVE\nWelcome to Claude Code!\n? for shortcuts\n'
SIDS = {'master': 2, 'cso': 2, 'worker': 3}


def scenario(binary, name, argv, *, reuse=False, complete=None, token=True, wait='2', timeout=40):
    """complete: False=reused seat never completes, True=completes, None=daemon predates launch_complete."""
    with tempfile.TemporaryDirectory(prefix='skew-', dir='/tmp') as d:
        root = Path(d); home, pack = root / 'home', root / 'pack'
        home.mkdir(); (pack / 'directives').mkdir(parents=True)
        for r in ('MASTER', 'RSI_LEARNING', 'WORKER', 'CSO'):
            (pack / 'directives' / f'{r}_DIRECTIVE.md').write_text('ABSOLUTE DIRECTIVE fixture')
        (pack / 'agents.json').write_text(json.dumps({'claude': {'cmd': '/bin/sh', 'ready_marker': '? for shortcuts', 'inject_delay_secs': 1}}))
        env = {k: v for k, v in os.environ.items() if not k.startswith(('CYS_', 'JAVIS_', 'AITERM_', 'CLAUDE_'))}
        env.update(HOME=str(home), CYS_PACK_DIR=str(pack), CYS_SOCKET=str(root / 'rpc.sock'), CYS_NO_AUTOSTART='1',
                   CYS_REUSED_LAUNCH_WAIT_SECS=wait)
        srv = socket.socket(socket.AF_UNIX); srv.bind(env['CYS_SOCKET']); srv.listen(); srv.settimeout(.1)
        calls, errors, roles, stop = [], [], {}, threading.Event()

        def surfaces():
            rows = []
            for role, sid in roles.items():
                row = {'surface_id': sid, 'role': role, 'exited': False}
                if complete is not None: row['launch_complete'] = complete
                rows.append(row)
            return rows

        def serve():
            try:
                while not stop.is_set():
                    try: c, _ = srv.accept()
                    except socket.timeout: continue
                    with c:
                        req = json.loads(c.makefile('rb').readline()); m, p = req['method'], req['params']; calls.append(m)
                        res = {}
                        if m == 'surface.create':
                            roles[p['role']] = SIDS[p['role']]
                            res = {'surface_id': SIDS[p['role']]}
                            if reuse: res['idempotent_reuse'] = True
                            elif token: res['launch_token'] = 'fixture-token'
                        elif m == 'surface.read_text': res = {'text': READY}
                        elif m == 'surface.list': res = {'surfaces': surfaces()}
                        elif m == 'system.topology':
                            res = {'live': [], 'tombstones': [], 'saved': [{'role': 'cso', 'agent': 'claude', 'cwd': str(home)}]}
                        c.sendall((json.dumps({'id': req['id'], 'ok': True, 'result': res}) + '\n').encode())
            except BaseException as e: errors.append(repr(e))

        t = threading.Thread(target=serve); t.start(); t0 = time.monotonic()
        try:
            p = subprocess.run([binary, *argv, '--cwd', str(home)] if argv[0] == 'launch-agent' else [binary, *argv],
                               env=env, capture_output=True, text=True, timeout=timeout)
        finally:
            stop.set(); t.join(2); srv.close()
        assert not errors, errors
        return p, calls, time.monotonic() - t0


def no_mutation(calls): return not any(m.startswith('surface.send_') or m in ('surface.set_meta', 'surface.close') for m in calls)


def main(binary):
    # Y-1/Y-2: reused seat that never completes -> exit 3 (pending, not failure), seat untouched, manual-continue hint
    p, calls, secs = scenario(binary, 'stuck', ['launch-agent', '--role', 'master', '--agent', 'claude'], reuse=True, complete=False)
    assert p.returncode == 3, (p.returncode, p.stderr)
    assert '기존 좌석' in p.stderr and 'surface:2' in p.stderr, p.stderr
    assert no_mutation(calls), calls
    assert secs < 15, secs
    print('PASS stuck reuse: exit 3, seat untouched, manual-continue hint')
    # Y-1: boot / restore must not count pending reuse as failure
    p, calls, _ = scenario(binary, 'boot', ['boot'], reuse=True, complete=False, wait='1')
    assert p.returncode == 0 and '실패 0' in p.stdout, (p.returncode, p.stdout, p.stderr)
    print('PASS boot does not count pending reuse as failure')
    p, calls, _ = scenario(binary, 'restore', ['restore'], reuse=True, complete=False, wait='1')
    assert p.returncode == 0 and '실패 0' in p.stdout, (p.returncode, p.stdout, p.stderr)
    print('PASS restore does not count pending reuse as failure')
    # Y-3: new cys + old cysd (no launch_token): keep the seat, skip the completion signal
    p, calls, _ = scenario(binary, 'old-daemon', ['launch-agent', '--role', 'master', '--agent', 'claude'], token=False)
    assert p.returncode == 0, (p.returncode, p.stderr)
    assert 'surface.close' not in calls and 'surface.launch_complete' not in calls, calls
    print('PASS old daemon without launch_token: seat kept, completion signal skipped')
    # Y-3 sibling: old daemon reuse reply has no launch_complete field -> cannot wait on it
    p, calls, secs = scenario(binary, 'old-reuse', ['launch-agent', '--role', 'master', '--agent', 'claude'], reuse=True, complete=None)
    assert p.returncode == 0 and p.stdout.strip() == 'surface:2' and no_mutation(calls), (p.returncode, p.stdout, p.stderr, calls)
    print('PASS old daemon reuse without launch_complete: returns the seat')


if __name__ == '__main__':
    main(str(Path(sys.argv[1]).resolve()))
