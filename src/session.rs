use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

use anyhow::Result;
use axum::extract::ws::{Message, WebSocket};
use futures_util::StreamExt;
use tokio::{
    io::AsyncWriteExt,
    process::Child,
    sync::{broadcast, mpsc, watch},
    time::MissedTickBehavior,
};
use tracing::{error, warn};

use crate::{
    audio::AudioFrame,
    clipboard::{read_remote_clipboard, write_remote_clipboard},
    ffmpeg::{MicInputHandle, read_stderr, run_xdotool, spawn_mic_input_injector, wake_display},
    media::{ActiveAudioState, ActiveVideoState, MediaHub},
    messages::{ClientMessage, ServerMessage},
    settings::{AudioStreamConfig, ServerConfig, StreamConfig},
    streamer::StreamFrame,
    system_stats::StatsSampler,
};

const KEY_STATE_TIMEOUT: Duration = Duration::from_millis(500);
const KEY_STATE_WATCHDOG_INTERVAL: Duration = Duration::from_millis(100);
const MIC_STREAM_ID_BYTES: usize = std::mem::size_of::<u32>();
const AUDIO_PACKET_QUEUE_CAPACITY: usize = 128;
const DISPLAY_WAKE_INTERVAL: Duration = Duration::from_secs(2);

pub async fn handle_socket(
    socket: WebSocket,
    server: ServerConfig,
    media: MediaHub,
    config: StreamConfig,
    audio_config: AudioStreamConfig,
) -> Result<()> {
    let mut initial_display_wake_at = None;
    maybe_wake_display(&server.display, &mut initial_display_wake_at).await;
    let video_stream = media.acquire_video(config.clone()).await?;
    let mut audio_stream = match media.acquire_audio(audio_config).await {
        Ok(stream) => Some(stream),
        Err(_) => None,
    };
    let initial_video = current_video_state(&video_stream.state_rx)?;
    let initial_audio = audio_stream
        .as_ref()
        .and_then(|stream| stream.state_rx.borrow().clone());
    let (ws_sender, receiver) = socket.split();
    let (out_tx, out_rx) = mpsc::channel::<Message>(32);
    let (video_tx, video_rx) = watch::channel(None::<Vec<u8>>);
    let (audio_tx, audio_rx) = mpsc::channel::<Vec<u8>>(AUDIO_PACKET_QUEUE_CAPACITY);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let session_id = format!("{:x}", rand_id());
    send_hello(
        &out_tx,
        Some(session_id.as_str()),
        Some(server.display.as_str()),
        &initial_video,
        initial_audio.as_ref(),
        None,
    )
    .await?;
    if initial_audio.is_none() {
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
    let writer_task = tokio::spawn(write_socket(ws_sender, out_rx, video_rx, audio_rx));
    let stats_task = spawn_stats(out_tx.clone(), video_stream.state_rx.clone());
    let send_task = tokio::spawn(forward_frames(
        out_tx.clone(),
        video_tx,
        video_stream.rx.clone(),
        video_stream.state_rx.clone(),
        audio_stream.as_ref().map(|stream| stream.state_rx.clone()),
    ));
    let mut audio_task = audio_stream
        .as_mut()
        .and_then(|stream| stream.take_audio_rx())
        .map(|rx| tokio::spawn(forward_audio(audio_tx, rx)));
    let mut recv_task = tokio::spawn(handle_client(
        receiver,
        out_tx.clone(),
        server.clone(),
        media,
        shutdown_rx,
    ));
    let mut writer_task = writer_task;
    let mut send_task = send_task;

    let mut recv_task_completed = false;
    let mut writer_task_completed = false;
    let mut send_task_completed = false;

    tokio::select! {
        result = &mut recv_task => {
            recv_task_completed = true;
            if let Err(err) = result {
                warn!("websocket receiver task failed: {err}");
            }
        }
        result = &mut writer_task => {
            writer_task_completed = true;
            if let Err(err) = result {
                warn!("websocket writer task failed: {err}");
            }
        }
        result = &mut send_task => {
            send_task_completed = true;
            if let Err(err) = result {
                warn!("websocket sender task failed: {err}");
            }
        }
    }

    let _ = shutdown_tx.send(true);
    stats_task.abort();
    if let Some(task) = audio_task.take() {
        task.abort();
        let _ = task.await;
    }
    if !writer_task_completed && !writer_task.is_finished() {
        writer_task.abort();
    }
    if !writer_task_completed {
        let _ = writer_task.await;
    }
    if !send_task_completed && !send_task.is_finished() {
        send_task.abort();
    }
    if !send_task_completed {
        let _ = send_task.await;
    }
    if !recv_task_completed
        && !recv_task.is_finished()
        && tokio::time::timeout(Duration::from_secs(1), &mut recv_task)
        .await
        .is_err()
    {
        recv_task.abort();
        let _ = recv_task.await;
    }
    Ok(())
}

async fn forward_frames(
    sender: mpsc::Sender<Message>,
    media_sender: watch::Sender<Option<Vec<u8>>>,
    mut rx: watch::Receiver<Option<StreamFrame>>,
    mut video_state_rx: watch::Receiver<Option<ActiveVideoState>>,
    mut audio_state_rx: Option<watch::Receiver<Option<ActiveAudioState>>>,
) -> Result<()> {
    loop {
        tokio::select! {
            changed = rx.changed() => {
                if changed.is_err() {
                    break;
                }
                let Some(frame) = rx.borrow_and_update().clone() else {
                    continue;
                };
                let _ = media_sender.send(Some(binary_frame(&frame)));
                if frame.description_b64.is_some() {
                    let video_state = current_video_state(&video_state_rx)?;
                    let audio_state = current_audio_state(audio_state_rx.as_ref());
                    send_hello(&sender, None, None, &video_state, audio_state.as_ref(), frame.description_b64.clone()).await?;
                }
            }
            changed = video_state_rx.changed() => {
                if changed.is_err() {
                    break;
                }
                let Some(video_state) = video_state_rx.borrow_and_update().clone() else {
                    continue;
                };
                let audio_state = current_audio_state(audio_state_rx.as_ref());
                send_hello(&sender, None, None, &video_state, audio_state.as_ref(), None).await?;
            }
            changed = wait_for_audio_state_change(audio_state_rx.as_mut()) => {
                if changed.is_err() {
                    break;
                }
                let video_state = current_video_state(&video_state_rx)?;
                let audio_state = current_audio_state(audio_state_rx.as_ref());
                send_hello(&sender, None, None, &video_state, audio_state.as_ref(), None).await?;
            }
        }
    }
    Ok(())
}

async fn forward_audio(
    media_sender: mpsc::Sender<Vec<u8>>,
    mut rx: broadcast::Receiver<AudioFrame>,
) -> Result<()> {
    loop {
        match rx.recv().await {
            Ok(frame) => {
                if media_sender.send(binary_audio_frame(&frame)).await.is_err() {
                    break;
                }
            }
            Err(broadcast::error::RecvError::Lagged(skipped)) => {
                warn!(skipped, "audio subscriber lagged behind live stream");
            }
            Err(broadcast::error::RecvError::Closed) => break,
        }
    }
    Ok(())
}

async fn write_socket(
    mut ws_sender: futures_util::stream::SplitSink<WebSocket, Message>,
    mut control_rx: mpsc::Receiver<Message>,
    mut video_rx: watch::Receiver<Option<Vec<u8>>>,
    mut audio_rx: mpsc::Receiver<Vec<u8>>,
) {
    let mut control_closed = false;
    let mut video_closed = false;
    let mut audio_closed = false;
    let mut pending_video: Option<Vec<u8>> = None;
    let mut pending_audio: Option<Vec<u8>> = None;

    loop {
        match control_rx.try_recv() {
            Ok(message) => {
                if futures_util::SinkExt::send(&mut ws_sender, message)
                    .await
                    .is_err()
                {
                    break;
                }
                continue;
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                control_closed = true;
            }
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {}
        }

        if let Some(bytes) = pending_audio.take() {
            if futures_util::SinkExt::send(&mut ws_sender, Message::Binary(bytes.into()))
                .await
                .is_err()
            {
                break;
            }
            continue;
        }

        if let Some(bytes) = pending_video.take() {
            if futures_util::SinkExt::send(&mut ws_sender, Message::Binary(bytes.into()))
                .await
                .is_err()
            {
                break;
            }
            continue;
        }

        if control_closed && video_closed && audio_closed {
            break;
        }

        tokio::select! {
            maybe = control_rx.recv(), if !control_closed => {
                match maybe {
                    Some(message) => {
                        if futures_util::SinkExt::send(&mut ws_sender, message).await.is_err() {
                            break;
                        }
                    }
                    None => control_closed = true,
                }
            }
            changed = video_rx.changed(), if !video_closed => {
                match changed {
                    Ok(()) => {
                        pending_video = video_rx.borrow_and_update().clone();
                    }
                    Err(_) => video_closed = true,
                }
            }
            maybe = audio_rx.recv(), if !audio_closed => {
                match maybe {
                    Some(bytes) => pending_audio = Some(bytes),
                    None => audio_closed = true,
                }
            }
        }
    }
}

fn spawn_stats(
    sender: mpsc::Sender<Message>,
    video_state_rx: watch::Receiver<Option<ActiveVideoState>>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut sampler = StatsSampler::new();
        let mut tick = tokio::time::interval(Duration::from_secs(3));
        let video_state_rx = video_state_rx;
        loop {
            tick.tick().await;
            let Some(video_state) = video_state_rx.borrow().clone() else {
                continue;
            };
            let sample = sampler.sample();
            let msg = ServerMessage::Stats {
                capture_fps: video_state.config.fps as f32,
                bitrate_kbps: video_state.config.bitrate_kbps,
                queue_depth: 0,
                active_encoder: video_state.encoder.ffmpeg_encoder.clone(),
                encoder_mode: video_state.encoder.mode,
                codec: video_state.config.codec,
                cpu_usage: sample.cpu_usage,
                memory_used_mb: sample.memory_used_mb,
                memory_total_mb: sample.memory_total_mb,
                swap_used_mb: sample.swap_used_mb,
                swap_total_mb: sample.swap_total_mb,
                net_tx_kbps: sample.net_tx_kbps,
                net_rx_kbps: sample.net_rx_kbps,
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
    server: ServerConfig,
    media: MediaHub,
    mut shutdown: watch::Receiver<bool>,
) -> Result<()> {
    let mut pressed_keys = HashSet::new();
    let mut mic_input = MicInputState::Idle;
    let mut last_key_state_at = None;
    let mut last_display_wake_at = None;
    let mut key_watchdog = tokio::time::interval(KEY_STATE_WATCHDOG_INTERVAL);
    key_watchdog.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            message = receiver.next() => {
                let Some(message) = message else {
                    break;
                };
                match message? {
                    Message::Text(text) => match serde_json::from_str::<ClientMessage>(&text) {
                        Ok(client) => {
                            if should_wake_display_for_message(&client) {
                                maybe_wake_display(&server.display, &mut last_display_wake_at)
                                    .await;
                            }
                            apply_client_message(
                                &server.display,
                                &sender,
                                &media,
                                client,
                                &mut pressed_keys,
                                &mut last_key_state_at,
                            )
                            .await?
                        }
                        Err(err) => warn!("invalid client message: {err}"),
                    },
                    Message::Binary(bytes) => {
                        forward_client_binary(&server, &sender, bytes.as_ref(), &mut mic_input).await;
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            changed = shutdown.changed() => {
                if changed.is_err() || *shutdown.borrow() {
                    break;
                }
            }
            _ = key_watchdog.tick() => {
                if let Some(last_key_state) = last_key_state_at {
                    if !pressed_keys.is_empty() && last_key_state.elapsed() > KEY_STATE_TIMEOUT {
                        release_pressed_keys(&server.display, &mut pressed_keys).await?;
                        last_key_state_at = None;
                    }
                }
            }
        }
    }
    reset_input_state(&server.display, &mut pressed_keys).await?;
    shutdown_mic_input(&mut mic_input).await;
    Ok(())
}

fn should_wake_display_for_message(message: &ClientMessage) -> bool {
    matches!(
        message,
        ClientMessage::PointerMove { .. }
            | ClientMessage::PointerAbsolute { .. }
            | ClientMessage::PointerButton { .. }
            | ClientMessage::PointerWheel { .. }
            | ClientMessage::TouchTap
            | ClientMessage::Key { .. }
            | ClientMessage::KeyState { .. }
            | ClientMessage::TextInput { .. }
            | ClientMessage::Paste
            | ClientMessage::PasteClipboard { .. }
            | ClientMessage::ResetInput
    )
}

async fn maybe_wake_display(display: &str, last_wake_at: &mut Option<Instant>) {
    if let Some(last_wake_at) = last_wake_at {
        if last_wake_at.elapsed() < DISPLAY_WAKE_INTERVAL {
            return;
        }
    }
    match wake_display(display).await {
        Ok(()) => *last_wake_at = Some(Instant::now()),
        Err(err) => warn!("display wake failed: {err}"),
    }
}

async fn apply_client_message(
    display: &str,
    sender: &mpsc::Sender<Message>,
    media: &MediaHub,
    message: ClientMessage,
    pressed_keys: &mut HashSet<String>,
    last_key_state_at: &mut Option<Instant>,
) -> Result<()> {
    match message {
        ClientMessage::PointerMove { dx, dy } => {
            run_xdotool(
                display,
                [
                    "mousemove_relative",
                    "--",
                    &dx.round().to_string(),
                    &dy.round().to_string(),
                ],
            )
            .await?;
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
            *last_key_state_at = if pressed_keys.is_empty() {
                None
            } else {
                Some(Instant::now())
            };
        }
        ClientMessage::KeyState {
            pressed_keys: desired_keys,
        } => {
            sync_pressed_keys(display, pressed_keys, desired_keys).await?;
            *last_key_state_at = if pressed_keys.is_empty() {
                None
            } else {
                Some(Instant::now())
            };
        }
        ClientMessage::TextInput { text } => {
            type_remote_text(display, &text).await?;
        }
        ClientMessage::Paste => {
            reset_input_state(display, pressed_keys).await?;
            *last_key_state_at = None;
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
            *last_key_state_at = None;
            run_xdotool(display, ["key", "ctrl+v"]).await?;
        }
        ClientMessage::ResetInput => {
            reset_input_state(display, pressed_keys).await?;
            *last_key_state_at = None;
        }
        ClientMessage::UpdateStreamSettings {
            config,
            audio_config,
        } => {
            media
                .update_stream_settings(config.normalized(), audio_config.normalized())
                .await?;
        }
        ClientMessage::Ping { seq } => {
            sender
                .send(Message::Text(
                    serde_json::to_string(&ServerMessage::Pong {
                        seq,
                        server_time_ms: current_ms(),
                    })?
                    .into(),
                ))
                .await?;
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

fn key_chord_rank(key: &str) -> usize {
    match key {
        "Control_L" | "Control_R" => 0,
        "Shift_L" | "Shift_R" => 1,
        "Alt_L" | "Alt_R" => 2,
        "Super_L" | "Super_R" => 3,
        _ => 10,
    }
}

async fn sync_pressed_keys(
    display: &str,
    pressed_keys: &mut HashSet<String>,
    desired_keys: Vec<String>,
) -> Result<()> {
    let mut desired_set = HashSet::new();
    let mut ordered_desired_keys = Vec::new();
    for key in desired_keys {
        if desired_set.insert(key.clone()) {
            ordered_desired_keys.push(key);
        }
    }

    let mut keys_to_release: Vec<String> = pressed_keys
        .iter()
        .filter(|key| !desired_set.contains(*key))
        .cloned()
        .collect();
    keys_to_release.sort_by(|left, right| {
        key_chord_rank(right)
            .cmp(&key_chord_rank(left))
            .then_with(|| left.cmp(right))
    });
    for key in keys_to_release {
        run_xdotool(display, ["keyup", &key]).await?;
        pressed_keys.remove(&key);
    }

    let mut keys_to_press = ordered_desired_keys
        .iter()
        .filter(|key| !pressed_keys.contains(*key))
        .cloned()
        .collect::<Vec<_>>();
    keys_to_press.sort_by(|left, right| key_chord_rank(left).cmp(&key_chord_rank(right)));
    for key in keys_to_press {
        run_xdotool(display, ["keydown", &key]).await?;
        pressed_keys.insert(key);
    }
    Ok(())
}

async fn release_pressed_keys(display: &str, pressed_keys: &mut HashSet<String>) -> Result<()> {
    let mut keys = pressed_keys.drain().collect::<Vec<_>>();
    keys.sort_by(|left, right| {
        key_chord_rank(right)
            .cmp(&key_chord_rank(left))
            .then_with(|| left.cmp(right))
    });
    for key in keys {
        run_xdotool(display, ["keyup", &key]).await?;
    }
    Ok(())
}

async fn reset_input_state(display: &str, pressed_keys: &mut HashSet<String>) -> Result<()> {
    for button in ["1", "2", "3", "4", "5"] {
        run_xdotool(display, ["mouseup", button]).await?;
    }
    release_pressed_keys(display, pressed_keys).await?;
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

#[cfg(test)]
mod tests {
    use super::key_chord_rank;

    #[test]
    fn chord_modifiers_sort_before_regular_keys() {
        let mut keys = vec![
            "v".to_string(),
            "Shift_L".to_string(),
            "Control_L".to_string(),
        ];
        keys.sort_by(|left, right| {
            key_chord_rank(left)
                .cmp(&key_chord_rank(right))
                .then_with(|| left.cmp(right))
        });
        assert_eq!(keys, vec!["Control_L", "Shift_L", "v"]);
    }
}

async fn type_remote_text(display: &str, text: &str) -> Result<()> {
    if text.is_empty() {
        return Ok(());
    }
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let segments: Vec<&str> = normalized.split('\n').collect();
    for (index, segment) in segments.iter().enumerate() {
        if !segment.is_empty() {
            run_xdotool(
                display,
                ["type", "--delay", "0", "--clearmodifiers", "--", *segment],
            )
            .await?;
        }
        if index + 1 < segments.len() {
            run_xdotool(display, ["key", "Return"]).await?;
        }
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

fn current_video_state(rx: &watch::Receiver<Option<ActiveVideoState>>) -> Result<ActiveVideoState> {
    rx.borrow()
        .clone()
        .ok_or_else(|| anyhow::anyhow!("shared video state unavailable"))
}

fn current_audio_state(
    rx: Option<&watch::Receiver<Option<ActiveAudioState>>>,
) -> Option<ActiveAudioState> {
    rx.and_then(|rx| rx.borrow().clone())
}

async fn wait_for_audio_state_change(
    rx: Option<&mut watch::Receiver<Option<ActiveAudioState>>>,
) -> Result<(), watch::error::RecvError> {
    match rx {
        Some(rx) => rx.changed().await,
        None => std::future::pending().await,
    }
}

async fn send_hello(
    sender: &mpsc::Sender<Message>,
    session_id: Option<&str>,
    display: Option<&str>,
    video_state: &ActiveVideoState,
    audio_state: Option<&ActiveAudioState>,
    description_b64: Option<String>,
) -> Result<()> {
    sender
        .send(Message::Text(
            serde_json::to_string(&ServerMessage::Hello {
                session_id: session_id.unwrap_or_default().into(),
                server_time_ms: current_ms(),
                display: display.unwrap_or_default().into(),
                config: video_state.config.clone(),
                audio_config: audio_state.map(|state| state.config.clone()),
                active_encoder: video_state.encoder.ffmpeg_encoder.clone(),
                encoder_mode: video_state.encoder.mode,
                codec_string: video_state.config.codec.as_webcodec().into(),
                description_b64,
                audio_enabled: audio_state.is_some(),
            })?
            .into(),
        ))
        .await?;
    Ok(())
}

fn rand_id() -> u64 {
    current_ms() ^ Instant::now().elapsed().as_nanos() as u64
}

enum MicInputState {
    Idle,
    Active {
        handle: MicInputHandle,
        stream_id: u32,
    },
    Failed,
}

async fn forward_client_binary(
    server: &ServerConfig,
    sender: &mpsc::Sender<Message>,
    bytes: &[u8],
    mic_input: &mut MicInputState,
) {
    let Some((&kind, payload)) = bytes.split_first() else {
        return;
    };
    if kind != 3 || payload.len() <= MIC_STREAM_ID_BYTES {
        return;
    }
    let stream_id = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
    let audio = &payload[MIC_STREAM_ID_BYTES..];
    if audio.is_empty() {
        return;
    }

    if matches!(mic_input, MicInputState::Failed) {
        return;
    }

    if let MicInputState::Active {
        handle,
        stream_id: active_stream_id,
    } = mic_input
    {
        if *active_stream_id != stream_id {
            let _ = handle.stdin.shutdown().await;
            if let Err(err) = shutdown_mic_child(&mut handle.child).await {
                warn!("mic injector restart shutdown failed: {err}");
            }
            *mic_input = MicInputState::Idle;
        } else if let Err(err) = handle.stdin.write_all(audio).await {
            warn!("mic injector write failed: {err}");
            send_runtime_error(
                sender,
                "mic_uplink_failed",
                "Microphone uplink stopped on the server.".into(),
            )
            .await;
            if let Err(stop_err) = shutdown_mic_child(&mut handle.child).await {
                warn!("mic injector shutdown failed: {stop_err}");
            }
            *mic_input = MicInputState::Failed;
            return;
        } else {
            return;
        }
    }

    match spawn_mic_input_injector(server).await {
        Ok(mut handle) => {
            if let Err(err) = handle.stdin.write_all(audio).await {
                warn!("mic injector write failed: {err}");
                send_runtime_error(
                    sender,
                    "mic_uplink_failed",
                    "Microphone uplink stopped on the server.".into(),
                )
                .await;
                if let Err(stop_err) = shutdown_mic_child(&mut handle.child).await {
                    warn!("mic injector shutdown failed: {stop_err}");
                }
                *mic_input = MicInputState::Failed;
                return;
            }
            *mic_input = MicInputState::Active { handle, stream_id };
        }
        Err(err) => {
            warn!("mic injector startup failed: {err}");
            send_runtime_error(
                sender,
                "mic_unavailable",
                format!("Microphone uplink is unavailable on the server: {err}"),
            )
            .await;
            *mic_input = MicInputState::Failed;
        }
    }
}

async fn shutdown_mic_input(mic_input: &mut MicInputState) {
    if let MicInputState::Active { handle, .. } = mic_input {
        let _ = handle.stdin.shutdown().await;
        if let Err(err) = shutdown_mic_child(&mut handle.child).await {
            warn!("mic injector shutdown failed: {err}");
        }
    }
    *mic_input = MicInputState::Idle;
}

async fn shutdown_mic_child(child: &mut Child) -> Result<()> {
    if let Err(err) = child.kill().await {
        warn!("mic ffmpeg kill failed: {err}");
    }
    let stderr = read_stderr(child).await;
    if !stderr.trim().is_empty() {
        warn!("mic ffmpeg stderr: {}", stderr.trim());
    }
    Ok(())
}

async fn send_runtime_error(sender: &mpsc::Sender<Message>, code: &'static str, message: String) {
    let payload = match serde_json::to_string(&ServerMessage::Error { code, message }) {
        Ok(payload) => payload,
        Err(err) => {
            warn!("failed to serialize runtime error: {err}");
            return;
        }
    };
    let _ = sender.send(Message::Text(payload.into())).await;
}
