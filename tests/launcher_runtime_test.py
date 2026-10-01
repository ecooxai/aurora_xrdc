#!/usr/bin/env python3
"""Owned-process launcher matrix; checks real TinyX servers, auth, capture and teardown."""
import argparse, http.client, json, os, socket, subprocess, tempfile, time
from pathlib import Path

ROOT=Path(__file__).resolve().parents[1]

def request(port,method='GET',path='/healthz',body=None):
    conn=http.client.HTTPConnection('127.0.0.1',port,timeout=1)
    try:
        headers={'Content-Type':'application/json'}
        conn.request(method,path,json.dumps(body) if body is not None else None,headers)
        r=conn.getresponse();return r.status,r.read()
    finally:conn.close()

class Session:
    def __init__(self,package,port,display,args=None,env=None,dev=False,password='2208'):
        self.package=package;self.port=port;self.display=display;self.password=password
        self.tmp=tempfile.TemporaryDirectory(prefix='aurora-launch-qa-');self.root=Path(self.tmp.name)
        self.env={k:v for k,v in os.environ.items() if not k.startswith(('AURORA_','VIBE_RDESK_','PULSE_','DBUS_','XDG_RUNTIME')) and k not in ['DISPLAY','XAUTHORITY']}
        self.env.update(env or {});self.root.chmod(0o700)
        self.proc=None;self.log=None
        # Refuse to affect an unrelated listener.
        with socket.socket() as s:s.bind(('127.0.0.1',port))
        self.command=[str(ROOT/'dev.sh' if dev else package/'run.sh')]
        if dev:self.env.update(AURORA_DEV_SKIP_BUILD='1',AURORA_DEV_PORT=str(port),AURORA_LAUNCHER_BIN=str(package/'aurora'))
        else:self.command+=['--port',str(port)]
        self.command+=['--https','no','--localhost','yes','--headless','yes','--audio','yes','--dbus','yes','--state-dir',str(self.root),'--passwd',password]
        self.command+=args or []
    def start(self):
        self.log=(self.root/'launcher.log').open('wb')
        self.proc=subprocess.Popen(self.command,env=self.env,stdin=subprocess.DEVNULL,stdout=self.log,stderr=self.log,start_new_session=True)
        deadline=time.monotonic()+20
        while time.monotonic()<deadline:
            if self.proc.poll() is not None:raise AssertionError((self.root/'launcher.log').read_text())
            try:
                if request(self.port)==(200,b'ok'):break
            except (OSError,http.client.HTTPException):pass
            time.sleep(.05)
        else:raise AssertionError('health timeout')
        self.session=max(self.root.glob('session-*'),key=lambda p:p.stat().st_mtime)
        self.runtime=json.loads((self.session/'session.json').read_text())
        assert self.runtime['DISPLAY']==self.display,self.runtime
        assert request(self.port,'POST','/api/auth',{'passwd':self.password})[0]==200
        self.pids=[]
        for p in Path('/proc').iterdir():
            if p.name.isdecimal():
                try:
                    stat=(p/'stat').read_text().rsplit(')',1)[1].split()
                    if int(stat[1])==self.proc.pid:self.pids.append(int(p.name))
                except (OSError,ValueError):pass
        return self
    def program(self,*args):
        env={**self.env,**{k:v for k,v in self.runtime.items() if isinstance(v,str)}}
        env['PULSE_CLIENTCONFIG']=str(self.session/'client.conf')
        result=subprocess.run([str(self.package/'vendor/x86_64/bin'/args[0]),*map(str,args[1:])],env=env,capture_output=True,timeout=20)
        if result.returncode: raise AssertionError(f'{args[0]} exited {result.returncode}: {result.stderr.decode(errors="replace")}')
        return result
    def close(self):
        if self.proc is not None:
            if self.proc.poll() is None:self.proc.terminate()
            try:self.proc.wait(timeout=8)
            except subprocess.TimeoutExpired:self.proc.kill();self.proc.wait();raise
            if self.proc.returncode!=0:raise AssertionError((self.root/'launcher.log').read_text())
            for pid in getattr(self,'pids',[]):
                assert not Path(f'/proc/{pid}').exists(),f'owned child {pid} survived shutdown'
        if self.log:self.log.close()
        self.tmp.cleanup()


