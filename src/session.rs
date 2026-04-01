use std::{
    collections::HashSet,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::Result;
use axum::extract::ws::{Message, WebSocket};
use futures_util::StreamExt;
use tokio::{
    process::Child,
    sync::{Mutex, mpsc},
};
use tracing::{error, warn};

use crate::{
    audio::AudioFrame,
    audio_streamer,
    clipboard::{read_remote_clipboard, write_remote_clipboard},
    ffmpeg::{read_stderr, run_xdotool},
    messages::{ClientMessage, ServerMessage},
    settings::{ServerConfig, StreamConfig},
    streamer::{StreamFrame, start},
    system_stats::StatsSampler,
};

pub async fn handle_socket(
    socket: WebSocket,
    server: ServerConfig,
    config: StreamConfig,
) -> Result<()> {
    let (stream, child) = start(server.clone(), config.clone()).await?;
    let video_child = Arc::new(Mutex::new(child));
    let (audio_stream, audio_child) = match audio_streamer::start(&server).await {
        Ok((stream, child)) => (Some(stream), Some(Arc::new(Mutex::new(child)))),
        Err(_) => (None, None),
    };
    let last_latency = Arc::new(AtomicU64::new(0));
    let (mut ws_sender, receiver) = socket.split();
    let (out_tx, mut out_rx) = mpsc::channel::<Message>(32);
    let audio_enabled = audio_stream.is_some();
    let session_id = format!("{:x}", rand_id());
    out_tx
        .send(Message::Text(
            serde_json::to_string(&ServerMessage::Hello {
                session_id,
                display: server.display.clone(),
                config: config.clone(),
                active_encoder: stream.encoder.ffmpeg_encoder.clone(),
                encoder_mode: stream.encoder.mode,
                codec_string: config.codec.as_webcodec().into(),
                description_b64: None,
                audio_enabled,
            })?
            .into(),
        ))
        .await?;
    if !audio_enabled {
        let _ = out_tx
            .send(Message::Text(
                serde_json::to_string(&ServerMessage::Error {
                    code: "audio_unavailable",
                    message: "Audio capture is unavailable. Install and run PulseAudio/PipeWire with pactl support, or set VIBE_RDESK_AUDIO_SOURCE.".into(),
                })?
                .into(),
            ))
            .await;
    }
    let writer_task = tokio::spawn(async move {
        while let Some(message) = out_rx.recv().await {
            if futures_util::SinkExt::send(&mut ws_sender, message).await.is_err() {
                break;
            }
        }
    });
    let stats_task = spawn_stats(out_tx.clone(), config.clone(), stream.encoder.ffmpeg_encoder.clone(), stream.encoder.mode, last_latency.clone());
    let send_task = tokio::spawn(forward_frames(out_tx.clone(), stream.rx, config.clone(), stream.encoder.ffmpeg_encoder.clone(), stream.encoder.mode, audio_enabled));
    let audio_task = audio_stream.map(|stream| tokio::spawn(forward_audio(out_tx.clone(), stream.rx)));
    let recv_task = tokio::spawn(handle_client(receiver, out_tx.clone(), server.display.clone(), last_latency));
    let _ = tokio::try_join!(send_task, recv_task);
    stats_task.abort();
    if let Some(task) = audio_task {
        task.abort();
    }
    writer_task.abort();
    kill_ffmpeg(video_child).await;
    if let Some(child) = audio_child {
        kill_ffmpeg(child).await;
    }
    Ok(())
}

async fn forward_frames(
    sender: mpsc::Sender<Message>,
    mut rx: tokio::sync::mpsc::Receiver<StreamFrame>,
    config: StreamConfig,
    encoder: String,
    mode: &'static str,
    audio_enabled: bool,
) -> Result<()> {
    while let Some(frame) = rx.recv().await {
        sender
            .send(Message::Binary(binary_frame(&frame).into()))
            .await?;
        if frame.description_b64.is_some() {
            sender
                .send(Message::Text(
                    serde_json::to_string(&ServerMessage::Hello {
                        session_id: String::new(),
                        display: String::new(),
                        config: config.clone(),
                        active_encoder: encoder.clone(),
                        encoder_mode: mode,
                        codec_string: frame.codec.as_webcodec().into(),
                        description_b64: frame.description_b64.clone(),
                        audio_enabled,
                    })?
                    .into(),
                ))
                .await?;
        }
    }
    Ok(())
}

async fn forward_audio(
    sender: mpsc::Sender<Message>,
    mut rx: tokio::sync::mpsc::Receiver<AudioFrame>,
) -> Result<()> {
    while let Some(frame) = rx.recv().await {
        sender
            .send(Message::Binary(binary_audio_frame(&frame).into()))
            .await?;
    }
    Ok(())
}

fn spawn_stats(
    sender: mpsc::Sender<Message>,
    config: StreamConfig,
    encoder: String,
    mode: &'static str,
    latency: Arc<AtomicU64>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut sampler = StatsSampler::new();
        let mut tick = tokio::time::interval(Duration::from_secs(3));
        loop {
            tick.tick().await;
            let sample = sampler.sample();
            let msg = ServerMessage::Stats {
                capture_fps: config.fps as f32,
                bitrate_kbps: config.bitrate_kbps,
                queue_depth: 0,
                active_encoder: encoder.clone(),
                encoder_mode: mode,
                codec: config.codec,
                cpu_usage: sample.cpu_usage,
                memory_used_mb: sample.memory_used_mb,
                memory_total_mb: sample.memory_total_mb,
                swap_used_mb: sample.swap_used_mb,
                swap_total_mb: sample.swap_total_mb,
                net_tx_kbps: sample.net_tx_kbps,
                latency_ms: latency.load(Ordering::Relaxed),
            };
            match serde_json::to_string(&msg) {
                Ok(text) => {
                    if sender.send(Message::Text(text.into())).await.is_err() {
                        return;
                    }
                }
                Err(err) => {
                    error!("stats serialize failed: {err}");
                    return;
                }
            }
        }
    })
}

