#!/usr/bin/env python3
"""A broken client transport must not leave a key or pointer button held."""
import asyncio,json,os,socket,struct,time
from pathlib import Path
import websockets
from Xlib import X,display
ROOT=Path(__file__).resolve().parents[1]
async def main():
    state=json.loads((ROOT/'.output/qa-session/session.json').read_text())
    # Explicit auth compatible with python-xlib and both old/new launcher cookie records.
    data=Path(state['XAUTHORITY']).read_bytes();auth=struct.pack('>H',256)
    for f in (socket.gethostname().encode(),state['DISPLAY'].lstrip(':').encode(),b'MIT-MAGIC-COOKIE-1',data[-16:]):auth+=struct.pack('>H',len(f))+f
    file=ROOT/'.output/Xauthority-disconnect';file.write_bytes(auth);file.chmod(0o600)
    os.environ['XAUTHORITY']=str(file)
    d=display.Display(state['DISPLAY']);root=d.screen().root
    key=d.keysym_to_keycode(ord('b'))
    password=(ROOT/'.output/qa-password').read_text().strip()
    # Avoid password-in-URL logging: obtain and send the HttpOnly session cookie.
    import http.client
    c=http.client.HTTPConnection('127.0.0.1',19990);c.request('POST','/api/auth',json.dumps({'passwd':password}),{'Content-Type':'application/json'})
    response=c.getresponse();cookie=response.getheader('set-cookie').split(';')[0];response.read();c.close()
    ws=await websockets.connect('ws://127.0.0.1:19990/ws?role=input&client_id=qa-abrupt',additional_headers={'Cookie':cookie})
    await asyncio.sleep(.15)
    for m in [{'type':'key','key':'b','down':True},{'type':'pointer_button','button':1,'down':True}]:await ws.send(json.dumps(m))
    await asyncio.sleep(.15)
    before=bool(d.query_keymap()[key//8]&(1<<(key%8)))
    ws.transport.abort()
    start=time.monotonic();released=False
    for _ in range(100):
        keys=d.query_keymap();mask=root.query_pointer().mask
        if not keys[key//8]&(1<<(key%8)) and not mask&X.Button1Mask:released=True;break
        await asyncio.sleep(.02)
    result={'key_was_pressed':before,'released_after_broken_transport':released,'release_ms':round((time.monotonic()-start)*1000,1)}
    (ROOT/'.output/input-disconnect-qa.json').write_text(json.dumps(result,indent=2));print(json.dumps(result))
    # Always clear the owned test display after a failure as well.
    from Xlib.ext import xtest
    xtest.fake_input(d,X.KeyRelease,key);xtest.fake_input(d,X.ButtonRelease,1);d.sync();d.close()
    assert before and released
asyncio.run(main())
