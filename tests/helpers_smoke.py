#!/usr/bin/env python3
"""Authenticated TinyX and static PulseAudio smoke tests; no host services modified."""
from pathlib import Path
import json,os,struct,subprocess,tempfile,time
ROOT=Path(__file__).resolve().parents[1]
BIN=ROOT/'vendor/x86_64/bin'
EXE=ROOT/'.output/dist/current/aurora'
if not EXE.exists():EXE=ROOT/'.output/qa-release/aurora'
results={}
def run(argv, env, timeout=4):
    return subprocess.run([str(a) for a in argv],env=env,capture_output=True,text=True,timeout=timeout)
def finish(p):
    if p.poll() is None:p.terminate()
    try:p.wait(timeout=3)
    except subprocess.TimeoutExpired:p.kill();p.wait()
with tempfile.TemporaryDirectory(prefix='aurora-helpers-') as d:
    tmp=Path(d);env={**os.environ,'DISPLAY':':71','XAUTHORITY':str(tmp/'Xauthority')}
    auth=struct.pack('>H',65535)
    for b in (b'',b'71',b'MIT-MAGIC-COOKIE-1',os.urandom(16)):auth+=struct.pack('>H',len(b))+b
    (tmp/'Xauthority').write_bytes(auth);(tmp/'Xauthority').chmod(0o600)
    with (tmp/'tinyx.log').open('w') as log:
        p=subprocess.Popen([str(BIN/'Xtiny'),':71','-screen','1280x720x24','-fp','built-ins','-nolisten','tcp','-s','0','-noreset','-auth',str(tmp/'Xauthority')],env=env,stdout=log,stderr=log)
        try:
            time.sleep(.5);q=run([EXE,'--probe-display',':71'],env)
            results['tinyx_xtest']=q.returncode==0
            if q.returncode:print(q.stderr,(tmp/'tinyx.log').read_text())
            bad={**env,'XAUTHORITY':str(tmp/'missing-auth')};q=run([EXE,'--probe-display',':71'],bad)
            results['tinyx_rejects_missing_cookie']=q.returncode!=0
        finally:finish(p)
    (tmp/'pulse-cookie').write_bytes(os.urandom(256));(tmp/'pulse-cookie').chmod(0o600)
    (tmp/'client.conf').write_text('autospawn = no\nenable-shm = no\n')
    (tmp/'default.pa').write_text(f'load-module module-native-protocol-unix socket={tmp}/native auth-cookie={tmp}/pulse-cookie auth-anonymous=0\nload-module module-null-sink sink_name=aurora_output rate=48000 channels=2\nset-default-sink aurora_output\n')
    env={**os.environ,'PULSE_SERVER':f'unix:{tmp}/native','PULSE_COOKIE':str(tmp/'pulse-cookie'),'PULSE_RUNTIME_PATH':str(tmp),'PULSE_STATE_PATH':str(tmp),'PULSE_CLIENTCONFIG':str(tmp/'client.conf')}
    with (tmp/'pulse.log').open('w') as log:
        p=subprocess.Popen([str(BIN/'pulseaudio'),'-n','--daemonize=no','--exit-idle-time=-1','--use-pid-file=no','--realtime=no','--high-priority=no','--file',str(tmp/'default.pa')],env=env,stdout=log,stderr=log)
        try:
            time.sleep(.5);q=run([BIN/'pactl','info'],env)
            results['pulse_native_protocol']=q.returncode==0
            if q.returncode:print(q.stderr,(tmp/'pulse.log').read_text())
            q=run([BIN/'pactl','load-module','module-sine-source','source_name=aurora_test','frequency=440'],env)
            results['pulse_builtin_module_loading']=q.returncode==0
            q=run([BIN/'pactl','load-module','/tmp/not-allowlisted.so'],env)
            results['pulse_rejects_dynamic_modules']=q.returncode!=0
        finally:finish(p)
print(json.dumps(results,indent=2))
(ROOT/'.output/helpers-smoke.json').write_text(json.dumps(results,indent=2))
assert all(results.values()),'helper smoke failures'
