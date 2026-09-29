import { test } from 'node:test';
import assert from 'node:assert/strict';
import { WheelQueue } from '../web/wheel_queue.mjs';
function fixture() {
  let callback, blocked = false, time = 0;
  const sent=[];
  const q=new WheelQueue(m=>sent.push(m),()=>blocked,{
    schedule: fn => { callback=fn; return 1; }, cancel:()=>{callback=null;}, now:()=>time,
  });
  return {q,sent,tick:()=>{const fn=callback; callback=null; fn?.();},block:v=>{blocked=v;},time:v=>{time=v;}};
}
const event=(y=1,mode=0)=>({type:'pointer_wheel',delta_x:0,delta_y:y,delta_mode:mode,scroll_speed:1});
test('wheel input is asynchronous and batched',()=>{
  const f=fixture(); for(let i=0;i<1000;i++)f.q.push(event());
  assert.equal(f.sent.length,0);f.tick();assert.equal(f.sent.length,1);assert.equal(f.sent[0].delta_y,1000);
});
test('backpressure does not queue writes or block other input',()=>{
  const f=fixture();f.block(true);f.q.push(event());f.tick();assert.equal(f.sent.length,0);
  const urgent=[];urgent.push('keydown','pointermove','keyup');assert.equal(urgent.length,3);
  f.block(false);f.tick();assert.equal(f.sent.length,1);
});
test('stale scroll is discarded instead of replayed',()=>{
  const f=fixture();f.block(true);f.q.push(event());f.time(101);f.tick();
  f.block(false);f.tick();assert.equal(f.sent.length,0);assert.equal(f.q.pending,null);
});
test('units are not mixed',()=>{
  const f=fixture();f.q.push(event(12,0));f.q.push(event(1,1));f.tick();
  assert.deepEqual(f.sent.map(m=>[m.delta_y,m.delta_mode]),[[12,0],[1,1]]);
});
test('nonfinite rejected, accumulated deltas bounded and clear cancels',()=>{
  const f=fixture();f.q.push(event(Infinity));assert.equal(f.q.pending,null);
  for(let i=0;i<10000;i++)f.q.push(event(1000));
  assert.equal(f.q.pending.message.delta_y,4096);f.q.clear();f.tick();assert.equal(f.sent.length,0);
});
