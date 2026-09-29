#!/usr/bin/env python3
"""Host reuse, native TLS, cleanup, and an empty-root runtime smoke test."""
import http.client,json,os,shutil,signal,ssl,subprocess,tarfile,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
PKG=ROOT/'.output/qa-release'
HOST=ROOT/'.output/qa-session'
PASSWORD=ROOT/'.output/qa-password'
ENV={**os.environ,**{k:v for k,v in json.loads((HOST/'session.json').read_text()).items() if isinstance(v,str)}}
OUT=ROOT/'.output/lifecycle-qa';OUT.mkdir(exist_ok=True)
results={}
def ready(p,port,tls=False):
    for _ in range(100):
        if p.poll() is not None:raise RuntimeError(f'launcher exited {p.returncode}')
        try:
            c=http.client.HTTPSConnection('127.0.0.1',port,timeout=.5,context=ssl._create_unverified_context()) if tls else http.client.HTTPConnection('127.0.0.1',port,timeout=.5)
            c.request('GET','/healthz');r=c.getresponse();data=r.read();c.close()
            if r.status==200 and data==b'ok':return
        except (OSError,http.client.HTTPException):pass
        time.sleep(.1)
    raise AssertionError('HTTP did not become healthy')
def stop(p):
    p.send_signal(signal.SIGTERM)
    try:p.wait(timeout=6)
    except subprocess.TimeoutExpired:p.kill();p.wait();raise
    assert p.returncode==0,p.returncode
def command(argv,env=ENV):return subprocess.check_output([str(x) for x in argv],env=env,text=True,stderr=subprocess.PIPE).strip()
initial_sink=command([PKG/'vendor/x86_64/bin/pactl','get-default-sink'])
for name,port,tls in [('host-reuse',19991,False),('native-tls',19993,True)]:
    state=OUT/name;state.mkdir(mode=0o700,exist_ok=True);state.chmod(0o700)
    logpath=OUT/(name+'.log')
    with logpath.open('w') as log:
        p=subprocess.Popen([str(PKG/'aurora'),'--passwd-file',str(PASSWORD),'--port',str(port),'--localhost','yes','--https','yes' if tls else 'no','--state-dir',str(state)],env=ENV,stdout=log,stderr=log)
        try:
            ready(p,port,tls)
            directory=max(state.glob('session-*'),key=lambda p:p.stat().st_mtime);runtime=json.loads((directory/'session.json').read_text())
            assert not runtime['private_x11']
            lognames=sorted(f.name for f in directory.glob('*.log'))
            assert lognames==['server.log'],lognames
            assert command([PKG/'vendor/x86_64/bin/pactl','get-default-sink'])==initial_sink
            results[name]={'healthy':True,'fallback_children':0,'host_display':runtime['DISPLAY'],'audio_default_unchanged':True,'native_tls':tls}
        finally:stop(p)
    assert command([PKG/'aurora','--probe-display',ENV['DISPLAY']])==''
    command([PKG/'vendor/x86_64/bin/pactl','info'])
    command([PKG/'vendor/x86_64/bin/dbus-send','--session','--print-reply','--dest=org.freedesktop.DBus','/','org.freedesktop.DBus.ListNames'])
    results[name]['host_services_survived_shutdown']=True
    print(name,results[name],flush=True)

# Empty-root test: no /bin/sh, shared libs, package manager, systemd or installed desktop.
clean=ROOT/'.output/clean-root';clean.mkdir(exist_ok=True)
for part in ['opt','etc','dev','dev/shm','tmp','home/dev','state']:(clean/part).mkdir(parents=True,exist_ok=True)
for part in ['tmp','dev/shm']:(clean/part).chmod(0o1777)
(clean/'state').chmod(0o700);(clean/'home/dev').chmod(0o700)
(clean/'etc/passwd').write_text('root:x:0:0:root:/root:/bin/false\ndev:x:1001:1001:dev:/home/dev:/opt/aurora/vendor/x86_64/bin/sh\n')
(clean/'etc/group').write_text('root:x:0:\ndev:x:1001:\n')
(clean/'etc/hostname').write_text('aurora-clean-root\n')
(clean/'password').write_bytes(PASSWORD.read_bytes());(clean/'password').chmod(0o600)
for device in ['null','zero','random','urandom']:
    subprocess.run(['sudo','cp','-a','/dev/'+device,str(clean/'dev'/device)],check=True)
# Avoid this shared kernel's already-used abstract X11 addresses without touching host locks.
for n in range(80):(clean/'tmp'/f'.X{n}-lock').write_text('QA namespace reservation\n')
if (clean/'opt/aurora').exists():shutil.rmtree(clean/'opt/aurora')
shutil.copytree(PKG,clean/'opt/aurora',symlinks=True)
(clean/'opt/aurora').chmod(0o755)
assert not (clean/'lib').exists() and not (clean/'usr/lib').exists() and not (clean/'bin/sh').exists()
with (OUT/'clean-root.log').open('w') as log:
    p=subprocess.Popen(['sudo','chroot','--userspec=1001:1001',str(clean),'/opt/aurora/vendor/x86_64/bin/busybox','env','-i','HOME=/home/dev','USER=dev','PATH=/opt/aurora/vendor/x86_64/bin','/opt/aurora/aurora','--passwd-file','/password','--port','19992','--localhost','yes','--https','no','--headless','yes','--audio','yes','--dbus','yes','--state-dir','/state'],stdout=log,stderr=log)
    try:
        ready(p,19992)
        directory=max((clean/'state').glob('session-*'),key=lambda p:p.stat().st_mtime);runtime=json.loads((directory/'session.json').read_text())
        assert runtime['private_x11']
        assert set(f.name for f in directory.glob('*.log'))=={'tinyx.log','pulseaudio.log','dbus.log','desktop.log','server.log'}
        cmd=['sudo','chroot','--userspec=1001:1001',str(clean),'/opt/aurora/vendor/x86_64/bin/busybox','env','-i',
            'DBUS_SESSION_BUS_ADDRESS='+runtime['DBUS_SESSION_BUS_ADDRESS'],'/opt/aurora/vendor/x86_64/bin/dbus-send',
            '--session','--print-reply','--dest=org.freedesktop.DBus','/','org.freedesktop.DBus.Peer.GetMachineId']
        machine_reply=subprocess.run(cmd,capture_output=True,text=True,timeout=3)
        assert machine_reply.returncode==0,machine_reply.stderr
        results['clean-root']={'healthy':True,'installed_runtime_packages':0,'shared_library_directories':0,'host_shell_absent':True,'dbus_machine_id_works':True,'fallbacks':['TinyX','PulseAudio','D-Bus','Aurora WM'],'display':runtime['DISPLAY']}
    finally:stop(p)
results['clean-root']['owned_services_stopped']=True
print('clean-root',results['clean-root'],flush=True)
(OUT/'results.json').write_text(json.dumps(results,indent=2))
