#!/usr/bin/env python3
"""Browser, codec, audio and input tests against an owned port-11220 session."""
import argparse,asyncio,importlib.util,json,subprocess
from pathlib import Path
from playwright.async_api import async_playwright
ROOT=Path(__file__).resolve().parents[1]
parser=argparse.ArgumentParser()
parser.add_argument('--package',type=Path,required=True)
parser.add_argument('--output',type=Path,required=True)
args=parser.parse_args()
OUT=args.output.resolve();OUT.mkdir(parents=True,exist_ok=True)
PKG=args.package.resolve()
spec=importlib.util.spec_from_file_location('runtime',ROOT/'tests/launcher_runtime_test.py');runtime=importlib.util.module_from_spec(spec);spec.loader.exec_module(runtime)

async def browser_test():
    results={}
    async with async_playwright() as p:
        browser=await p.chromium.launch(executable_path='/home/dev/.local/bin/chromium',headless=True,args=['--no-sandbox'])
        try:
            for transport in ['websocket','webrtc-dc','webrtc-media']:
                errors=[]
                context=await browser.new_context(viewport={'width':1440,'height':1000})
                await context.add_init_script("localStorage.setItem('vibe_rdesk_transport',"+json.dumps(transport)+");window.__qaRTC=[];const PC=window.RTCPeerConnection;if(PC)window.RTCPeerConnection=class extends PC {constructor(...a){super(...a);window.__qaRTC.push(this);}};")
                page=await context.new_page();page.on('pageerror',lambda err:errors.append(str(err)))
                await page.goto('http://127.0.0.1:11220/',wait_until='domcontentloaded')
                await page.locator('#auth-passwd').fill('2208');await page.locator('#auth-form button').click()
                await page.locator('#auth-modal.hidden').wait_for(state='attached',timeout=15000)
                await page.wait_for_timeout(4500)
                info=await page.locator('#screen').evaluate('''async c=>{
                    const image=await createImageBitmap(c),tmp=new OffscreenCanvas(c.width,c.height),ctx=tmp.getContext('2d');
                    ctx.drawImage(image,0,0);image.close();const bytes=ctx.getImageData(0,0,c.width,c.height).data;
                    let n=0;for(let i=0;i<bytes.length;i+=4000)if(bytes[i]+bytes[i+1]+bytes[i+2]>30)n++;
                    return {width:c.width,height:c.height,nonblank_pixel_samples:n};
                }''')
                peers=await page.evaluate('window.__qaRTC.map(p=>({state:p.connectionState,ice:p.iceConnectionState,tracks:p.getReceivers().map(r=>r.track?.kind)}))')
                assert info['nonblank_pixel_samples']>50,info
                if transport.startswith('webrtc'):assert any(p['state']=='connected' for p in peers),peers
                await page.screenshot(path=str(OUT/f'browser-{transport}.png'))
                await page.mouse.move(640,410)
                for i in range(20):await page.mouse.wheel(0,100);await page.mouse.move(640+i,410)
                await page.keyboard.press('a')
                await page.set_viewport_size({'width':390,'height':844})
                overflow=await page.evaluate('Math.max(0,document.documentElement.scrollWidth-innerWidth)')
                assert not errors,errors
                assert overflow==0,overflow
                results[transport]={**info,'peers':peers,'javascript_errors':errors,'mobile_horizontal_overflow':overflow}
                await context.close()
        finally:await browser.close()
    (OUT/'browser-results.json').write_text(json.dumps(results,indent=2)+'\n');print(json.dumps(results,indent=2))

s=runtime.Session(PKG,11220,':220')
try:
    s.start()
    password=s.root/'qa-password';password.write_text('2208');password.chmod(0o600)
    with (OUT/'full-stream-input.log').open('wb') as log:
        p=subprocess.run(['python3',str(ROOT/'tests/live_qa.py'),'--port','11220','--session',str(s.session),'--password-file',str(password)],cwd=ROOT,stdout=log,stderr=log,timeout=120)
    print((OUT/'full-stream-input.log').read_text())
    assert p.returncode==0,'stream/input regression failed'
    source=ROOT/'.output/live-qa/results.json';(OUT/'stream-input-results.json').write_bytes(source.read_bytes())
    asyncio.run(browser_test())
finally:s.close()