def main():
    ap=argparse.ArgumentParser();ap.add_argument('--package',type=Path,required=True);ap.add_argument('--output',type=Path,required=True);a=ap.parse_args()
    pkg=a.package.resolve();out=a.output.resolve();out.mkdir(parents=True,exist_ok=True)
    results={};main_session=Session(pkg,11220,':220')
    try:
        main_session.start()
        assert main_session.runtime['private_x11']
        main_session.program('xdotool','getdisplaygeometry')
        main_session.program('xdotool','mousemove','245','230','click','--delay','0','1')
        main_session.program('xdotool','key','--delay','0','a')
        shot=out/'desktop-11220.png'
        main_session.program('ffmpeg','-y','-v','error','-f','x11grab','-video_size','1280x720','-i',':220','-frames:v','1',shot)
        video=out/'desktop-11220.h264'
        main_session.program('ffmpeg','-y','-v','error','-f','x11grab','-video_size','1280x720','-framerate','10','-i',':220','-frames:v','5','-c:v','libx264','-preset','ultrafast','-pix_fmt','yuv420p',video)
        main_session.program('ffmpeg','-v','error','-i',video,'-f','null','-')
        results['run_sh_port11220']={'display':':220','healthy':True,'auth':True,'x11_input':True,'video_encoded_decoded':True,'screenshot_bytes':shot.stat().st_size}
        original=main_session.program('pactl','get-default-sink').stdout
        # A colliding display must fail; it must neither switch displays nor touch the first service.
        collision=Session(pkg,12220,':220')
        try:
            p=subprocess.run(collision.command,env=collision.env,capture_output=True,text=True,timeout=8)
            assert p.returncode!=0 and 'already in use' in p.stderr,(p.returncode,p.stderr)
            assert request(11220)==(200,b'ok')
            results['same_suffix_collision']={'rejected':True,'original_unaffected':True}
        finally:collision.tmp.cleanup()
        override=Session(pkg,12220,':321',args=['--display',':321.0'])
        try:
            override.start();results['explicit_override']={'port':12220,'display':':321','healthy':True}
        finally:override.close()
        reuse=Session(pkg,11444,':220',args=['--headless','auto','--audio','auto','--dbus','auto'],env={k:v for k,v in main_session.runtime.items() if isinstance(v,str)})
        try:
            reuse.start()
            assert not reuse.runtime['private_x11']
            assert sorted(p.name for p in reuse.session.glob('*.log'))==['server.log']
            results['host_reuse']={'private_x11':False,'no_fallback_services':True}
        finally:reuse.close()
        assert main_session.program('pactl','get-default-sink').stdout==original
        assert request(11220)==(200,b'ok')
        results['host_reuse']['host_alive_after_reuse_shutdown']=True
    finally:main_session.close()
    for name,port,expected,kwargs in [
        ('decimal_leading_zeros',11222,':222',{'args':['--port=011222']}),
        ('option_looking_password',11224,':224',{'password':'--port'}),
        ('dev_sh',11221,':221',{'dev':True}),
        ('auto_headless_fallback',11223,':223',{'args':['--headless','auto'],'env':{'DISPLAY':':65000'}}),
    ]:
        s=Session(pkg,port,expected,**kwargs)
        try:s.start();results[name]={'port':port,'display':expected,'healthy':True,'auth':True}
        finally:s.close()
    results['cleanup']={'all_owned_children_reaped':True}
    (out/'runtime-results.json').write_text(json.dumps(results,indent=2)+'\n')
    print(json.dumps(results,indent=2))
if __name__=='__main__':main()
