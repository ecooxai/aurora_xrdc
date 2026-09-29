//! Bounded wheel worker. Slow X11 or subprocess work never runs on the input receiver.
use crate::{ffmpeg::run_xdotool, uinput::UInputWheelInjector, x11_input::X11InputInjector};
use anyhow::Result;
use std::{
    collections::VecDeque,
    sync::{Arc, Condvar, Mutex},
    thread,
    time::{Duration, Instant},
};
use tracing::warn;
// The X11 backend injects wheel input as button clicks. Keep pixel-mode input
// responsive for slow trackpad deltas, but never turn one browser wheel event
// into a burst of server clicks.
const WHEEL_PIXEL_STEP: f64 = 12.0;
const WHEEL_LINE_STEP: f64 = 3.0;
const WHEEL_PAGE_STEPS: f64 = 8.0;
const WHEEL_MAX_STEPS_PER_MESSAGE: f64 = 1.0;
const WHEEL_GESTURE_IDLE_INTERVAL: Duration = Duration::from_millis(800);
const WEBCLIENT_CLICK_SCROLL_DISTANCE_SCALE: f64 = 0.5;
const WEBCLIENT_SMOOTH_SCROLL_DISTANCE_SCALE: f64 = 2.0;
const SMOOTH_WHEEL_UNITS_PER_PIXEL: f64 = 1.0;
const SMOOTH_WHEEL_LINE_PIXELS: f64 = 40.0;
const SMOOTH_WHEEL_PAGE_PIXELS: f64 = 800.0;
const SMOOTH_WHEEL_MAX_UNITS_PER_MESSAGE: f64 = 120.0;

#[derive(Debug, Default)]
pub(crate) struct WheelAccumulator {
    x_steps: f64,
    y_steps: f64,
    x_last_at: Option<Instant>,
    y_last_at: Option<Instant>,
    x_last_sign: i8,
    y_last_sign: i8,
}

pub(crate) fn wheel_clicks_for_delta(
    accumulator: &mut WheelAccumulator,
    delta_x: f64,
    delta_y: f64,
    delta_mode: Option<u8>,
    scroll_speed: Option<f64>,
) -> Vec<(u8, u32)> {
    let speed = wheel_speed_scale(
        scroll_speed,
        delta_mode,
        WEBCLIENT_CLICK_SCROLL_DISTANCE_SCALE,
    );
    let x_steps = normalize_wheel_delta(delta_x, delta_mode) * speed;
    let y_steps = normalize_wheel_delta(delta_y, delta_mode) * speed;
    let horizontal_steps = accumulate_wheel_steps(
        &mut accumulator.x_steps,
        &mut accumulator.x_last_at,
        &mut accumulator.x_last_sign,
        x_steps,
        true,
    );
    let vertical_steps = accumulate_wheel_steps(
        &mut accumulator.y_steps,
        &mut accumulator.y_last_at,
        &mut accumulator.y_last_sign,
        y_steps,
        true,
    );
    let mut clicks = Vec::with_capacity(2);
    if horizontal_steps < 0 {
        clicks.push((6, horizontal_steps.unsigned_abs()));
    } else if horizontal_steps > 0 {
        clicks.push((7, horizontal_steps as u32));
    }
    if vertical_steps < 0 {
        clicks.push((4, vertical_steps.unsigned_abs()));
    } else if vertical_steps > 0 {
        clicks.push((5, vertical_steps as u32));
    }
    clicks
}

pub(crate) fn smooth_wheel_units_for_delta(
    accumulator: &mut WheelAccumulator,
    delta_x: f64,
    delta_y: f64,
    delta_mode: Option<u8>,
    scroll_speed: Option<f64>,
) -> (i32, i32) {
    let speed = wheel_speed_scale(
        scroll_speed,
        delta_mode,
        WEBCLIENT_SMOOTH_SCROLL_DISTANCE_SCALE,
    );
    let x_units = normalize_smooth_wheel_delta(delta_x, delta_mode) * speed;
    let y_units = normalize_smooth_wheel_delta(delta_y, delta_mode) * speed;
    let horizontal_units = accumulate_wheel_steps(
        &mut accumulator.x_steps,
        &mut accumulator.x_last_at,
        &mut accumulator.x_last_sign,
        x_units,
        false,
    );
    let vertical_units = accumulate_wheel_steps(
        &mut accumulator.y_steps,
        &mut accumulator.y_last_at,
        &mut accumulator.y_last_sign,
        y_units,
        false,
    );
    (horizontal_units, -vertical_units)
}

