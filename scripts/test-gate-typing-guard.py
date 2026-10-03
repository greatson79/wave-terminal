#!/usr/bin/env python3
"""Unix transport fixture exercising the real launch-agent CLI, never a live daemon.
Run: python3 scripts/test-gate-typing-guard.py target/debug/cys
RPC replies model a human accepting the first-run gate, then a typing guard.
"""
import concurrent.futures
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import threading
import time

GATE = 'WARNING: Claude Code running in Bypass Permissions mode\n❯ 1. No, exit\n2. Yes, I accept\nEnter to confirm · Esc to cancel\n'
READY = 'Welcome to Claude Code!\n❯\n? for shortcuts\n'


def scenario(binary, name, *, gate=True, key=False, permanent=False, error='typing_guard', command_error=False):
    with tempfile.TemporaryDirectory(prefix='gate-rpc-', dir='/tmp') as directory:
        root = Path(directory)
        home, pack = root / 'home', root / 'pack'
        home.mkdir()
        (pack / 'directives').mkdir(parents=True)
        for role in ['MASTER', 'RSI_LEARNING']:
            (pack / 'directives' / f'{role}_DIRECTIVE.md').write_text('ABSOLUTE DIRECTIVE fixture')
        (pack / 'agents.json').write_text(json.dumps({'claude': {
            'cmd': 'claude', 'ready_marker': '? for shortcuts', 'inject_delay_secs': 1,
        }}))
        env = {k: v for k, v in os.environ.items()
               if not k.startswith(('CYS_', 'JAVIS_', 'AITERM_', 'CLAUDE_'))}
        env.update(HOME=str(home), CYS_PACK_DIR=str(pack), CYS_SOCKET=str(root / 'rpc.sock'),
                   CYS_NO_AUTOSTART='1', XDG_CONFIG_HOME=str(home / '.config'),
                   XDG_STATE_HOME=str(home / '.local/state'))
        state = dict(alive=False, role=None, reads=0, guards=0, pastes=0, submits=0)
        calls, failures = [], []
        stopped = threading.Event()
        listener = socket.socket(socket.AF_UNIX)
        listener.bind(env['CYS_SOCKET'])
        listener.listen()
        listener.settimeout(.1)

        def serve():
            try:
                while not stopped.is_set():
                    try:
                        connection, _ = listener.accept()
                    except socket.timeout:
                        continue
                    with connection:
                        request = json.loads(connection.makefile('rb').readline())
                        method, params = request['method'], request['params']
                        calls.append((method, params))
                        result, refusal = {}, None
                        if method == 'surface.create':
                            state.update(alive=True, role=params['role'])
                            result = {'surface_id': 2}
                        elif method == 'surface.read_text':
                            state['reads'] += 1
                            result = {'text': ('claude: command not found' if command_error else
                                      GATE if gate and state['reads'] == 1 else
                                      'ABSOLUTE DIRECTIVE\n' + READY)}
                        elif method == 'surface.close':
                            state.update(alive=False, role=None)
                        elif method in ('surface.send_text', 'surface.send_key') and state['reads']:
                            # Any program input before the gate has disappeared is forbidden.
                            assert not (gate and state['reads'] == 1), calls
                            is_paste = method == 'surface.send_text'
                            target = not is_paste if key else is_paste
                            if target and (permanent or state['guards'] < 2):
                                state['guards'] += 1
                                refusal = {'code': error, 'message': 'human is typing in this pane'}
                            elif is_paste:
                                state['pastes'] += 1
                            else:
                                state['submits'] += 1
                        response = {'id': request['id'], 'ok': refusal is None}
                        response['error' if refusal else 'result'] = refusal or result
                        connection.sendall((json.dumps(response) + '\n').encode())
            except BaseException as exc:
                failures.append(repr(exc))

        thread = threading.Thread(target=serve)
        thread.start()
        started = time.monotonic()
        try:
            proc = subprocess.run([binary, 'launch-agent', '--role', 'master', '--agent', 'claude',
                                   '--cwd', str(home)], env=env, capture_output=True, text=True, timeout=65)
        finally:
            stopped.set()
            thread.join(timeout=2)
            listener.close()
        assert not failures, failures
        preserve = gate and error == 'typing_guard' and not command_error
        expected_code = 2 if preserve and permanent else 0 if preserve else 1
        expected = dict(alive=preserve, role='master' if preserve else None)
        details = f'{name}: exit={proc.returncode} state={state}\n{proc.stderr}'
        assert proc.returncode == expected_code, details
        assert all(state[k] == v for k, v in expected.items()), details
        if preserve:
            assert not any(m == 'surface.close' for m, _ in calls), details
            assert state['guards'] >= 2, details
            if permanent:
                assert state['submits'] == 0, details
                assert 'retained' in proc.stderr and 'incomplete' in proc.stderr, details
                assert 30 <= time.monotonic() - started < 60, details
            else:
                assert state['pastes'] == state['submits'] == 1, details
            # The fix must not acquire an authoritative bypass after the human gate.
            post_gate = False
            for method, params in calls:
                if method == 'surface.read_text':
                    post_gate = True
                elif post_gate and method.startswith('surface.send_'):
                    assert not params.get('authoritative'), (name, method, params)
        print(f'PASS {name}: exit={proc.returncode}, {state}', flush=True)


def main():
    binary = str(Path(sys.argv[1]).resolve())
    cases = [
        ('gate_text_retry', {}),
        ('gate_return_retry_no_duplicate_paste', {'key': True}),
        ('gate_text_exhaustion_keeps_role', {'permanent': True}),
        ('gate_return_exhaustion_keeps_role', {'key': True, 'permanent': True}),
        ('no_gate_guard_still_rolls_back', {'gate': False}),
        ('gate_other_error_still_rolls_back', {'error': 'send_denied'}),
        ('command_error_still_rolls_back', {'command_error': True}),
    ]
    failed = 0
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        futures = [pool.submit(scenario, binary, name, **options) for name, options in cases]
        for future in futures:
            try:
                future.result()
            except Exception as exc:
                print(f'FAIL {exc}', flush=True)
                failed += 1
    print(f'{len(cases) - failed} passed; {failed} failed')
    return int(bool(failed))


if __name__ == '__main__':
    raise SystemExit(main())
