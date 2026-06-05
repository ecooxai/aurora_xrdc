use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

use anyhow::Result;
use axum::extract::ws::{CloseFrame, Message, WebSocket};
use bytes::Bytes;
use futures_util::{FutureExt, StreamExt};
use serde::Deserialize;
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
    messages::{ClientMessage, KeyModifiers, ServerMessage},
    settings::{AudioStreamConfig, ServerConfig, StreamConfig},
    streamer::StreamFrame,
    system_stats::StatsSampler,
    uinput::{UInputPointerInjector, UInputWheelInjector},
    x11_input::{X11InputInjector, screen_size},
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionRole {
    #[default]
    All,
    Control,
    Video,
    Audio,
    Mic,
}

const KEY_STATE_TIMEOUT: Duration = Duration::from_millis(500);
const KEY_STATE_WATCHDOG_INTERVAL: Duration = Duration::from_millis(100);
const MIC_STREAM_ID_BYTES: usize = std::mem::size_of::<u32>();
const VIDEO_PACKET_QUEUE_CAPACITY: usize = 64;
const AUDIO_PACKET_QUEUE_CAPACITY: usize = 128;
const DISPLAY_WAKE_INTERVAL: Duration = Duration::from_millis(350);
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum PointerMotionCommand {
    Absolute { x: i32, y: i32 },
    Relative { dx: i32, dy: i32 },
}

#[derive(Debug, Default)]
struct WheelAccumulator {
    x_steps: f64,
    y_steps: f64,
    x_last_at: Option<Instant>,
    y_last_at: Option<Instant>,
    x_last_sign: i8,
    y_last_sign: i8,
}

pub async fn handle_socket(
    socket: WebSocket,
    server: ServerConfig,
    media: MediaHub,
    config: StreamConfig,
    audio_config: AudioStreamConfig,
    role: SessionRole,
    close_rx: watch::Receiver<bool>,
) -> Result<()> {
    match role {
        SessionRole::All => {
            handle_combined_socket(socket, server, media, config, audio_config, close_rx).await
        }
        SessionRole::Control => handle_control_socket(socket, server, media, close_rx).await,
        SessionRole::Video => handle_video_socket(socket, server, media, config, close_rx).await,
        SessionRole::Audio => handle_audio_socket(socket, media, audio_config, close_rx).await,
        SessionRole::Mic => handle_mic_socket(socket, server, close_rx).await,
    }
}