fn normalize_wheel_delta(delta: f64, delta_mode: Option<u8>) -> f64 {
    if !delta.is_finite() {
        return 0.0;
    }
    let delta = delta.clamp(-10_000.0, 10_000.0);
    match delta_mode {
        // Old clients sent already-quantized wheel steps and had no deltaMode.
        None => delta,
        Some(0) => delta / WHEEL_PIXEL_STEP,
        Some(1) => delta / WHEEL_LINE_STEP,
        Some(2) => delta * WHEEL_PAGE_STEPS,
        Some(_) => delta / WHEEL_PIXEL_STEP,
    }
    .clamp(-WHEEL_MAX_STEPS_PER_MESSAGE, WHEEL_MAX_STEPS_PER_MESSAGE)
}

fn wheel_speed_scale(
    scroll_speed: Option<f64>,
    delta_mode: Option<u8>,
    webclient_scale: f64,
) -> f64 {
    let speed = scroll_speed
        .filter(|speed| speed.is_finite())
        .unwrap_or(1.0)
        .clamp(0.1, 5.0);
    if delta_mode.is_some() {
        speed * webclient_scale
    } else {
        speed
    }
}

fn normalize_smooth_wheel_delta(delta: f64, delta_mode: Option<u8>) -> f64 {
    if !delta.is_finite() {
        return 0.0;
    }
    let delta = delta.clamp(-10_000.0, 10_000.0);
    match delta_mode {
        None => delta * 120.0,
        Some(0) => delta * SMOOTH_WHEEL_UNITS_PER_PIXEL,
        Some(1) => delta * SMOOTH_WHEEL_LINE_PIXELS,
        Some(2) => delta * SMOOTH_WHEEL_PAGE_PIXELS,
        Some(_) => delta * SMOOTH_WHEEL_UNITS_PER_PIXEL,
    }
    .clamp(
        -SMOOTH_WHEEL_MAX_UNITS_PER_MESSAGE,
        SMOOTH_WHEEL_MAX_UNITS_PER_MESSAGE,
    )
}

fn accumulate_wheel_steps(
    remainder: &mut f64,
    last_at: &mut Option<Instant>,
    last_sign: &mut i8,
    delta_steps: f64,
    kick_start: bool,
) -> i32 {
    if !delta_steps.is_finite() || delta_steps == 0.0 {
        return 0;
    }
    let now = Instant::now();
    let idle =
        last_at.is_none_or(|last_at| now.duration_since(last_at) > WHEEL_GESTURE_IDLE_INTERVAL);
    let sign = if delta_steps > 0.0 { 1 } else { -1 };
    if idle || (*last_sign != 0 && *last_sign != sign) {
        *remainder = 0.0;
        *last_sign = 0;
    }
    *last_at = Some(now);
    *last_sign = sign;
    *remainder += delta_steps;
    let mut whole_steps = if *remainder >= 0.0 {
        remainder.floor()
    } else {
        remainder.ceil()
    };
    // On a fresh gesture (the pointer has been idle), emit at least one step in
    // the scroll direction right away instead of swallowing the first event
    // while the sub-step remainder fills up. This keeps coarse XTEST wheel
    // clicks (headless X11) responsive: the first notch moves immediately. The
    // borrowed fraction is repaid against the next event so the overall scroll
    // rate is unchanged. Only the discrete-click path opts in; the high-res
    // uinput path already moves on every event.
    if kick_start && idle && whole_steps == 0.0 {
        whole_steps = sign as f64;
    }
    *remainder -= whole_steps;
    whole_steps as i32
}

async fn apply_wheel_clicks(
    display: &str,
    input_injector: Option<&X11InputInjector>,
    clicks: &[(u8, u32)],
) -> Result<()> {
    if clicks.is_empty() {
        return Ok(());
    }
    if let Some(input_injector) = input_injector {
        for (button, count) in clicks {
            for _ in 0..*count {
                input_injector.queue_pointer_click(*button)?;
            }
        }
        return input_injector.flush();
    }
    let mut args = Vec::new();
    for (button, count) in clicks {
        for _ in 0..*count {
            args.push("click".to_string());
            args.push("--delay".to_string());
            args.push("0".to_string());
            args.push(button.to_string());
        }
    }
    run_xdotool(display, &args).await
}

const MAX_PENDING: usize = 16;
const MAX_AGE: Duration = Duration::from_millis(100);

