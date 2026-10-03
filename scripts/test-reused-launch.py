#!/usr/bin/env python3
"""Isolated RPC contract for idempotent launch reuse. No live daemon is contacted."""
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import threading


def run(binary, role='master', actual_role=None):
    actual_role = actual_role or role
    with tempfile.TemporaryDirectory(prefix='reuse-launch-', dir='/tmp') as d:
        root = Path(d)
        (root/'home').mkdir()
        (root/'pack'/'directives').mkdir(parents=True)
        for name in ('MASTER_DIRECTIVE.md', 'RSI_LEARNING_DIRECTIVE.md'):
            (root/'pack'/'directives'/name).write_text('ABSOLUTE DIRECTIVE fixture')
        (root/'pack'/'agents.json').write_text(json.dumps({'claude': {'cmd':'claude', 'ready_marker':'? for shortcuts'}}))
        env = {k:v for k,v in os.environ.items() if not k.startswith(('CYS_','JAVIS_','AITERM_','CLAUDE_'))}
        env.update(HOME=str(root/'home'), CYS_PACK_DIR=str(root/'pack'), CYS_SOCKET=str(root/'rpc.sock'), CYS_NO_AUTOSTART='1')
        server = socket.socket(socket.AF_UNIX)
        server.bind(env['CYS_SOCKET'])
        server.listen()
        server.settimeout(.1)
        calls = []
        state = {'polls':0, 'complete':False, 'error':None, 'stop':False}
        def serve():
            try:
                while not state['stop']:
                    try:
                        connection,_ = server.accept()
                    except socket.timeout:
                        continue
                    with connection:
                        req=json.loads(connection.makefile('rb').readline())
                        method=req['method']
                        calls.append(method)
                        if method=='surface.create':
                            result={'surface_id':2,'idempotent_reuse':True}
                        elif method=='surface.read_text':
                            result={'text':'claude: command not found'}
                        elif method=='surface.list':
                            state['polls']+=1
                            if state['polls']>=3: state['complete']=True
                            result={'surfaces':[{'surface_id':2,'role':actual_role,'exited':False,'launch_complete':state['complete']}]}
                        else:
                            result={}
                        connection.sendall((json.dumps({'id':req['id'],'ok':True,'result':result})+'\n').encode())
            except BaseException as exc: state['error']=repr(exc)
        thread=threading.Thread(target=serve)
        thread.start()
        try:
            p=subprocess.run([binary,'launch-agent','--role',role,'--agent','claude','--cwd',str(root/'home')],env=env,capture_output=True,text=True,timeout=15)
        finally:
            state['stop']=True
            thread.join(2)
            server.close()
        assert state['error'] is None, state
        assert p.returncode==0, (p.returncode,p.stderr,calls)
        assert p.stdout.strip()=='surface:2',p.stdout
        assert state['polls']>=3,state
        assert calls.count('surface.create')==1,calls
        assert not any(m.startswith('surface.send_') or m in ('surface.set_meta','surface.close') for m in calls),calls
        print(f'PASS {role}->{actual_role} reuse waits for prior launch_complete without sending, setting meta, or closing')

if __name__=='__main__':
    binary=str(Path(sys.argv[1]).resolve())
    run(binary)
    run(binary, 'worker', 'worker-2')
