#!/usr/bin/env python3
"""Download official Debian 11 slim rootfs on the host and run Aurora inside it.
No apt/dpkg/package download is performed in the chroot; repositories/DNS are disabled.
"""
from __future__ import annotations
import argparse, hashlib, http.client, json, os, signal, subprocess, tarfile, time, urllib.parse, urllib.request
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]; IMAGE='library/debian'; TAG='11-slim'; REG='https://registry-1.docker.io'

def jget(url, token=None, accept=None):
    h={};
    if token:h['Authorization']='Bearer '+token
    if accept:h['Accept']=accept
    with urllib.request.urlopen(urllib.request.Request(url,headers=h),timeout=180) as r:return json.load(r)

def token():
    q=urllib.parse.urlencode({'service':'registry.docker.io','scope':f'repository:{IMAGE}:pull'})
    return jget('https://auth.docker.io/token?'+q)['token']

def manifest(tok):
    idx=jget(f'{REG}/v2/{IMAGE}/manifests/{TAG}',tok,'application/vnd.oci.image.index.v1+json, application/vnd.docker.distribution.manifest.list.v2+json')
    for m in idx['manifests']:
        p=m.get('platform',{})
        if p.get('os')=='linux' and p.get('architecture')=='amd64':
            return m['digest'],jget(f'{REG}/v2/{IMAGE}/manifests/{m["digest"]}',tok,'application/vnd.oci.image.manifest.v1+json, application/vnd.docker.distribution.manifest.v2+json')
    raise RuntimeError('no linux/amd64 Debian 11 manifest')

def download(tok,digest,path):
    expected=digest.split(':',1)[1]
    if path.exists() and hashlib.sha256(path.read_bytes()).hexdigest()==expected:return
    req=urllib.request.Request(f'{REG}/v2/{IMAGE}/blobs/{digest}',headers={'Authorization':'Bearer '+tok});h=hashlib.sha256();tmp=path.with_suffix('.part')
    with urllib.request.urlopen(req,timeout=240) as src,tmp.open('wb') as out:
        while b:=src.read(1024*1024):h.update(b);out.write(b)
    if h.hexdigest()!=expected:raise RuntimeError('Debian layer checksum mismatch')
    tmp.replace(path)

def run(args,**kw):
    kw.setdefault('timeout',60)
    return subprocess.run([str(x) for x in args],check=True,text=True,**kw)
def digest_tree(path):
    h=hashlib.sha256()
    if path.exists():
        for p in sorted(x for x in path.rglob('*') if x.is_file()):h.update(str(p.relative_to(path)).encode()+b'\0'+hashlib.sha256(p.read_bytes()).digest())
    return h.hexdigest()
def wait_health(port,p):
    for _ in range(160):
        if p.poll() is not None:raise RuntimeError(f'Aurora exited {p.returncode}')
        try:
            c=http.client.HTTPConnection('127.0.0.1',port,timeout=.5);c.request('GET','/healthz');r=c.getresponse();b=r.read();c.close()
            if r.status==200 and b==b'ok':return
        except Exception:pass
        time.sleep(.1)
    raise RuntimeError('health timeout')