#[derive(Clone, Copy, Debug)]
pub(crate) struct WheelEvent {
    x: f64,
    y: f64,
    mode: Option<u8>,
    speed: Option<f64>,
    at: Instant,
}
impl WheelEvent {
    pub fn new(x: f64, y: f64, mode: Option<u8>, speed: Option<f64>) -> Self {
        Self {
            x,
            y,
            mode,
            speed,
            at: Instant::now(),
        }
    }
}
#[derive(Default)]
struct Pending {
    events: VecDeque<WheelEvent>,
    stopped: bool,
}
impl Pending {
    fn push(&mut self, event: WheelEvent) {
        if self.stopped || !event.x.is_finite() || !event.y.is_finite() {
            return;
        }
        // Do not replay an old scroll after congestion. Memory and latency are bounded.
        while self
            .events
            .front()
            .is_some_and(|e| e.at.elapsed() > MAX_AGE)
            || self.events.len() >= MAX_PENDING
        {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }
}

pub(crate) struct WheelDispatcher {
    shared: Arc<(Mutex<Pending>, Condvar)>,
}
impl WheelDispatcher {
    pub fn new(display: String, use_uinput: bool) -> Result<Self> {
        let shared = Arc::new((Mutex::new(Pending::default()), Condvar::new()));
        let worker = shared.clone();
        thread::Builder::new()
            .name("aurora-wheel".into())
            .spawn(move || {
                let rt = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(rt) => rt,
                    Err(err) => {
                        warn!("wheel runtime failed: {err}");
                        return;
                    }
                };
                let x11 = X11InputInjector::connect(&display).ok();
                let mut smooth = if use_uinput {
                    UInputWheelInjector::connect().ok()
                } else {
                    None
                };
                let mut accumulator = WheelAccumulator::default();
                loop {
                    let event = {
                        let (lock, ready) = &*worker;
                        let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
                        while !q.stopped && q.events.is_empty() {
                            q = ready.wait(q).unwrap_or_else(|e| e.into_inner());
                        }
                        if q.stopped {
                            break;
                        }
                        q.events.pop_front().unwrap()
                    }; // Never hold the queue lock while performing I/O.
                    if event.at.elapsed() > MAX_AGE {
                        continue;
                    }
                    let result = if let Some(injector) = smooth.as_mut() {
                        let (x, y) = smooth_wheel_units_for_delta(
                            &mut accumulator,
                            event.x,
                            event.y,
                            event.mode,
                            event.speed,
                        );
                        injector.emit_scroll(x, y)
                    } else {
                        let clicks = wheel_clicks_for_delta(
                            &mut accumulator,
                            event.x,
                            event.y,
                            event.mode,
                            event.speed,
                        );
                        rt.block_on(apply_wheel_clicks(&display, x11.as_ref(), &clicks))
                    };
                    if let Err(err) = result {
                        warn!("wheel injection failed: {err}");
                    }
                }
            })?;
        Ok(Self { shared })
    }
    pub fn submit(&self, event: WheelEvent) {
        let (lock, ready) = &*self.shared;
        lock.lock().unwrap_or_else(|e| e.into_inner()).push(event);
        ready.notify_one();
    }
    pub fn clear(&self) {
        self.shared
            .0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .events
            .clear();
    }
}
impl Drop for WheelDispatcher {
    fn drop(&mut self) {
        let (lock, ready) = &*self.shared;
        let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
        q.stopped = true;
        q.events.clear();
        ready.notify_one();
        // No join on a network task: the worker exits after its bounded in-flight operation.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wheel_storm_has_fixed_memory() {
        let mut q = Pending::default();
        for _ in 0..100_000 {
            q.push(WheelEvent::new(0.0, 1.0, Some(0), None));
        }
        assert_eq!(q.events.len(), MAX_PENDING);
    }
    #[test]
    fn stale_wheel_is_not_replayed() {
        let mut q = Pending::default();
        let mut old = WheelEvent::new(0.0, 1.0, Some(0), None);
        old.at = Instant::now() - Duration::from_secs(1);
        q.events.push_back(old);
        q.push(WheelEvent::new(0.0, -1.0, Some(0), None));
        assert_eq!(q.events.len(), 1);
        assert_eq!(q.events[0].y, -1.0);
    }
    #[test]
    fn stopped_and_nonfinite_input_ignored() {
        let mut q = Pending::default();
        q.push(WheelEvent::new(f64::NAN, 1.0, None, None));
        q.push(WheelEvent::new(0.0, f64::INFINITY, None, None));
        assert!(q.events.is_empty());
        q.stopped = true;
        q.push(WheelEvent::new(0.0, 1.0, None, None));
        assert!(q.events.is_empty());
    }
}
