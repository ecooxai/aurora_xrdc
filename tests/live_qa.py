#!/usr/bin/env python3
"""Live QA of an owned Aurora test instance, with authenticated X11 input observation."""
import argparse,asyncio,array,hashlib,http.client,json,math,os,statistics,subprocess,threading,time
from pathlib import Path
from urllib.parse import urlencode
import websockets
from Xlib import X,display as xdisplay

ROOT=Path(__file__).resolve().parents[1]
P=argparse.ArgumentParser();P.add_argument('--port',type=int,default=19990);P.add_argument('--session',type=Path,required=True);P.add_argument('--password-file',type=Path,required=True)
args=P.parse_args();password=args.password_file.read_text().strip();session=json.loads((args.session/'session.json').read_text())
env={**os.environ,**{k:v for k,v in session.items() if isinstance(v,str)}}
bin=ROOT/'vendor/x86_64/bin';out=ROOT/'.output/live-qa';out.mkdir(exist_ok=True)
results={};errors={}
def test(name,fn):
    try:result=fn();results[name]=result;print(name,json.dumps(result),flush=True)
    except Exception as e:errors[name]=str(e);print(name,'FAILED',repr(e),flush=True)
def request(method,path,body=None,cookie=None):
    c=http.client.HTTPConnection('127.0.0.1',args.port,timeout=20)
    h={'Content-Type':'application/json'}
    if cookie:h['Cookie']=cookie
    c.request(method,path,json.dumps(body) if body is not None else None,h)
    r=c.getresponse();data=r.read();headers=dict(r.getheaders());status=r.status;c.close();return status,data,headers
status,data,headers=request('POST','/api/auth',{'passwd':password});assert status==200,(status,data)
cookie=headers['set-cookie'].split(';')[0]
def api():
    assert request('GET','/healthz')[0]==200
    assert request('GET','/api/auth')[0]==401
    assert request('GET','/api/auth',cookie=cookie)[0]==200
    assert request('GET','/wheel_queue.mjs')[0]==200
    s,b,_=request('GET','/api/codecs',cookie=cookie);assert s==200,(s,b)
    return {'health':True,'authentication_required':True,'authenticated_session':True,'wheel_module':True,'codecs':json.loads(b)}
test('http_auth',api)
def command(argv,timeout=30,check=True):
    p=subprocess.run([str(x) for x in argv],env=env,capture_output=True,timeout=timeout)
    if check and p.returncode:raise AssertionError(p.stderr.decode(errors='replace')[-4000:])
    return p

def capture_video():
    res={}
    for encoder,ext in [('libx264','h264'),('libx265','hevc'),('libvpx','ivf'),('libvpx-vp9','ivf')]:
        path=out/f'video-{encoder}.{ext}'
        extra=['-preset','ultrafast'] if encoder in ('libx264','libx265') else ['-deadline','realtime','-cpu-used','8']
        command([bin/'ffmpeg','-y','-loglevel','error','-f','x11grab','-video_size','1280x720','-framerate','10','-i',session['DISPLAY'],'-frames:v','5','-an','-c:v',encoder,*extra,'-pix_fmt','yuv420p',path])
        q=command([bin/'ffprobe','-v','error','-show_entries','stream=codec_name,width,height','-of','json',path])
        info=json.loads(q.stdout)['streams'][0];assert info['width']==1280 and info['height']==720
        command([bin/'ffmpeg','-v','error','-i',path,'-f','null','-'])
        res[encoder]={**info,'bytes':path.stat().st_size,'decoded':True}
    command([bin/'ffmpeg','-y','-loglevel','error','-f','x11grab','-video_size','1280x720','-i',session['DISPLAY'],'-frames:v','1',out/'desktop.ppm'])
    return res
test('x11_video_codecs',capture_video)

