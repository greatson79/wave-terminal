#!/usr/bin/env python3
"""P0 측정 전용. 앱/운영 홈/데몬 무변경; 격리 홈에 init-pack 결과와 원문 저장."""
import argparse, hashlib, json, os, pathlib, subprocess, sys
ap=argparse.ArgumentParser();ap.add_argument('--app',required=True);ap.add_argument('--out',required=True);a=ap.parse_args()
app=pathlib.Path(a.app).resolve();out=pathlib.Path(a.out).resolve();out.mkdir(parents=True,exist_ok=True)
home=out/'home';home.mkdir(exist_ok=True)
pack=home/'.cys/pack';cys=app/'Contents/MacOS/cys'
env={'HOME':str(home),'PATH':str(cys.parent)+':/usr/bin:/bin:/usr/sbin:/sbin','CYS_PACK_DIR':str(pack),'CYS_SOCKET':str(home/'absent.sock'),'CYS_NO_AUTOSTART':'1','XDG_CONFIG_HOME':str(home/'.config'),'XDG_DATA_HOME':str(home/'.local/share'),'XDG_STATE_HOME':str(home/'.local/state'),'LANG':'en_US.UTF-8','PYTHONDONTWRITEBYTECODE':'1'}
results={}
def run(name,cmd,timeout=180,extra=None):
 e=dict(env);e.update(extra or {})
 try:
  r=subprocess.run(cmd,cwd=home,env=e,input=b'{}\n',capture_output=True,timeout=timeout);rc=r.returncode;stdout=r.stdout;stderr=r.stderr
 except subprocess.TimeoutExpired as x:rc=124;stdout=x.stdout or b'';stderr=x.stderr or b''
 (out/(name+'.stdout')).write_bytes(stdout);(out/(name+'.stderr')).write_bytes(stderr)
 results[name]={'command':[str(x) for x in cmd],'exit':rc,'stdout_bytes':len(stdout),'stderr_bytes':len(stderr)}
 print(name,rc,flush=True);return stdout
run('host',['/bin/sh','-c','sw_vers; uname -m; xcode-select -p; /usr/bin/python3 --version; /usr/bin/file /usr/bin/python3'])
run('runtimes',['/bin/sh','-c','for t in python3 bash git node uv; do echo "TOOL=$t"; command -v "$t"; "$t" --version; done'])
resources=app/'Contents/Resources'
(out/'app-files.txt').write_text('\n'.join(str(p.relative_to(app)) for p in app.rglob('*')))
run('init-pack',[str(cys),'init-pack'])
if (pack/'bin/javis_preflight.py').is_file():
 run('preflight',['/usr/bin/python3',str(pack/'bin/javis_preflight.py'),'--json'])
 for role in ['master','worker','cso','reviewer']:
  data=run('hook-'+role,['/bin/sh',str(pack/'hooks/session-start.sh')],extra={'CYS_ROLE':role,'CYS_SURFACE_ID':'p0-measure'})
  d=pack/'directives'/({'master':'MASTER','worker':'WORKER','cso':'CSO','reviewer':'REVIEWER'}[role]+'_DIRECTIVE.md')
  body=d.read_bytes();results['hook-'+role].update(directive_bytes=len(body),directive_sha256=hashlib.sha256(body).hexdigest(),directive_exactly_contained=body in data)
 # 개별 단계 직접 측정: 데몬이 없는 격리 환경. 부트 성공 검증과 구별한다.
 run('step2-ping',[str(cys),'ping'],20)
 run('step3-claim',[str(cys),'claim-role','master'],20)
 run('step4-boot',[str(cys),'boot'],30)
 run('step5-check',['/usr/bin/python3',str(pack/'bin/javis_orchestra.py'),'check'],60)
 results['bootstrap-full']={'status':'NOT_RUN','reason':'bootstrap --fix는 설치·daemon spawn을 수행하므로 서버 없는 측정 범위에서 보류. 단계1은 --json 진단이며 --fix 동등 검증이 아님.'}
(out/'results.json').write_text(json.dumps(results,ensure_ascii=False,indent=2))
(out/'env.json').write_text(json.dumps(env,indent=2))
