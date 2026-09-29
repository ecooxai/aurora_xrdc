/** Bounded, asynchronous wheel batching. Keyboard and pointer edges never await it. */
export class WheelQueue {
  constructor(send, congested, {
    schedule = (fn, ms) => setTimeout(fn, ms), cancel = clearTimeout,
    now = () => performance.now(), interval = 8, maxAge = 100,
  } = {}) {
    Object.assign(this, { send, congested, schedule, cancel, now, interval, maxAge });
    this.pending = null; this.timer = null;
  }
  push(message) {
    if (!Number.isFinite(message.delta_x) || !Number.isFinite(message.delta_y)) return;
    if (!message.delta_x && !message.delta_y) return;
    const at = this.now();
    if (this.pending && (at - this.pending.at > this.maxAge
      || this.pending.message.delta_mode !== message.delta_mode
      || this.pending.message.scroll_speed !== message.scroll_speed)) {
      this.flush();
      // Units must not be combined across pixel/line/page modes, even when congested.
      this.pending = null;
    }
    if (!this.pending) this.pending = { message: { ...message }, at };
    else {
      this.pending.message.delta_x = bounded(this.pending.message.delta_x + message.delta_x);
      this.pending.message.delta_y = bounded(this.pending.message.delta_y + message.delta_y);
    }
    this.pending.message.delta_x = bounded(this.pending.message.delta_x);
    this.pending.message.delta_y = bounded(this.pending.message.delta_y);
    this.arm();
  }
  arm() {
    if (this.timer !== null) return;
    this.timer = this.schedule(() => { this.timer = null; this.flush(); }, this.interval);
  }
  flush() {
    if (this.timer !== null) { this.cancel(this.timer); this.timer = null; }
    if (!this.pending) return;
    if (this.now() - this.pending.at > this.maxAge) { this.pending = null; return; }
    if (this.congested()) { this.arm(); return; }
    const { message } = this.pending; this.pending = null;
    this.send(message);
  }
  clear() {
    if (this.timer !== null) this.cancel(this.timer);
    this.timer = null; this.pending = null;
  }
}
const bounded = (delta) => Math.max(-4096, Math.min(4096, delta));