def capture_audio():
    samples=array.array('h')
    for i in range(48000*3):
        v=int(math.sin(i*2*math.pi*440/48000)*12000);samples.extend([v,v])
    pcm=out/'tone.s16';pcm.write_bytes(samples.tobytes());res={}
    monitor=command([bin/'pactl','get-default-sink']).stdout.decode().strip()+'.monitor'
    for codec,ext in [('aac','aac'),('libopus','opus')]:
        path=out/f'audio.{ext}';err=(out/f'{codec}-capture.log').open('wb')
        cap=subprocess.Popen([str(bin/'ffmpeg'),'-y','-v','error','-f','pulse','-sample_rate','48000','-channels','2','-i',monitor,'-t','2','-c:a',codec,'-b:a','128k',str(path)],env=env,stdout=subprocess.DEVNULL,stderr=err)
        time.sleep(.3)
        play=subprocess.Popen([str(bin/'pacat'),'--playback','--raw','--format=s16le','--rate=48000','--channels=2'],env=env,stdin=pcm.open('rb'),stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
        try:
            cap.wait(timeout=10);play.wait(timeout=10)
        finally:
            for p in (cap,play):
                if p.poll() is None:p.kill();p.wait()
            err.close()
        assert cap.returncode==0,(out/f'{codec}-capture.log').read_text()
        raw=command([bin/'ffmpeg','-v','error','-i',path,'-f','s16le','-ac','2','-ar','48000','-']).stdout
        values=array.array('h');values.frombytes(raw)
        rms=math.sqrt(sum(x*x for x in values)/len(values));assert rms>100,f'silent audio: RMS={rms}'
        res[codec]={'bytes':path.stat().st_size,'decoded_samples':len(values),'rms':round(rms,1)}
    return res
test('pulse_monitor_audio',capture_audio)

async def input_storm():
    os.environ.update({k:v for k,v in session.items() if isinstance(v,str)})
    # python-xlib requires a FamilyLocal record; keep the same server cookie.
    import socket,struct
    data=Path(session['XAUTHORITY']).read_bytes();cookie_bytes=data[-16:]
    local=struct.pack('>H',256)
    for field in (socket.gethostname().encode(),session['DISPLAY'].lstrip(':').split('.')[0].encode(),b'MIT-MAGIC-COOKIE-1',cookie_bytes):
        local+=struct.pack('>H',len(field))+field
    auth=out/'Xauthority-qa';auth.write_bytes(local);auth.chmod(0o600);os.environ['XAUTHORITY']=str(auth)
    d=xdisplay.Display(session['DISPLAY']);screen=d.screen()
    w=screen.root.create_window(100,100,600,400,0,screen.root_depth,X.InputOutput,X.CopyFromParent,background_pixel=screen.white_pixel,override_redirect=True,event_mask=X.KeyPressMask|X.KeyReleaseMask|X.ButtonPressMask|X.ButtonReleaseMask|X.PointerMotionMask)
    w.map();w.set_input_focus(X.RevertToParent,X.CurrentTime);d.sync()
    stop=threading.Event();events=[]
    def observe():
        while not stop.is_set():
            while d.pending_events():
                e=d.next_event();events.append((e.type,getattr(e,'detail',None),time.monotonic()))
            time.sleep(.001)
    observer=threading.Thread(target=observe,daemon=True);observer.start()
    latencies=[];pongs={}
    try:
        async with websockets.connect(f'ws://127.0.0.1:{args.port}/ws?role=input&client_id=qa-storm',additional_headers={'Cookie':cookie},max_size=16*1024*1024) as ws:
            async def read():
                async for m in ws:
                    if isinstance(m,str):
                        m=json.loads(m)
                        if m.get('type')=='pong':pongs[m['seq']]=time.monotonic()
                        elif m.get('type')=='error':print('server input message',m,flush=True)
            reader=asyncio.create_task(read());await asyncio.sleep(.2)
            async def send(m):await ws.send(json.dumps(m))
            await send({'type':'pointer_absolute','x':250,'y':250})
            start=time.monotonic();sent={}
            for i in range(2000):
                await send({'type':'pointer_wheel','delta_x':0,'delta_y':120 if i%2 else -120,'delta_mode':0,'scroll_speed':1})
                if i%40==0:
                    await send({'type':'pointer_absolute','x':220+i%120,'y':240})
                    await send({'type':'key','key':'a','down':True})
                    await send({'type':'key','key':'a','down':False})
                    await send({'type':'pointer_button','button':1,'down':True})
                    await send({'type':'pointer_button','button':1,'down':False})
                    sent[i]=time.monotonic();await send({'type':'ping','seq':i})
                if i%50==0:await asyncio.sleep(.001)
            end=time.monotonic()+5
            while len(pongs)<len(sent) and time.monotonic()<end:await asyncio.sleep(.01)
            await send({'type':'reset_input'});await asyncio.sleep(.3)
            latencies=[(pongs[n]-t)*1000 for n,t in sent.items() if n in pongs]
            assert len(latencies)==50,(len(latencies),'missing pongs')
            # Full trace must show both edges despite simultaneous scroll.
            keycode=d.keysym_to_keycode(ord('a'))
            downs=sum(t==X.KeyPress and c==keycode for t,c,_ in events)
            ups=sum(t==X.KeyRelease and c==keycode for t,c,_ in events)
            buttons=sum(t==X.ButtonRelease and c==1 for t,c,_ in events)
            assert downs==50 and ups>=50 and buttons>=50,(downs,ups,buttons)
            elapsed=time.monotonic()-start
            reader.cancel()
        await asyncio.sleep(.15)
        result={'wheel_messages':2000,'interleaved_key_pairs':downs,'button_releases':buttons,'pongs':len(latencies),'elapsed_s':round(elapsed,3),'ping_p50_ms':round(statistics.median(latencies),2),'ping_p95_ms':round(sorted(latencies)[47],2),'ping_max_ms':round(max(latencies),2)}
        assert max(latencies)<500,'severe input stall under scroll'
        return result
    finally:
        stop.set();observer.join(timeout=1)
        w.destroy();d.sync();d.close()
test('mixed_input_scroll_storm',lambda:asyncio.run(input_storm()))
report={'passed':results,'failed':errors}
(out/'results.json').write_text(json.dumps(report,indent=2))
assert not errors,errors