async fn handle_combined_socket(
    socket: WebSocket,
    server: ServerConfig,
    media: MediaHub,
    config: StreamConfig,
    audio_config: AudioStreamConfig,
    mut close_rx: watch::Receiver<bool>,
) -> Result<()> {
    let mut initial_display_wake_at = None;
    maybe_wake_display(&server.display, &mut initial_display_wake_at).await;
    let mut video_stream = media.acquire_video(config.clone()).await?;
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
    let (video_tx, video_rx) = mpsc::channel::<Bytes>(VIDEO_PACKET_QUEUE_CAPACITY);
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
        video_stream
            .take_video_rx()
            .expect("video lease receiver already taken"),
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
        _ = wait_for_client_close(&mut close_rx) => {
            send_server_close(&out_tx).await;
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

async fn handle_control_socket(
    socket: WebSocket,
    server: ServerConfig,
    media: MediaHub,
    mut close_rx: watch::Receiver<bool>,
) -> Result<()> {
    let mut initial_display_wake_at = None;
    maybe_wake_display(&server.display, &mut initial_display_wake_at).await;
    let (ws_sender, receiver) = socket.split();
    let (out_tx, out_rx) = mpsc::channel::<Message>(32);
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let writer_task = tokio::spawn(write_control_socket(ws_sender, out_rx));
    let stats_task = spawn_stats(out_tx.clone(), media.video_state_rx());
    let state_task = tokio::spawn(forward_state_hellos(
        out_tx.clone(),
        server.display.clone(),
        media.video_state_rx(),
        media.audio_state_rx(),
    ));
    let mut recv_task = tokio::spawn(handle_client(
        receiver,
        out_tx.clone(),
        server,
        media,
        shutdown_rx,
    ));
    let mut writer_task = writer_task;
    let mut state_task = state_task;

    tokio::select! {
        result = &mut recv_task => {
            if let Err(err) = result {
                warn!("control websocket receiver task failed: {err}");
            }
        }
        result = &mut writer_task => {
            if let Err(err) = result {
                warn!("control websocket writer task failed: {err}");
            }
        }
        result = &mut state_task => {
            if let Err(err) = result {
                warn!("control websocket state task failed: {err}");
            }
        }
        _ = wait_for_client_close(&mut close_rx) => {
            send_server_close(&out_tx).await;
        }
    }

    let _ = shutdown_tx.send(true);
    stats_task.abort();
    if !recv_task.is_finished() {
        recv_task.abort();
    }
    if !writer_task.is_finished() {
        writer_task.abort();
    }
    if !state_task.is_finished() {
        state_task.abort();
    }
    Ok(())
}

async fn handle_video_socket(
    socket: WebSocket,
    server: ServerConfig,
    media: MediaHub,
    config: StreamConfig,
    close_rx: watch::Receiver<bool>,
) -> Result<()> {
    let mut video_stream = media.acquire_video(config).await?;
    let initial_video = current_video_state(&video_stream.state_rx)?;
    let audio_state_rx = media.audio_state_rx();
    let initial_audio = current_audio_state(Some(&audio_state_rx));
    let (ws_sender, receiver) = socket.split();
    let (out_tx, out_rx) = mpsc::channel::<Message>(32);
    let (video_tx, video_rx) = mpsc::channel::<Bytes>(VIDEO_PACKET_QUEUE_CAPACITY);
    let (audio_tx, audio_rx) = mpsc::channel::<Vec<u8>>(1);
    drop(audio_tx);
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
    let writer_task = tokio::spawn(write_socket(ws_sender, out_rx, video_rx, audio_rx));
    let send_task = tokio::spawn(forward_frames(
        out_tx.clone(),
        video_tx,
        video_stream
            .take_video_rx()
            .expect("video lease receiver already taken"),
        video_stream.state_rx.clone(),
        Some(audio_state_rx),
    ));
    let close_task = tokio::spawn(wait_for_socket_close(receiver));
    wait_for_role_tasks(
        "video",
        writer_task,
        send_task,
        close_task,
        out_tx,
        close_rx,
    )
    .await;
    Ok(())
}

async fn handle_audio_socket(
    socket: WebSocket,
    media: MediaHub,
    audio_config: AudioStreamConfig,
    close_rx: watch::Receiver<bool>,
) -> Result<()> {
    let (ws_sender, receiver) = socket.split();
    let (out_tx, out_rx) = mpsc::channel::<Message>(32);
    let (video_tx, video_rx) = mpsc::channel::<Bytes>(1);
    drop(video_tx);
    let (audio_tx, audio_rx) = mpsc::channel::<Vec<u8>>(AUDIO_PACKET_QUEUE_CAPACITY);
    let writer_task = tokio::spawn(write_socket(ws_sender, out_rx, video_rx, audio_rx));
    let close_task = tokio::spawn(wait_for_socket_close(receiver));
    let mut audio_stream = match media.acquire_audio(audio_config).await {
        Ok(stream) => stream,
        Err(_) => {
            let _ = out_tx
                .send(Message::Text(
                    serde_json::to_string(&ServerMessage::Error {
                        code: "audio_unavailable",
                        message: "Audio capture is unavailable. Install and run PulseAudio/PipeWire with pactl support, or set VIBE_RDESK_AUDIO_SOURCE.".into(),
                    })?
                    .into(),
                ))
                .await;
            wait_for_audio_error_socket(writer_task, close_task, out_tx, close_rx).await;
            return Ok(());
        }
    };
    let video_state = { media.video_state_rx().borrow().clone() };
    let audio_state = { audio_stream.state_rx.borrow().clone() };
    if let (Some(video_state), Some(audio_state)) = (video_state, audio_state) {
        send_hello(&out_tx, None, None, &video_state, Some(&audio_state), None).await?;
    }
    let audio_task = audio_stream
        .take_audio_rx()
        .map(|rx| tokio::spawn(forward_audio(audio_tx, rx)));
    wait_for_audio_tasks(writer_task, audio_task, close_task, out_tx, close_rx).await;
    Ok(())
}

async fn handle_mic_socket(
    socket: WebSocket,
    server: ServerConfig,
    close_rx: watch::Receiver<bool>,
) -> Result<()> {
    let (ws_sender, receiver) = socket.split();
    let (out_tx, out_rx) = mpsc::channel::<Message>(8);
    let writer_task = tokio::spawn(write_control_socket(ws_sender, out_rx));
    let recv_task = tokio::spawn(handle_mic_client(receiver, out_tx.clone(), server));
    wait_for_mic_tasks(writer_task, recv_task, out_tx, close_rx).await;
    Ok(())
}

async fn wait_for_client_close(close_rx: &mut watch::Receiver<bool>) {
    loop {
        if *close_rx.borrow() {
            break;
        }
        if close_rx.changed().await.is_err() {
            break;
        }
    }
}

async fn send_server_close(sender: &mpsc::Sender<Message>) {
    let _ = sender
        .send(Message::Close(Some(CloseFrame {
            code: 4000,
            reason: "closed_by_server".into(),
        })))
        .await;
    tokio::time::sleep(Duration::from_millis(50)).await;
}

async fn write_control_socket(
    mut ws_sender: futures_util::stream::SplitSink<WebSocket, Message>,
    mut control_rx: mpsc::Receiver<Message>,
) {
    while let Some(message) = control_rx.recv().await {
        let should_close = matches!(message, Message::Close(_));
        if futures_util::SinkExt::send(&mut ws_sender, message)
            .await
            .is_err()
        {
            break;
        }
        if should_close {
            break;
        }
    }
}

async fn wait_for_socket_close(
    mut receiver: futures_util::stream::SplitStream<WebSocket>,
) -> Result<()> {
    while let Some(message) = receiver.next().await {
        match message? {
            Message::Close(_) => break,
            _ => {}
        }
    }
    Ok(())
}

async fn forward_state_hellos(
    sender: mpsc::Sender<Message>,
    display: String,
    mut video_state_rx: watch::Receiver<Option<ActiveVideoState>>,
    mut audio_state_rx: watch::Receiver<Option<ActiveAudioState>>,
) -> Result<()> {
    let session_id = format!("{:x}", rand_id());
    loop {
        let video_state = { video_state_rx.borrow().clone() };
        if let Some(video_state) = video_state {
            let audio_state = { audio_state_rx.borrow().clone() };
            send_hello(
                &sender,
                Some(session_id.as_str()),
                Some(display.as_str()),
                &video_state,
                audio_state.as_ref(),
                None,
            )
            .await?;
            break;
        }
        video_state_rx.changed().await?;
    }

    loop {
        tokio::select! {
            changed = video_state_rx.changed() => {
                if changed.is_err() {
                    break;
                }
            }
            changed = audio_state_rx.changed() => {
                if changed.is_err() {
                    break;
                }
            }
        }
        let video_state = { video_state_rx.borrow().clone() };
        let Some(video_state) = video_state else {
            continue;
        };
        let audio_state = { audio_state_rx.borrow().clone() };
        send_hello(
            &sender,
            Some(session_id.as_str()),
            Some(display.as_str()),
            &video_state,
            audio_state.as_ref(),
            None,
        )
        .await?;
    }
    Ok(())
}

async fn handle_mic_client(
    mut receiver: futures_util::stream::SplitStream<WebSocket>,
    sender: mpsc::Sender<Message>,
    server: ServerConfig,
) -> Result<()> {
    let mut mic_input = MicInputState::Idle;
    while let Some(message) = receiver.next().await {
        match message? {
            Message::Binary(bytes) => {
                forward_client_binary(&server, &sender, bytes.as_ref(), &mut mic_input).await;
            }
            Message::Close(_) => break,
            _ => {}
        }
    }
    shutdown_mic_input(&mut mic_input).await;
    Ok(())
}

async fn wait_for_role_tasks(
    kind: &'static str,
    mut writer_task: tokio::task::JoinHandle<()>,
    mut send_task: tokio::task::JoinHandle<Result<()>>,
    mut close_task: tokio::task::JoinHandle<Result<()>>,
    sender: mpsc::Sender<Message>,
    mut close_rx: watch::Receiver<bool>,
) {
    tokio::select! {
        result = &mut writer_task => {
            if let Err(err) = result {
                warn!("{kind} websocket writer task failed: {err}");
            }
        }
        result = &mut send_task => {
            match result {
                Ok(Err(err)) => warn!("{kind} websocket sender task failed: {err}"),
                Err(err) => warn!("{kind} websocket sender task failed: {err}"),
                Ok(Ok(())) => {}
            }
        }
        result = &mut close_task => {
            match result {
                Ok(Err(err)) => warn!("{kind} websocket close task failed: {err}"),
                Err(err) => warn!("{kind} websocket close task failed: {err}"),
                Ok(Ok(())) => {}
            }
        }
        _ = wait_for_client_close(&mut close_rx) => {
            send_server_close(&sender).await;
        }
    }
    if !writer_task.is_finished() {
        writer_task.abort();
    }
    if !send_task.is_finished() {
        send_task.abort();
    }
    if !close_task.is_finished() {
        close_task.abort();
    }
}

async fn wait_for_audio_error_socket(
    mut writer_task: tokio::task::JoinHandle<()>,
    mut close_task: tokio::task::JoinHandle<Result<()>>,
    sender: mpsc::Sender<Message>,
    mut close_rx: watch::Receiver<bool>,
) {
    tokio::select! {
        result = &mut writer_task => {
            if let Err(err) = result {
                warn!("audio websocket writer task failed: {err}");
            }
        }
        result = &mut close_task => {
            match result {
                Ok(Err(err)) => warn!("audio websocket close task failed: {err}"),
                Err(err) => warn!("audio websocket close task failed: {err}"),
                Ok(Ok(())) => {}
            }
        }
        _ = wait_for_client_close(&mut close_rx) => {
            send_server_close(&sender).await;
        }
    }
    if !writer_task.is_finished() {
        writer_task.abort();
    }
    if !close_task.is_finished() {
        close_task.abort();
    }
}

async fn wait_for_audio_tasks(
    mut writer_task: tokio::task::JoinHandle<()>,
    mut audio_task: Option<tokio::task::JoinHandle<Result<()>>>,
    mut close_task: tokio::task::JoinHandle<Result<()>>,
    sender: mpsc::Sender<Message>,
    mut close_rx: watch::Receiver<bool>,
) {
    match audio_task.as_mut() {
        Some(audio_task) => {
            tokio::select! {
                result = &mut writer_task => {
                    if let Err(err) = result {
                        warn!("audio websocket writer task failed: {err}");
                    }
                }
                result = audio_task => {
                    match result {
                        Ok(Err(err)) => warn!("audio websocket sender task failed: {err}"),
                        Err(err) => warn!("audio websocket sender task failed: {err}"),
                        Ok(Ok(())) => {}
                    }
                }
                result = &mut close_task => {
                    match result {
                        Ok(Err(err)) => warn!("audio websocket close task failed: {err}"),
                        Err(err) => warn!("audio websocket close task failed: {err}"),
                        Ok(Ok(())) => {}
                    }
                }
                _ = wait_for_client_close(&mut close_rx) => {
                    send_server_close(&sender).await;
                }
            }
        }
        None => {
            wait_for_audio_error_socket(writer_task, close_task, sender, close_rx).await;
            return;
        }
    }
    if !writer_task.is_finished() {
        writer_task.abort();
    }
    if let Some(audio_task) = audio_task {
        if !audio_task.is_finished() {
            audio_task.abort();
        }
    }
    if !close_task.is_finished() {
        close_task.abort();
    }
}

async fn wait_for_mic_tasks(
    mut writer_task: tokio::task::JoinHandle<()>,
    mut recv_task: tokio::task::JoinHandle<Result<()>>,
    sender: mpsc::Sender<Message>,
    mut close_rx: watch::Receiver<bool>,
) {
    tokio::select! {
        result = &mut writer_task => {
            if let Err(err) = result {
                warn!("mic websocket writer task failed: {err}");
            }
        }
        result = &mut recv_task => {
            match result {
                Ok(Err(err)) => warn!("mic websocket receiver task failed: {err}"),
                Err(err) => warn!("mic websocket receiver task failed: {err}"),
                Ok(Ok(())) => {}
            }
        }
        _ = wait_for_client_close(&mut close_rx) => {
            send_server_close(&sender).await;
        }
    }
    if !writer_task.is_finished() {
        writer_task.abort();
    }
    if !recv_task.is_finished() {
        recv_task.abort();
    }
}

async fn forward_frames(
    sender: mpsc::Sender<Message>,
    media_sender: mpsc::Sender<Bytes>,
    mut rx: broadcast::Receiver<StreamFrame>,
    mut video_state_rx: watch::Receiver<Option<ActiveVideoState>>,
    mut audio_state_rx: Option<watch::Receiver<Option<ActiveAudioState>>>,
) -> Result<()> {
    let mut last_description_b64: Option<String> = None;
    loop {
        tokio::select! {
            frame = rx.recv() => {
                match frame {
                    Ok(frame) => {
                        if frame.description_b64.is_some() && frame.description_b64 != last_description_b64 {
                            let video_state = current_video_state(&video_state_rx)?;
                            let audio_state = current_audio_state(audio_state_rx.as_ref());
                            send_hello(&sender, None, None, &video_state, audio_state.as_ref(), frame.description_b64.clone()).await?;
                            last_description_b64 = frame.description_b64.clone();
                        }
                        if media_sender.send(frame.packet.clone()).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!(skipped, "video subscriber lagged behind live stream");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
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
    mut video_rx: mpsc::Receiver<Bytes>,
    mut audio_rx: mpsc::Receiver<Vec<u8>>,
) {
    let mut control_closed = false;
    let mut video_closed = false;
    let mut audio_closed = false;
    let mut pending_video: Option<Bytes> = None;
    let mut pending_audio: Option<Vec<u8>> = None;

    loop {
        match control_rx.try_recv() {
            Ok(message) => {
                let should_close = matches!(message, Message::Close(_));
                if futures_util::SinkExt::send(&mut ws_sender, message)
                    .await
                    .is_err()
                {
                    break;
                }
                if should_close {
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
            if futures_util::SinkExt::send(&mut ws_sender, Message::Binary(bytes))
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
            biased;
            maybe = control_rx.recv(), if !control_closed => {
                match maybe {
                    Some(message) => {
                        let should_close = matches!(message, Message::Close(_));
                        if futures_util::SinkExt::send(&mut ws_sender, message).await.is_err() {
                            break;
                        }
                        if should_close {
                            break;
                        }
                    }
                    None => control_closed = true,
                }
            }
            maybe = video_rx.recv(), if !video_closed => {
                match maybe {
                    Some(bytes) => {
                        pending_video = Some(bytes);
                    }
                    None => video_closed = true,
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
    let mut wheel_accumulator = WheelAccumulator::default();
    let input_injector = match X11InputInjector::connect(&server.display) {
        Ok(injector) => Some(injector),
        Err(err) => {
            warn!("persistent X11 input unavailable, falling back to xdotool: {err}");
            None
        }
    };
    let use_uinput = uinput_input_enabled();
    let mut pointer_motion_injector = if use_uinput {
        match UInputPointerInjector::connect() {
            Ok(injector) => Some(injector),
            Err(err) => {
                warn!("uinput pointer motion unavailable, falling back to X11/xdotool: {err}");
                None
            }
        }
    } else {
        None
    };
    let mut smooth_wheel_injector = if use_uinput {
        match UInputWheelInjector::connect() {
            Ok(injector) => Some(injector),
            Err(err) => {
                warn!("smooth uinput wheel unavailable, falling back to X11 wheel clicks: {err}");
                None
            }
        }
    } else {
        None
    };
    let mut pending_message = None;
    let mut key_watchdog = tokio::time::interval(KEY_STATE_WATCHDOG_INTERVAL);
    key_watchdog.set_missed_tick_behavior(MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            message = async {
                if let Some(message) = pending_message.take() {
                    Some(Ok(message))
                } else {
                    receiver.next().await
                }
            } => {
                let Some(message) = message else {
                    break;
                };
                match message? {
                    Message::Text(text) => match serde_json::from_str::<ClientMessage>(&text) {
                        Ok(client) => {
                            let receiver_closed = if is_pointer_motion_message(&client) {
                                let mut motions = Vec::new();
                                push_pointer_motion_command(&mut motions, &client);
                                let receiver_closed = drain_pointer_motion_messages(
                                    &mut receiver,
                                    &mut pending_message,
                                    &mut motions,
                                )?;
                                apply_pointer_motion_batch(
                                    &server.display,
                                    &mut pointer_motion_injector,
                                    input_injector.as_ref(),
                                    &motions,
                                )
                                .await?;
                                receiver_closed
                            } else {
                                if should_wake_display_for_message(&client) {
                                    maybe_wake_display(&server.display, &mut last_display_wake_at)
                                        .await;
                                }
                                apply_client_message(
                                    &server.display,
                                    &mut pointer_motion_injector,
                                    input_injector.as_ref(),
                                    smooth_wheel_injector.as_mut(),
                                    &sender,
                                    &media,
                                    client,
                                    &mut pressed_keys,
                                    &mut last_key_state_at,
                                    &mut wheel_accumulator,
                                )
                                .await?;
                                false
                            };
                            if receiver_closed {
                                break;
                            }
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
    reset_input_state(&server.display, input_injector.as_ref(), &mut pressed_keys).await?;
    shutdown_mic_input(&mut mic_input).await;
    Ok(())
}

fn should_wake_display_for_message(message: &ClientMessage) -> bool {
    matches!(
        message,
        ClientMessage::PointerButton { .. }
            | ClientMessage::PointerWheel { .. }
            | ClientMessage::TouchTap
            | ClientMessage::Paste
            | ClientMessage::PasteClipboard { .. }
            | ClientMessage::ResetInput
    )
}

fn uinput_input_enabled() -> bool {
    match std::env::var("VIBE_RDESK_INPUT_BACKEND") {
        Ok(value) => !matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "x11" | "xdotool" | "no-uinput" | "nouinput" | "0" | "false" | "no"
        ),
        Err(_) => true,
    }
}

fn is_pointer_motion_message(message: &ClientMessage) -> bool {
    matches!(
        message,
        ClientMessage::PointerMove { .. } | ClientMessage::PointerAbsolute { .. }
    )
}

fn push_pointer_motion_command(commands: &mut Vec<PointerMotionCommand>, message: &ClientMessage) {
    match message {
        ClientMessage::PointerAbsolute { x, y } => {
            commands.push(PointerMotionCommand::Absolute { x: *x, y: *y });
        }
        ClientMessage::PointerMove { dx, dy } => {
            let dx = dx.round() as i32;
            let dy = dy.round() as i32;
            if dx == 0 && dy == 0 {
                return;
            }
            match commands.last_mut() {
                Some(PointerMotionCommand::Relative {
                    dx: existing_dx,
                    dy: existing_dy,
                }) => {
                    *existing_dx += dx;
                    *existing_dy += dy;
                }
                _ => commands.push(PointerMotionCommand::Relative { dx, dy }),
            }
        }
        _ => {}
    }
}

fn drain_pointer_motion_messages(
    receiver: &mut futures_util::stream::SplitStream<WebSocket>,
    pending_message: &mut Option<Message>,
    motions: &mut Vec<PointerMotionCommand>,
) -> Result<bool> {
    loop {
        match receiver.next().now_or_never() {
            None => return Ok(false),
            Some(None) => return Ok(true),
            Some(Some(Ok(Message::Text(text)))) => {
                match serde_json::from_str::<ClientMessage>(&text) {
                    Ok(client) if is_pointer_motion_message(&client) => {
                        push_pointer_motion_command(motions, &client);
                    }
                    Ok(_) => {
                        *pending_message = Some(Message::Text(text));
                        return Ok(false);
                    }
                    Err(err) => warn!("invalid client message: {err}"),
                }
            }
            Some(Some(Ok(message))) => {
                *pending_message = Some(message);
                return Ok(false);
            }
            Some(Some(Err(err))) => return Err(err.into()),
        }
    }
}

fn wheel_clicks_for_delta(
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
    );
    let vertical_steps = accumulate_wheel_steps(
        &mut accumulator.y_steps,
        &mut accumulator.y_last_at,
        &mut accumulator.y_last_sign,
        y_steps,
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

fn smooth_wheel_units_for_delta(
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
    );
    let vertical_units = accumulate_wheel_steps(
        &mut accumulator.y_steps,
        &mut accumulator.y_last_at,
        &mut accumulator.y_last_sign,
        y_units,
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
    let whole_steps = if *remainder >= 0.0 {
        remainder.floor()
    } else {
        remainder.ceil()
    };
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
            args.push(button.to_string());
        }
    }
    run_xdotool(display, &args).await
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
    pointer_motion_injector: &mut Option<UInputPointerInjector>,
    input_injector: Option<&X11InputInjector>,
    smooth_wheel_injector: Option<&mut UInputWheelInjector>,
    sender: &mpsc::Sender<Message>,
    media: &MediaHub,
    message: ClientMessage,
    pressed_keys: &mut HashSet<String>,
    last_key_state_at: &mut Option<Instant>,
    wheel_accumulator: &mut WheelAccumulator,
) -> Result<()> {
    match message {
        ClientMessage::PointerMove { dx, dy } => {
            let dx = dx.round() as i32;
            let dy = dy.round() as i32;
            apply_relative_pointer_motion(display, pointer_motion_injector, input_injector, dx, dy)
                .await?;
        }
        ClientMessage::PointerAbsolute { x, y } => {
            if let Some(input_injector) = input_injector {
                input_injector.queue_pointer_absolute(x, y)?;
                input_injector.flush()?;
            } else {
                run_xdotool(display, ["mousemove", &x.to_string(), &y.to_string()]).await?;
            }
        }
        ClientMessage::PointerButton { button, down } => {
            if let Some(input_injector) = input_injector {
                input_injector.pointer_button(button, down)?;
            } else {
                let action = if down { "mousedown" } else { "mouseup" };
                run_xdotool(display, [action, &button.to_string()]).await?;
            }
        }
        ClientMessage::PointerWheel {
            delta_x,
            delta_y,
            delta_mode,
            scroll_speed,
        } => {
            if let Some(smooth_wheel_injector) = smooth_wheel_injector {
                let (horizontal, vertical) = smooth_wheel_units_for_delta(
                    wheel_accumulator,
                    delta_x,
                    delta_y,
                    delta_mode,
                    scroll_speed,
                );
                smooth_wheel_injector.emit_scroll(horizontal, vertical)?;
            } else {
                let clicks = wheel_clicks_for_delta(
                    wheel_accumulator,
                    delta_x,
                    delta_y,
                    delta_mode,
                    scroll_speed,
                );
                apply_wheel_clicks(display, None, &clicks).await?;
            }
        }
        ClientMessage::TouchTap => {
            if let Some(input_injector) = input_injector {
                input_injector.pointer_click(1)?;
            } else {
                run_xdotool(display, ["click", "1"]).await?;
            }
        }
        ClientMessage::Key {
            key,
            down,
            modifiers,
        } => {
            let was_pressed = pressed_keys.contains(&key);
            if let Some(modifiers) = modifiers {
                if !down
                    && !was_pressed
                    && key_logical_modifier(&key).is_none()
                    && key_modifiers_active(modifiers)
                {
                    tap_key_with_modifiers(display, pressed_keys, &key, modifiers).await?;
                    *last_key_state_at = if pressed_keys.is_empty() {
                        None
                    } else {
                        Some(Instant::now())
                    };
                    return Ok(());
                }
                sync_event_modifier_keys(display, pressed_keys, modifiers, &key).await?;
            }
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
            reset_input_state(display, input_injector, pressed_keys).await?;
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
            reset_input_state(display, input_injector, pressed_keys).await?;
            *last_key_state_at = None;
            run_xdotool(display, ["key", "ctrl+v"]).await?;
        }
        ClientMessage::ResetInput => {
            reset_input_state(display, input_injector, pressed_keys).await?;
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

async fn apply_pointer_motion_batch(
    display: &str,
    pointer_motion_injector: &mut Option<UInputPointerInjector>,
    input_injector: Option<&X11InputInjector>,
    motions: &[PointerMotionCommand],
) -> Result<()> {
    if motions.is_empty() {
        return Ok(());
    }
    if pointer_motion_injector.is_some() {
        for motion in motions {
            match motion {
                PointerMotionCommand::Absolute { x, y } => {
                    apply_absolute_pointer_motion(display, input_injector, *x, *y).await?;
                }
                PointerMotionCommand::Relative { dx, dy } => {
                    apply_relative_pointer_motion(
                        display,
                        pointer_motion_injector,
                        input_injector,
                        *dx,
                        *dy,
                    )
                    .await?;
                }
            }
        }
        return Ok(());
    }
    if let Some(input_injector) = input_injector {
        for motion in motions {
            match motion {
                PointerMotionCommand::Absolute { x, y } => {
                    input_injector.queue_pointer_absolute(*x, *y)?;
                }
                PointerMotionCommand::Relative { dx, dy } => {
                    if *dx == 0 && *dy == 0 {
                        continue;
                    }
                    input_injector.queue_pointer_relative(*dx, *dy)?;
                }
            }
        }
        return input_injector.flush();
    }
    let mut args = Vec::new();
    for motion in motions {
        match motion {
            PointerMotionCommand::Absolute { x, y } => {
                args.push("mousemove".to_string());
                args.push(x.to_string());
                args.push(y.to_string());
            }
            PointerMotionCommand::Relative { dx, dy } => {
                if *dx == 0 && *dy == 0 {
                    continue;
                }
                args.push("mousemove_relative".to_string());
                args.push("--".to_string());
                args.push(dx.to_string());
                args.push(dy.to_string());
            }
        }
    }
    if args.is_empty() {
        return Ok(());
    }
    run_xdotool(display, &args).await
}

async fn apply_relative_pointer_motion(
    display: &str,
    pointer_motion_injector: &mut Option<UInputPointerInjector>,
    input_injector: Option<&X11InputInjector>,
    dx: i32,
    dy: i32,
) -> Result<()> {
    if dx == 0 && dy == 0 {
        return Ok(());
    }
    if let Some(injector) = pointer_motion_injector.as_mut() {
        match injector.emit_motion(dx, dy) {
            Ok(()) => return Ok(()),
            Err(err) => {
                warn!("uinput pointer motion failed, falling back to X11/xdotool: {err}");
                *pointer_motion_injector = None;
            }
        }
    }
    if let Some(input_injector) = input_injector {
        input_injector.queue_pointer_relative(dx, dy)?;
        input_injector.flush()?;
    } else {
        run_xdotool(
            display,
            ["mousemove_relative", "--", &dx.to_string(), &dy.to_string()],
        )
        .await?;
    }
    Ok(())
}

async fn apply_absolute_pointer_motion(
    display: &str,
    input_injector: Option<&X11InputInjector>,
    x: i32,
    y: i32,
) -> Result<()> {
    if let Some(input_injector) = input_injector {
        input_injector.queue_pointer_absolute(x, y)?;
        input_injector.flush()?;
    } else {
        run_xdotool(display, ["mousemove", &x.to_string(), &y.to_string()]).await?;
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

fn key_logical_modifier(key: &str) -> Option<&'static str> {
    match key {
        "Control_L" | "Control_R" => Some("Control"),
        "Shift_L" | "Shift_R" => Some("Shift"),
        "Alt_L" | "Alt_R" => Some("Alt"),
        "Super_L" | "Super_R" => Some("Meta"),
        _ => None,
    }
}

fn key_modifiers_active(modifiers: KeyModifiers) -> bool {
    modifiers.ctrl || modifiers.shift || modifiers.alt || modifiers.meta
}

fn modifier_fallback_keys(modifiers: KeyModifiers) -> Vec<(&'static str, [&'static str; 2])> {
    let mut keys = Vec::new();
    if modifiers.ctrl {
        keys.push(("Control_L", ["Control_L", "Control_R"]));
    }
    if modifiers.shift {
        keys.push(("Shift_L", ["Shift_L", "Shift_R"]));
    }
    if modifiers.alt {
        keys.push(("Alt_L", ["Alt_L", "Alt_R"]));
    }
    if modifiers.meta {
        keys.push(("Super_L", ["Super_L", "Super_R"]));
    }
    keys
}

async fn tap_key_with_modifiers(
    display: &str,
    pressed_keys: &HashSet<String>,
    key: &str,
    modifiers: KeyModifiers,
) -> Result<()> {
    let modifier_keys = modifier_fallback_keys(modifiers);
    let temporary_modifiers = modifier_keys
        .iter()
        .filter_map(|(fallback, keys)| {
            if keys.iter().any(|key| pressed_keys.contains(*key)) {
                None
            } else {
                Some(*fallback)
            }
        })
        .collect::<Vec<_>>();
    let mut args = Vec::with_capacity(temporary_modifiers.len() * 4 + 4);
    for modifier in &temporary_modifiers {
        args.push("keydown".to_string());
        args.push((*modifier).to_string());
    }
    args.push("keydown".to_string());
    args.push(key.to_string());
    args.push("keyup".to_string());
    args.push(key.to_string());
    for modifier in temporary_modifiers.iter().rev() {
        args.push("keyup".to_string());
        args.push((*modifier).to_string());
    }
    run_xdotool(display, &args).await
}

async fn sync_event_modifier_keys(
    display: &str,
    pressed_keys: &mut HashSet<String>,
    modifiers: KeyModifiers,
    event_key: &str,
) -> Result<()> {
    let skip_logical = key_logical_modifier(event_key);
    let modifier_states = [
        (
            "Control",
            modifiers.ctrl,
            ["Control_L", "Control_R"],
            "Control_L",
        ),
        ("Shift", modifiers.shift, ["Shift_L", "Shift_R"], "Shift_L"),
        ("Alt", modifiers.alt, ["Alt_L", "Alt_R"], "Alt_L"),
        ("Meta", modifiers.meta, ["Super_L", "Super_R"], "Super_L"),
    ];

    for (logical, pressed, keys, fallback) in modifier_states {
        if skip_logical == Some(logical) {
            continue;
        }
        if pressed {
            if keys.iter().any(|key| pressed_keys.contains(*key)) {
                continue;
            }
            run_xdotool(display, ["keydown", fallback]).await?;
            pressed_keys.insert(fallback.to_string());
            continue;
        }
        for key in keys {
            if pressed_keys.remove(key) {
                run_xdotool(display, ["keyup", key]).await?;
            }
        }
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

async fn reset_input_state(
    display: &str,
    input_injector: Option<&X11InputInjector>,
    pressed_keys: &mut HashSet<String>,
) -> Result<()> {
    if let Some(input_injector) = input_injector {
        input_injector.release_all_buttons()?;
    } else {
        for button in ["1", "2", "3", "4", "5"] {
            run_xdotool(display, ["mouseup", button]).await?;
        }
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
    use super::{
        PointerMotionCommand, WheelAccumulator, key_chord_rank, push_pointer_motion_command,
        should_wake_display_for_message, smooth_wheel_units_for_delta, wheel_clicks_for_delta,
    };
    use crate::messages::ClientMessage;

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

    #[test]
    fn pointer_motion_does_not_trigger_display_wake() {
        assert!(!should_wake_display_for_message(
            &ClientMessage::PointerMove { dx: 5.0, dy: -3.0 }
        ));
        assert!(!should_wake_display_for_message(
            &ClientMessage::PointerAbsolute { x: 320, y: 240 }
        ));
        assert!(should_wake_display_for_message(
            &ClientMessage::PointerButton {
                button: 1,
                down: true,
            }
        ));
    }

    #[test]
    fn keyboard_input_does_not_trigger_display_wake() {
        assert!(!should_wake_display_for_message(&ClientMessage::Key {
            key: "a".to_string(),
            down: true,
            modifiers: None,
        }));
        assert!(!should_wake_display_for_message(&ClientMessage::KeyState {
            pressed_keys: vec!["Shift_L".to_string()],
        }));
        assert!(!should_wake_display_for_message(
            &ClientMessage::TextInput {
                text: "hello".to_string(),
            }
        ));
    }

    #[test]
    fn pointer_motion_batch_preserves_absolute_positions() {
        let mut motions = Vec::new();
        push_pointer_motion_command(
            &mut motions,
            &ClientMessage::PointerMove { dx: 12.0, dy: 4.0 },
        );
        push_pointer_motion_command(
            &mut motions,
            &ClientMessage::PointerAbsolute { x: 320, y: 240 },
        );
        push_pointer_motion_command(
            &mut motions,
            &ClientMessage::PointerAbsolute { x: 640, y: 480 },
        );
        assert_eq!(
            motions,
            vec![
                PointerMotionCommand::Relative { dx: 12, dy: 4 },
                PointerMotionCommand::Absolute { x: 320, y: 240 },
                PointerMotionCommand::Absolute { x: 640, y: 480 },
            ]
        );
    }

    #[test]
    fn pointer_motion_batch_sums_consecutive_relative_moves() {
        let mut motions = Vec::new();
        push_pointer_motion_command(
            &mut motions,
            &ClientMessage::PointerMove { dx: 4.4, dy: 1.2 },
        );
        push_pointer_motion_command(
            &mut motions,
            &ClientMessage::PointerMove { dx: 2.0, dy: -3.0 },
        );
        assert_eq!(
            motions,
            vec![PointerMotionCommand::Relative { dx: 6, dy: -2 }]
        );
    }

    #[test]
    fn legacy_wheel_message_uses_step_units() {
        let mut accumulator = WheelAccumulator::default();
        assert_eq!(
            wheel_clicks_for_delta(&mut accumulator, 0.0, 1.0, None, None),
            vec![(5, 1)]
        );
        assert_eq!(
            wheel_clicks_for_delta(&mut accumulator, 0.0, -1.0, None, None),
            vec![(4, 1)]
        );
    }

    #[test]
    fn pixel_wheel_deltas_accumulate_until_a_full_step() {
        let mut accumulator = WheelAccumulator::default();
        assert!(wheel_clicks_for_delta(&mut accumulator, 0.0, 12.0, Some(0), Some(1.0)).is_empty());
        assert_eq!(
            wheel_clicks_for_delta(&mut accumulator, 0.0, 12.0, Some(0), Some(1.0)),
            vec![(5, 1)]
        );
    }

    #[test]
    fn large_pixel_wheel_delta_emits_half_step_for_webclient() {
        let mut accumulator = WheelAccumulator::default();
        assert!(
            wheel_clicks_for_delta(&mut accumulator, 0.0, 100.0, Some(0), Some(1.0)).is_empty()
        );
        assert_eq!(
            wheel_clicks_for_delta(&mut accumulator, 0.0, 100.0, Some(0), Some(1.0)),
            vec![(5, 1)]
        );
    }

    #[test]
    fn smooth_wheel_deltas_use_high_resolution_units() {
        let mut accumulator = WheelAccumulator::default();
        assert_eq!(
            smooth_wheel_units_for_delta(&mut accumulator, 0.0, 0.2, Some(0), Some(1.0)),
            (0, 0)
        );
        assert_eq!(
            smooth_wheel_units_for_delta(&mut accumulator, 0.0, 0.3, Some(0), Some(1.0)),
            (0, -1)
        );

        let mut accumulator = WheelAccumulator::default();
        assert_eq!(
            smooth_wheel_units_for_delta(&mut accumulator, 0.0, 500.0, Some(0), Some(1.0)),
            (0, -240)
        );
    }

    #[test]
    fn wheel_supports_horizontal_axis() {
        let mut accumulator = WheelAccumulator::default();
        assert!(wheel_clicks_for_delta(&mut accumulator, 12.0, 0.0, Some(0), Some(1.0)).is_empty());
        assert_eq!(
            wheel_clicks_for_delta(&mut accumulator, 12.0, 0.0, Some(0), Some(1.0)),
            vec![(7, 1)]
        );
        assert!(
            wheel_clicks_for_delta(&mut accumulator, -12.0, 0.0, Some(0), Some(1.0)).is_empty()
        );
        assert_eq!(
            wheel_clicks_for_delta(&mut accumulator, -12.0, 0.0, Some(0), Some(1.0)),
            vec![(6, 1)]
        );
    }

    #[test]
    fn wheel_direction_change_drops_substep_remainder() {
        let mut accumulator = WheelAccumulator::default();
        assert!(wheel_clicks_for_delta(&mut accumulator, 0.0, 24.0, Some(0), Some(1.0)).is_empty());
        assert_eq!(
            wheel_clicks_for_delta(&mut accumulator, 0.0, 24.0, Some(0), Some(1.0)),
            vec![(5, 1)]
        );
        assert!(wheel_clicks_for_delta(&mut accumulator, 0.0, -6.0, Some(0), Some(1.0)).is_empty());
        assert!(wheel_clicks_for_delta(&mut accumulator, 0.0, -6.0, Some(0), Some(1.0)).is_empty());
        assert!(wheel_clicks_for_delta(&mut accumulator, 0.0, -6.0, Some(0), Some(1.0)).is_empty());
        assert_eq!(
            wheel_clicks_for_delta(&mut accumulator, 0.0, -6.0, Some(0), Some(1.0)),
            vec![(4, 1)]
        );
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
    let screen_size = display.and_then(|display| match screen_size(display) {
        Ok(size) => Some(size),
        Err(err) => {
            warn!("failed to query X11 screen size: {err}");
            None
        }
    });
    sender
        .send(Message::Text(
            serde_json::to_string(&ServerMessage::Hello {
                session_id: session_id.unwrap_or_default().into(),
                server_time_ms: current_ms(),
                display: display.unwrap_or_default().into(),
                screen_width: screen_size.map(|(width, _)| width),
                screen_height: screen_size.map(|(_, height)| height),
                config: video_state.config.clone(),
                audio_config: audio_state.map(|state| state.config.clone()),
                config_fallback: video_state.config_fallback,
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
