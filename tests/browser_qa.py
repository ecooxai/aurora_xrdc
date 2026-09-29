#!/usr/bin/env python3
import asyncio,json,time,os
from pathlib import Path
from playwright.async_api import async_playwright
ROOT=Path(__file__).resolve().parents[1]
async def main():
    password=(ROOT/'.output/qa-password').read_text().strip()
    errors=[];http_errors=[];result={}
    async with async_playwright() as p:
        browser=await p.chromium.launch(executable_path='/home/dev/.local/bin/chromium',headless=True,args=['--no-sandbox'])
        context=await browser.new_context(viewport={'width':1440,'height':1000},ignore_https_errors=True)
        transport=os.environ.get('AURORA_TEST_TRANSPORT','websocket')
        result['transport']=transport
        await context.add_init_script(f"localStorage.setItem('vibe_rdesk_transport',{json.dumps(transport)});")
        await context.add_init_script("""window.__qaRTC=[];
          const OriginalPC=window.RTCPeerConnection;
          if(OriginalPC)window.RTCPeerConnection=class extends OriginalPC {
            constructor(...args){super(...args);window.__qaRTC.push(this);}
          };
        """)
        page=await context.new_page()
        page.on('pageerror',lambda error:errors.append(str(error)))
        page.on('response',lambda r:http_errors.append([r.status,r.url]) if r.status>=500 else None)
        start=time.monotonic()
        await page.goto('http://127.0.0.1:19990/',wait_until='domcontentloaded')
        await page.locator('#auth-modal:not(.hidden)').wait_for(timeout=10000)
        result['login_visible_ms']=round((time.monotonic()-start)*1000,1)
        await page.locator('#auth-passwd').fill(password)
        await page.locator('#auth-form button[type=submit]').click()
        await page.locator('#auth-modal.hidden').wait_for(state='attached',timeout=15000)
        await page.wait_for_function("document.querySelector('#screen')?.width >= 640",timeout=30000)
        await page.wait_for_timeout(3000)
        result['fps_label']=await page.locator('#status-fps').text_content()
        result['fps_detail']=await page.locator('#status-fps-meta').text_content()
        result['video_pixels']=await page.locator('#screen').evaluate("""async (c)=>{
            const image=await createImageBitmap(c);const tmp=new OffscreenCanvas(c.width,c.height);
            const ctx=tmp.getContext('2d');ctx.drawImage(image,0,0);image.close();
            const p=ctx.getImageData(0,0,c.width,c.height).data;let n=0;
            for(let i=0;i<p.length;i+=4000)if(p[i]+p[i+1]+p[i+2]>30)n++;return n;
        }""")
        result['canvas']=await page.locator('#screen').evaluate('(c)=>({width:c.width,height:c.height})')
        await page.screenshot(path=str(ROOT/f'.output/browser-desktop-{transport}.png'),full_page=True)
        await page.mouse.move(650,450)
        for _ in range(40):
            await page.mouse.wheel(0,100)
            await page.mouse.move(650+_%30,450)
        await page.keyboard.press('a')
        await page.wait_for_timeout(600)
        result['rtc_connections']=await page.evaluate('window.__qaRTC.map(p=>({connection:p.connectionState,ice:p.iceConnectionState,receivers:p.getReceivers().map(r=>r.track?.kind)}))')
        result['after_scroll_fps']=await page.locator('#status-fps').text_content()
        await page.set_viewport_size({'width':390,'height':844})
        await page.screenshot(path=str(ROOT/f'.output/browser-mobile-{transport}.png'),full_page=True)
        result['mobile_overflow_px']=await page.evaluate('Math.max(0,document.documentElement.scrollWidth-innerWidth)')
        result['javascript_errors']=errors;result['http_5xx']=http_errors
        print(json.dumps(result,indent=2))
        (ROOT/f'.output/browser-qa-{transport}.json').write_text(json.dumps(result,indent=2))
        await context.close();await browser.close()
        assert not errors and not http_errors
        if transport.startswith('webrtc'):assert any(p['connection']=='connected' for p in result['rtc_connections']),result
        assert result['canvas']['width']>=640
        assert result['video_pixels'] is None or result['video_pixels']>50
asyncio.run(main())