async fn handle_client(
    mut receiver: futures_util::stream::SplitStream<WebSocket>,
    sender: mpsc::Sender<Message>,
    display: String,
    last_latency: Arc<AtomicU64>,
) -> Result<()> {
    let mut pressed_keys = HashSet::new();
    while let Some(message) = receiver.next().await {
        match message? {
            Message::Text(text) => match serde_json::from_str::<ClientMessage>(&text) {
                Ok(client) => apply_client_message(&display, &sender, client, &last_latency, &mut pressed_keys).await?,
                Err(err) => warn!("invalid client message: {err}"),
            },
            Message::Close(_) => break,
            _ => {}
        }
    }
    reset_input_state(&display, &mut pressed_keys).await?;
    Ok(())
}

async fn apply_client_message(
    display: &str,
    sender: &mpsc::Sender<Message>,
    message: ClientMessage,
    last_latency: &AtomicU64,
    pressed_keys: &mut HashSet<String>,
) -> Result<()> {
    match message {
        ClientMessage::PointerMove { dx, dy } => {
            run_xdotool(display, ["mousemove_relative", "--", &dx.round().to_string(), &dy.round().to_string()]).await?;
        }
        ClientMessage::PointerAbsolute { x, y } => {
            run_xdotool(display, ["mousemove", &x.to_string(), &y.to_string()]).await?;
        }
        ClientMessage::PointerButton { button, down } => {
            let action = if down { "mousedown" } else { "mouseup" };
            run_xdotool(display, [action, &button.to_string()]).await?;
        }
        ClientMessage::PointerWheel { delta_y } => {
            let button = if delta_y < 0 { "4" } else { "5" };
            run_xdotool(display, ["click", button]).await?;
        }
        ClientMessage::TouchTap => {
            run_xdotool(display, ["click", "1"]).await?;
        }
        ClientMessage::Key { key, down } => {
            if down {
                pressed_keys.insert(key.clone());
            } else {
                pressed_keys.remove(&key);
            }
            let action = if down { "keydown" } else { "keyup" };
            run_xdotool(display, [action, &key]).await?;
        }
        ClientMessage::Paste => {
            reset_input_state(display, pressed_keys).await?;
            run_xdotool(display, ["key", "ctrl+v"]).await?;
        }
        ClientMessage::PasteClipboard { payload } => {
            write_remote_clipboard(display, &payload).await?;
            tokio::time::sleep(Duration::from_millis(80)).await;
            sender
                .send(Message::Text(
                    serde_json::to_string(&ServerMessage::Clipboard {
                        side: "remote",
                        payload: payload.clone(),
                    })?
                    .into(),
                ))
                .await?;
            reset_input_state(display, pressed_keys).await?;
            run_xdotool(display, ["key", "ctrl+v"]).await?;
        }
        ClientMessage::ResetInput => {
            reset_input_state(display, pressed_keys).await?;
        }
        ClientMessage::Ping { sent_at_ms } => {
            let now = crate::streamer::start;
            let _ = now;
            let current = current_ms().saturating_sub(sent_at_ms);
            last_latency.store(current, Ordering::Relaxed);
        }
        ClientMessage::ClipboardSet { payload } => {
            write_remote_clipboard(display, &payload).await?;
            sender
                .send(Message::Text(
                    serde_json::to_string(&ServerMessage::Clipboard {
                        side: "remote",
                        payload,
                    })?
                    .into(),
                ))
                .await?;
        }
        ClientMessage::ClipboardGet => {
            send_clipboard_update(display, sender).await?;
        }
    }
    Ok(())
}