def main():
    if os.geteuid() != 0:
        raise SystemExit("run this chroot QA as root, for example: sudo python3 tests/debian11_chroot_qa.py ...")
    ap=argparse.ArgumentParser();ap.add_argument('--archive',type=Path,required=True);ap.add_argument('--work',type=Path,default=ROOT/'.agentwork/debian11-chroot-qa');ap.add_argument('--port',type=int,default=11220);a=ap.parse_args()
    work=a.work.resolve();work.mkdir(parents=True,exist_ok=True);root=work/'rootfs';layer=work/'debian11-slim-amd64.tar.gz';tok=token();md,m=manifest(tok);layers=m['layers'];assert len(layers)==1;download(tok,layers[0]['digest'],layer)
    if root.exists():
        run(['sudo','rm','-rf',root])
    root.mkdir()
    run(['tar','-xzf',layer,'-C',root,'--no-same-owner'])
    osr=(root/'etc/os-release').read_text();assert 'VERSION_ID="11"' in osr or 'VERSION_ID=11' in osr,osr
    # Extract already-built release; no install/package-manager operation.
    target=root/'opt/aurora';target.mkdir(parents=True)
    with tarfile.open(a.archive.resolve()) as tf:
        top=tf.getmembers()[0].name.split('/')[0]
        for x in tf.getmembers():
            if x.name==top:continue
            assert x.name.startswith(top+'/');x.name=x.name[len(top)+1:];tf.extract(x,target,filter='data')
    with (root/'etc/passwd').open('a') as f:f.write('aurora:x:1000:1000:Aurora QA:/home/aurora:/bin/sh\n')
    with (root/'etc/group').open('a') as f:f.write('aurora:x:1000:\n')
    for d in ['home/aurora','state','tmp/.X11-unix']:(root/d).mkdir(parents=True,exist_ok=True)
    run(['sudo','chmod','1777',root/'tmp',root/'tmp/.X11-unix']);os.chmod(root/'state',0o700);os.chmod(root/'home/aurora',0o700);run(['sudo','chown','-R','1000:1000',root/'home/aurora',root/'state'])
    for name in ['null','zero','random','urandom']:
        dst=root/'dev'/name;dst.unlink(missing_ok=True);run(['sudo','cp','-a','/dev/'+name,dst])
    pw=root/'home/aurora/password';pw.write_text('2208\n');run(['sudo','chown','1000:1000',pw]);pw.chmod(0o600)
    # Fail closed against package downloads in the chroot.
    (root/'etc/resolv.conf').write_text('# DNS disabled for Aurora runtime QA\n')
    if (root/'etc/apt/sources.list').exists():(root/'etc/apt/sources.list').write_text('# apt disabled for Aurora runtime QA\n')
    for p in (root/'etc/apt/sources.list.d').glob('*') if (root/'etc/apt/sources.list.d').exists() else []:
        if p.is_file():p.write_text('# apt disabled for Aurora runtime QA\n')
    status=root/'var/lib/dpkg/status';before=(hashlib.sha256(status.read_bytes()).hexdigest(),digest_tree(root/'var/lib/apt/lists'),digest_tree(root/'var/cache/apt/archives'))
    log=work/'aurora.log';cmd=['sudo','chroot','--userspec=1000:1000',root,'/opt/aurora/run.sh','--port',str(a.port),'--headless','yes','--https','no','--localhost','yes','--audio','yes','--dbus','yes','--passwd-file','/home/aurora/password','--state-dir','/state']
    with log.open('wb') as f:p=subprocess.Popen([str(x) for x in cmd],stdin=subprocess.DEVNULL,stdout=f,stderr=f,start_new_session=True)
    try:
        wait_health(a.port,p);session=max((root/'state').glob('session-*'),key=lambda x:x.stat().st_mtime);rt=json.loads((session/'session.json').read_text());expected=f':{a.port%1000}';assert rt['DISPLAY']==expected and rt['private_x11'] is True,rt
        base=['sudo','chroot','--userspec=1000:1000',root,'/opt/aurora/vendor/x86_64/bin/busybox','env','-i','HOME=/home/aurora','PATH=/opt/aurora/vendor/x86_64/bin',f'DISPLAY={rt["DISPLAY"]}',f'XAUTHORITY={rt["XAUTHORITY"]}',f'PULSE_SERVER={rt["PULSE_SERVER"]}',f'PULSE_COOKIE={rt["PULSE_COOKIE"]}',f'PULSE_CLIENTCONFIG=/{session.relative_to(root).as_posix()}/client.conf']
        run(base+['/opt/aurora/vendor/x86_64/bin/ffmpeg','-v','error','-f','x11grab','-video_size','1280x720','-i',rt['DISPLAY'],'-frames:v','1','-f','null','-'])
        q=run(base+['/opt/aurora/vendor/x86_64/bin/pactl','info'],capture_output=True);assert 'Server String' in q.stdout
        geometry=run(base+['/opt/aurora/vendor/x86_64/bin/xdotool','getdisplaygeometry'],capture_output=True)
        assert geometry.stdout.strip()=='1280 720',geometry.stdout
        run(base+['/opt/aurora/vendor/x86_64/bin/xdotool','mousemove','200','200','click','--delay','0','1'])
        run(base+['/opt/aurora/vendor/x86_64/bin/xdotool','key','--delay','0','a'])
        busbase=base.copy()
        # Insert child environment values before the executable, never install a D-Bus package.
        busbase += [f'DBUS_SESSION_BUS_ADDRESS={rt["DBUS_SESSION_BUS_ADDRESS"]}',f'AURORA_DBUS_MACHINE_ID_FILE={rt["AURORA_DBUS_MACHINE_ID_FILE"]}']
        bus=run(busbase+['/opt/aurora/vendor/x86_64/bin/dbus-send','--session','--print-reply','--dest=org.freedesktop.DBus','/','org.freedesktop.DBus.Peer.GetMachineId'],capture_output=True)
        assert 'string' in bus.stdout,bus.stdout
        c=http.client.HTTPConnection('127.0.0.1',a.port,timeout=3);c.request('POST','/api/auth',json.dumps({'passwd':'2208'}),{'Content-Type':'application/json'});r=c.getresponse();r.read();assert r.status==200;c.close()
    finally:
        if p.poll() is None:
            os.killpg(p.pid,signal.SIGTERM)
            try:p.wait(timeout=8)
            except subprocess.TimeoutExpired:os.killpg(p.pid,signal.SIGKILL);p.wait(timeout=3)
    after=(hashlib.sha256(status.read_bytes()).hexdigest(),digest_tree(root/'var/lib/apt/lists'),digest_tree(root/'var/cache/apt/archives'));assert before==after,'Debian package database/cache changed'
    result={'image':'debian:11-slim','manifest_digest':md,'layer_digest':layers[0]['digest'],'os':'Debian GNU/Linux 11 (bullseye)','port':a.port,'display':f':{a.port%1000}','health':'ok','auth_2208':'ok','tinyx_capture':'ok','pulseaudio':'ok','xdotool_core_x11':'ok','dbus_machine_id':'ok','runtime_uid':1000,'archive_sha256':hashlib.sha256(a.archive.read_bytes()).hexdigest(),'kernel_note':'chroot uses the host Linux kernel; Debian 11 userspace tested','package_database_unchanged':True,'apt_cache_unchanged':True,'package_downloads_inside_chroot':0,'package_installs_inside_chroot':0,'dns_inside_chroot':'disabled during QA'}
    out=ROOT/'.output/debian11-chroot-qa.json';out.parent.mkdir(exist_ok=True);out.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2));print('log:',log)
if __name__=='__main__':main()