async fn reset_input_state(display: &str, pressed_keys: &mut HashSet<String>) -> Result<()> {
    for button in ["1", "2", "3", "4", "5"] {
        run_xdotool(display, ["mouseup", button]).await?;
    }
    for key in pressed_keys.drain() {
        run_xdotool(display, ["keyup", &key]).await?;
    }
    for key in [
        "Shift_L",
        "Shift_R",
        "Control_L",
        "Control_R",
        "Alt_L",
        "Alt_R",
        "Super_L",
        "Super_R",
    ] {
        run_xdotool(display, ["keyup", key]).await?;
    }
    Ok(())
}

async fn send_clipboard_update(display: &str, sender: &mpsc::Sender<Message>) -> Result<()> {
    let payload = read_remote_clipboard(display).await?;
    sender
        .send(Message::Text(
            serde_json::to_string(&ServerMessage::Clipboard {
                side: "remote",
                payload,
            })?
            .into(),
        ))
        .await?;
    Ok(())
}

async fn kill_ffmpeg(child: Arc<Mutex<Child>>) {
    let mut child = child.lock().await;
    if let Err(err) = child.kill().await {
        warn!("ffmpeg kill failed: {err}");
    }
    let stderr = read_stderr(&mut child).await;
    if !stderr.trim().is_empty() {
        warn!("ffmpeg stderr: {}", stderr.trim());
    }
}

fn binary_frame(frame: &StreamFrame) -> Vec<u8> {
    let mut out = Vec::with_capacity(18 + frame.bytes.len());
    out.push(1);
    out.push(u8::from(frame.keyframe));
    out.extend_from_slice(&frame.sent_at_ms.to_le_bytes());
    out.extend_from_slice(&(frame.bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(&frame.bytes);
    out
}

fn binary_audio_frame(frame: &AudioFrame) -> Vec<u8> {
    let mut out = Vec::with_capacity(17 + frame.bytes.len());
    out.push(2);
    out.extend_from_slice(&frame.sent_at_ms.to_le_bytes());
    out.extend_from_slice(&(frame.bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(&frame.bytes);
    out
}

fn current_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn rand_id() -> u64 {
    current_ms() ^ Instant::now().elapsed().as_nanos() as u64
}
