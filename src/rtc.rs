//! WebRTC backend (a third transport alongside WebSocket and WebTransport).
//!
//! Like the WebTransport backend, every WebRTC session is reduced to the same
//! transport-agnostic `(WireSink, WireStream)` pair the session layer consumes,
//! so the existing `session::handle_socket` handlers are reused verbatim. The
//! browser opens one `RTCPeerConnection` per role (control, input, video, audio,
//! mic), mirroring the per-role split the WebSocket/WebTransport clients already
//! use.
//!
//! Two modes are offered, selectable in the client's protocol menu:
//!
//! * **DataChannel mode** ([`RtcMode::DataChannel`]). Control/text rides a
//!   *reliable, ordered* `ctrl` data channel; encoded video rides a separate
//!   *unreliable, unordered* `video` data channel through the same
//!   keyframe-coalescing [`VideoQueue`] the WebTransport video stream uses, so a
//!   congested video backlog never head-of-line-blocks control traffic and the
//!   client always advances to the freshest decode point. The browser keeps its
//!   WebCodecs renderer; this is effectively WebTransport-over-SCTP/DTLS.
//!
//! * **Media mode** ([`RtcMode::Media`]). Encoded video and audio are sent as
//!   real WebRTC media *tracks* (RTP) for native/hardware decode and WebRTC's
//!   built-in congestion control; only control/input/mic ride data channels. The
//!   sink strips our framing header and writes the Annex-B / Opus payload to a
//!   `TrackLocalStaticSample`.
//!
//! Signaling is a single non-trickle HTTP round-trip (`POST /api/webrtc/offer`):
//! the browser gathers all ICE candidates into its offer, the server answers
//! with its fully-gathered description. For localhost/LAN the host candidates are
//! sufficient, so no STUN/TURN is required.

use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use anyhow::{Error, Result, anyhow};
use axum::extract::ws::Message;
use bytes::Bytes;
use futures_util::{Sink, SinkExt, stream};
use serde::Deserialize;
use tokio::io::AsyncReadExt;
use tokio::sync::{Notify, mpsc, watch};
use tokio_util::sync::PollSender;
use tracing::warn;

use webrtc::api::APIBuilder;
use webrtc::api::interceptor_registry::register_default_interceptors;
use webrtc::api::media_engine::{MIME_TYPE_H264, MIME_TYPE_OPUS, MIME_TYPE_VP8, MediaEngine};
use webrtc::api::setting_engine::SettingEngine;
use webrtc::data_channel::RTCDataChannel;
use webrtc::data_channel::data_channel_message::DataChannelMessage;
use webrtc::interceptor::registry::Registry;
use webrtc::media::Sample;
use webrtc::peer_connection::RTCPeerConnection;
use webrtc::peer_connection::configuration::RTCConfiguration;
use webrtc::peer_connection::peer_connection_state::RTCPeerConnectionState;
use webrtc::peer_connection::sdp::session_description::RTCSessionDescription;
use webrtc::rtp_transceiver::rtp_codec::RTCRtpCodecCapability;
use webrtc::track::track_local::TrackLocal;
use webrtc::track::track_local::track_local_static_sample::TrackLocalStaticSample;

use crate::{
    client_manager::{ClientManager, ClientSocketLease},
    media::MediaHub,
    session::{self, SessionRole},
    settings::{AudioStreamConfig, CodecKind, ServerConfig, StreamConfig},
    transport::{WireSink, WireStream},
    webtransport::{VideoQueue, is_video_packet},
};

/// Byte offset of the encoded payload inside a packed video frame
/// (`streamer::pack_frame`: `[tag][keyframe][u64 ts][u32 seq][u32 len]`).
const VIDEO_PAYLOAD_OFFSET: usize = 18;
/// Leading tag of a packed audio frame (`session::binary_audio_frame`).
const AUDIO_FRAME_TAG: u8 = 2;
/// Opus frames emitted by the dedicated media-mode encoder are 20 ms.
const OPUS_FRAME_DURATION: Duration = Duration::from_millis(20);

/// Outbound dispatcher depth for the reliable control channel. Video/audio are
/// offloaded to their own queue/track, so this only buffers hello/stats/close.
const CTRL_CHANNEL_CAPACITY: usize = 32;
/// Inbound channel depth for messages read off the `ctrl` data channel.
const INBOUND_CHANNEL_CAPACITY: usize = 64;
/// How long a session waits without any inbound control message before treating
/// the client as gone. The browser sends a transport keepalive every ~3s on
/// every role's control channel, so ~3 missed beats is a confident signal. This
/// is the reliable disconnect detector: webrtc-rs's ICE state does not always
/// transition to `Failed` when a browser tab vanishes (or closes a peer
/// connection on reconnect), which would otherwise leak the session and its
/// ffmpeg lease and spin CPU.
const CTRL_IDLE_TIMEOUT: Duration = Duration::from_secs(10);
/// Transport-level keepalive sent by the client on every role's control channel
/// (see `WEBRTC_KEEPALIVE` in `web/app.js`). It is consumed here (updating
/// liveness) and never forwarded to the session. JSON control messages always
/// start with `{`, so this short text marker is unambiguous.
const KEEPALIVE_SENTINEL: &str = "ka";

/// Which WebRTC variant a session uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RtcMode {
    /// Everything (including video) over data channels; client keeps WebCodecs.
    #[default]
    DataChannel,
    /// Video/audio as RTP media tracks; control over data channels.
    Media,
}

/// Builds a peer connection, wires it to a reused `session::handle_socket`, and
/// returns the SDP answer for the HTTP signaling response. The peer connection
/// and session run on a detached task that owns `lease` for its lifetime.
#[allow(clippy::too_many_arguments)]
pub async fn answer(
    offer_sdp: String,
    mode: RtcMode,
    audio_in_video: bool,
    role: SessionRole,
    server: ServerConfig,
    media: MediaHub,
    config: StreamConfig,
    audio_config: AudioStreamConfig,
    close_rx: watch::Receiver<bool>,
    clients: Arc<ClientManager>,
    lease: ClientSocketLease,
) -> Result<String> {
    let pc = Arc::new(build_peer_connection().await?);

    // Inbound: messages read off the `ctrl` data channel feed the session's
    // WireStream. Outbound control flows through `ctrl_tx`; video/audio are
    // offloaded to the coalescing queue / media tracks below.
    let (in_tx, in_rx) = mpsc::channel::<Result<Message>>(INBOUND_CHANNEL_CAPACITY);
    let (ctrl_tx, ctrl_rx) = mpsc::channel::<Message>(CTRL_CHANNEL_CAPACITY);
    let video = Arc::new(VideoQueue::new());
    let ctrl_slot = Arc::new(CtrlDcSlot::new());
    // Tracks the last inbound control message for the disconnect watchdog below.
    let last_seen = Arc::new(Mutex::new(Instant::now()));
    // Fired when the peer connection reaches a terminal state, to deterministically
    // end the session (the inbound mpsc can't be relied on for this: a `try_send`
    // of the close marker silently fails if it is momentarily full).
    let shutdown = Arc::new(Notify::new());

    // Media-mode tracks, created up front (so they appear in the SDP answer) and
    // scoped to the role that actually produces that media: the video role gets a
    // video track, the audio role an audio track. This matches the client's
    // per-role recvonly transceiver so the m-lines line up. Other roles stay
    // data-channel-only.
    let mut video_track = None;
    let mut audio_track = None;
    if mode == RtcMode::Media {
        if matches!(role, SessionRole::Video | SessionRole::All) {
            let vt = Arc::new(TrackLocalStaticSample::new(
                RTCRtpCodecCapability {
                    mime_type: media_video_mime(config.codec).to_owned(),
                    ..Default::default()
                },
                "video".to_owned(),
                "vibe-rdesk".to_owned(),
            ));
            add_track(&pc, vt.clone()).await?;
            video_track = Some(vt);
        }
        // Audio rides a native Opus RTP track only when the client opts to carry
        // it "in the video stream". WebRTC media tracks must be Opus, but the
        // shared capture is AAC (for the WebCodecs clients), so a dedicated Opus
        // encoder feeds this track. When the option is off the audio instead flows
        // as AAC over the ctrl data channel and is decoded by WebCodecs (see the
        // sink), exactly like the DataChannel transport.
        if audio_in_video && matches!(role, SessionRole::Audio | SessionRole::All) {
            let at = Arc::new(TrackLocalStaticSample::new(
                RTCRtpCodecCapability {
                    mime_type: MIME_TYPE_OPUS.to_owned(),
                    clock_rate: 48000,
                    channels: 2,
                    ..Default::default()
                },
                "audio".to_owned(),
                "vibe-rdesk".to_owned(),
            ));
            add_track(&pc, at.clone()).await?;
            spawn_opus_track_feed(at.clone(), server.clone(), audio_config.clone(), shutdown.clone());
            audio_track = Some(at);
        }
    }

    // Wire the browser-created data channels as they arrive: the unreliable
    // `video` channel drains the coalescing queue; the reliable `ctrl` channel
    // carries inbound messages and is published to the slot the outbound writer
    // awaits.
    let video_for_dc = video.clone();
    let in_tx_for_dc = in_tx.clone();
    let ctrl_slot_for_dc = ctrl_slot.clone();
    let last_seen_for_dc = last_seen.clone();
    pc.on_data_channel(Box::new(move |dc: Arc<RTCDataChannel>| {
        let label = dc.label().to_owned();
        let video = video_for_dc.clone();
        let in_tx = in_tx_for_dc.clone();
        let ctrl_slot = ctrl_slot_for_dc.clone();
        let last_seen = last_seen_for_dc.clone();
        Box::pin(async move {
            match label.as_str() {
                "video" => wire_video_channel(dc, video),
                "ctrl" => {
                    wire_ctrl_inbound(dc.clone(), in_tx, last_seen);
                    ctrl_slot.attach(dc);
                }
                _ => {}
            }
        })
    }));

    // Outbound ctrl writer: waits for the channel handle, then drains `ctrl_rx`.
    spawn_ctrl_writer(ctrl_slot, ctrl_rx);

    // Per-connection disconnect watchdog. Every role's control channel carries a
    // periodic transport keepalive, so if `last_seen` goes stale the client is
    // gone — fire the shutdown signal to end this session and release its video
    // lease. This is self-contained per peer connection and does not depend on
    // webrtc-rs's (unreliable) ICE-failure detection.
    spawn_disconnect_watchdog(last_seen.clone(), shutdown.clone());

    // Close detection: when the peer connection fails or closes, surface a
    // logical Close to the session so its loops terminate, and release the video
    // queue so its writer drains and exits.
    let video_state = video.clone();
    let shutdown_state = shutdown.clone();
    pc.on_peer_connection_state_change(Box::new(move |s: RTCPeerConnectionState| {
        let video = video_state.clone();
        let shutdown = shutdown_state.clone();
        Box::pin(async move {
            if matches!(
                s,
                RTCPeerConnectionState::Failed
                    | RTCPeerConnectionState::Disconnected
                    | RTCPeerConnectionState::Closed
            ) {
                // Wake the session's WireStream so it yields a logical Close and
                // its loops terminate; release the video queue so its writer
                // drains and exits. `notify_one` stores a permit if the stream
                // isn't awaiting at this instant, so the signal can't be missed.
                shutdown.notify_one();
                video.close();
            }
        })
    }));

    // Apply the offer and produce a fully-gathered answer.
    let offer = RTCSessionDescription::offer(offer_sdp)?;
    pc.set_remote_description(offer).await?;
    let answer = pc.create_answer(None).await?;
    let mut gather_complete = pc.gathering_complete_promise().await;
    pc.set_local_description(answer).await?;
    let _ = gather_complete.recv().await;
    let local = pc
        .local_description()
        .await
        .ok_or_else(|| anyhow!("missing local description after gathering"))?;
    let answer_sdp = local.sdp;

    // Build the transport-agnostic sink/stream and run the session on a detached
    // task that owns the peer connection and the client lease for its lifetime.
    let sink = build_sink(
        mode,
        config.codec,
        ctrl_tx,
        video,
        video_track,
        audio_track.is_some(),
        config.fps,
    );
    let stream = build_stream(in_rx, shutdown);
    let pc_for_task = pc.clone();
    tokio::spawn(async move {
        let _lease = lease;
        if let Err(err) = session::handle_socket(
            sink,
            stream,
            server,
            media,
            config,
            audio_config,
            role,
            close_rx,
            clients,
        )
        .await
        {
            warn!(error = %err, "webrtc session ended with an error");
        }
        let _ = pc_for_task.close().await;
    });

    Ok(answer_sdp)
}

/// Builds an `RTCPeerConnection`. The media engine registers the default codecs
/// (needed so media-mode H.264/VP8/Opus tracks negotiate); data-channel-only
/// sessions simply ignore them.
async fn build_peer_connection() -> Result<RTCPeerConnection> {
    let mut media_engine = MediaEngine::default();
    media_engine
        .register_default_codecs()
        .map_err(|err| anyhow!("register webrtc codecs: {err}"))?;
    let mut registry = Registry::new();
    registry = register_default_interceptors(registry, &mut media_engine)
        .map_err(|err| anyhow!("register webrtc interceptors: {err}"))?;

    // Actively detect a vanished peer. A browser that navigates away or crashes
    // never sends a clean DTLS/SCTP close, so without ICE consent freshness the
    // peer connection sits in `Connected` forever — leaking the session task, its
    // video lease (ffmpeg keeps encoding) and burning CPU. A short keepalive plus
    // tight disconnected/failed timeouts make the agent probe and transition to
    // `Failed` within a few seconds of the client disappearing, which fires our
    // state-change handler and tears the session down.
    let mut setting_engine = SettingEngine::default();
    setting_engine.set_ice_timeouts(
        Some(Duration::from_secs(5)),
        Some(Duration::from_secs(10)),
        Some(Duration::from_secs(2)),
    );

    let api = APIBuilder::new()
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .with_setting_engine(setting_engine)
        .build();
    api.new_peer_connection(RTCConfiguration::default())
        .await
        .map_err(|err| anyhow!("create peer connection: {err}"))
}

fn media_video_mime(codec: CodecKind) -> &'static str {
    match codec {
        CodecKind::Vp8 | CodecKind::Vp9 | CodecKind::Av1 => MIME_TYPE_VP8,
        CodecKind::H264 | CodecKind::H265 => MIME_TYPE_H264,
    }
}

async fn add_track(pc: &Arc<RTCPeerConnection>, track: Arc<TrackLocalStaticSample>) -> Result<()> {
    let sender = pc
        .add_track(track as Arc<dyn TrackLocal + Send + Sync>)
        .await
        .map_err(|err| anyhow!("add webrtc track: {err}"))?;
    // Drain inbound RTCP (PLI/REMB/NACK) so the interceptors run; we don't act on
    // the reports directly but the read keeps the sender's pipeline flowing.
    tokio::spawn(async move {
        let mut buf = vec![0u8; 1500];
        while sender.read(&mut buf).await.is_ok() {}
    });
    Ok(())
}

/// Maximum size of a single SCTP message we send on the video channel. Encoded
/// video keyframes routinely exceed the data channel's negotiated max message
/// size (webrtc-sctp defaults to 64 KiB), and an over-size message is silently
/// dropped — which on a static screen means the client never receives a keyframe
/// and stays blank. So each frame is split into fragments below this bound and
/// reassembled on the client. 16 KiB stays comfortably under every common limit.
const VIDEO_FRAGMENT_PAYLOAD: usize = 16 * 1024;
/// Per-fragment header: `[u32 LE frame seq][u16 LE fragment index][u16 LE count]`.
const VIDEO_FRAGMENT_HEADER: usize = 8;
/// If the data channel's send buffer already holds more than this, the next frame
/// is skipped rather than queued. The video channel is unreliable (latency-first):
/// queuing onto a backed-up channel only adds delay, and the server already
/// coalesces to the freshest keyframe, so dropping a frame and waiting for the
/// next is strictly better than stalling. Roughly one large keyframe's worth.
const VIDEO_BUFFER_SKIP_THRESHOLD: usize = 256 * 1024;

/// Wires the unreliable, unordered `video` data channel to the coalescing queue:
/// once the browser opens it, drain the freshest-keyframe queue onto it,
/// fragmenting each frame so no single SCTP message exceeds the channel's max.
///
/// Latency-first: the channel does not retransmit, and a frame is skipped
/// entirely when the send buffer is congested, so a momentary slow link can't
/// build a backlog — the client always advances toward the newest frame.
fn wire_video_channel(dc: Arc<RTCDataChannel>, video: Arc<VideoQueue>) {
    let dc_open = dc.clone();
    dc.on_open(Box::new(move || {
        let dc = dc_open.clone();
        let video = video.clone();
        Box::pin(async move {
            let mut seq: u32 = 0;
            while let Some(frame) = video.next().await {
                // Drop the frame if the channel is already backed up: sending it
                // would only add latency, and a fresher frame is coming.
                if dc.buffered_amount().await > VIDEO_BUFFER_SKIP_THRESHOLD {
                    seq = seq.wrapping_add(1);
                    continue;
                }
                if send_video_fragments(&dc, seq, &frame).await.is_err() {
                    break;
                }
                seq = seq.wrapping_add(1);
            }
        })
    }));
}

/// Splits one encoded frame into `[u32 seq][u16 idx][u16 count]`-prefixed
/// fragments and sends each as its own data channel message. The channel is
/// unreliable, so a fragment can be lost; the client reassembles by `seq` and
/// discards any frame whose fragments don't all arrive before the next one.
async fn send_video_fragments(
    dc: &Arc<RTCDataChannel>,
    seq: u32,
    frame: &Bytes,
) -> Result<(), webrtc::Error> {
    let count = frame.len().div_ceil(VIDEO_FRAGMENT_PAYLOAD).max(1);
    for idx in 0..count {
        let start = idx * VIDEO_FRAGMENT_PAYLOAD;
        let end = (start + VIDEO_FRAGMENT_PAYLOAD).min(frame.len());
        let chunk = &frame[start..end];
        let mut message = Vec::with_capacity(VIDEO_FRAGMENT_HEADER + chunk.len());
        message.extend_from_slice(&seq.to_le_bytes());
        message.extend_from_slice(&(idx as u16).to_le_bytes());
        message.extend_from_slice(&(count as u16).to_le_bytes());
        message.extend_from_slice(chunk);
        dc.send(&Bytes::from(message)).await?;
    }
    Ok(())
}

/// Wires the reliable `ctrl` data channel inbound to the session's WireStream,
/// stamping `last_seen` on every message so the disconnect watchdog can tell the
/// client is still alive.
fn wire_ctrl_inbound(
    dc: Arc<RTCDataChannel>,
    in_tx: mpsc::Sender<Result<Message>>,
    last_seen: Arc<Mutex<Instant>>,
) {
    let in_tx_close = in_tx.clone();
    dc.on_message(Box::new(move |msg: DataChannelMessage| {
        let in_tx = in_tx.clone();
        let last_seen = last_seen.clone();
        Box::pin(async move {
            // Any inbound traffic proves the client is alive.
            *last_seen.lock().expect("last_seen poisoned") = Instant::now();
            let message = if msg.is_string {
                match String::from_utf8(msg.data.to_vec()) {
                    // The transport keepalive is consumed here, not forwarded.
                    Ok(text) if text == KEEPALIVE_SENTINEL => return,
                    Ok(text) => Message::Text(text.into()),
                    Err(_) => return,
                }
            } else {
                Message::Binary(msg.data)
            };
            let _ = in_tx.send(Ok(message)).await;
        })
    }));
    dc.on_close(Box::new(move || {
        let in_tx = in_tx_close.clone();
        Box::pin(async move {
            let _ = in_tx.try_send(Ok(Message::Close(None)));
        })
    }));
}

/// Drains the outbound control queue onto the `ctrl` data channel once the
/// browser has created it. Text/binary control messages map to the SCTP
/// string/binary message kinds; a Close finishes the channel.
fn spawn_ctrl_writer(ctrl_slot: Arc<CtrlDcSlot>, mut ctrl_rx: mpsc::Receiver<Message>) {
    tokio::spawn(async move {
        let dc = ctrl_slot.get().await;
        while let Some(message) = ctrl_rx.recv().await {
            let result = match message {
                Message::Text(text) => dc.send_text(text.as_str().to_owned()).await.map(|_| ()),
                Message::Binary(bytes) => dc.send(&bytes).await.map(|_| ()),
                Message::Close(_) => {
                    let _ = dc.close().await;
                    break;
                }
                Message::Ping(_) | Message::Pong(_) => Ok(()),
            };
            if result.is_err() {
                break;
            }
        }
    });
}

/// Watches this connection's `last_seen` timestamp and fires the shutdown signal
/// when the client's keepalive stops, ending the session and releasing its video
/// lease. Exits once it has signalled.
fn spawn_disconnect_watchdog(last_seen: Arc<Mutex<Instant>>, shutdown: Arc<Notify>) {
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(Duration::from_secs(3));
        loop {
            tick.tick().await;
            let idle = last_seen.lock().expect("last_seen poisoned").elapsed();
            if idle >= CTRL_IDLE_TIMEOUT {
                shutdown.notify_one();
                break;
            }
        }
    });
}

/// A slot holding the `ctrl` data channel once the browser creates it, plus a
/// notifier so the outbound writer can await its arrival. The channel is created
/// by the offerer (browser), so the server has no handle until `on_data_channel`
/// fires.
struct CtrlDcSlot {
    dc: Mutex<Option<Arc<RTCDataChannel>>>,
    notify: Notify,
}

impl CtrlDcSlot {
    fn new() -> Self {
        Self {
            dc: Mutex::new(None),
            notify: Notify::new(),
        }
    }

    fn attach(&self, dc: Arc<RTCDataChannel>) {
        *self.dc.lock().expect("ctrl dc slot poisoned") = Some(dc);
        self.notify.notify_waiters();
    }

    async fn get(&self) -> Arc<RTCDataChannel> {
        loop {
            // Register for notification before re-checking to avoid missing an
            // `attach` that races between the check and the await.
            let notified = self.notify.notified();
            if let Some(dc) = self.dc.lock().expect("ctrl dc slot poisoned").clone() {
                return dc;
            }
            notified.await;
        }
    }
}

/// State threaded through the inbound stream: the message receiver, the terminal
/// shutdown signal, and whether the final close has already been emitted.
struct StreamState {
    rx: mpsc::Receiver<Result<Message>>,
    shutdown: Arc<Notify>,
    closed: bool,
}

/// Builds the session's inbound stream. It yields control messages from `rx`, and
/// when the peer connection reaches a terminal state (`shutdown`) it emits a
/// single logical Close and ends — guaranteeing the session loops terminate even
/// if the browser vanished without a clean close.
fn build_stream(rx: mpsc::Receiver<Result<Message>>, shutdown: Arc<Notify>) -> WireStream {
    let state = StreamState {
        rx,
        shutdown,
        closed: false,
    };
    Box::pin(stream::unfold(state, |mut state| async move {
        if state.closed {
            return None;
        }
        tokio::select! {
            item = state.rx.recv() => match item {
                Some(msg) => Some((msg, state)),
                None => {
                    state.closed = true;
                    Some((Ok(Message::Close(None)), state))
                }
            },
            _ = state.shutdown.notified() => {
                state.closed = true;
                Some((Ok(Message::Close(None)), state))
            }
        }
    }))
}

#[allow(clippy::too_many_arguments)]
fn build_sink(
    mode: RtcMode,
    codec: CodecKind,
    ctrl_tx: mpsc::Sender<Message>,
    video: Arc<VideoQueue>,
    video_track: Option<Arc<TrackLocalStaticSample>>,
    audio_in_track: bool,
    fps: u32,
) -> WireSink {
    Box::pin(RtcSink {
        mode,
        codec,
        ctrl: PollSender::new(ctrl_tx),
        video,
        video_track,
        audio_in_track,
        fps: fps.max(1),
    })
}

/// Outbound sink. In DataChannel mode, video is coalesced onto the unreliable
/// channel and everything else onto the reliable ctrl channel. In Media mode,
/// video/audio are written to RTP tracks and only control rides the ctrl channel.
struct RtcSink {
    mode: RtcMode,
    codec: CodecKind,
    ctrl: PollSender<Message>,
    video: Arc<VideoQueue>,
    video_track: Option<Arc<TrackLocalStaticSample>>,
    /// Whether a native Opus audio track exists (fed by a dedicated encoder). When
    /// true the session's AAC audio frames are dropped here; when false they are
    /// forwarded over the ctrl channel for WebCodecs decode.
    audio_in_track: bool,
    fps: u32,
}

impl Sink<Message> for RtcSink {
    type Error = Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        // Only the control channel applies back-pressure; video/audio are accepted
        // unconditionally (coalesced or written to a track) so we never stall.
        self.ctrl.poll_ready_unpin(cx).map_err(|err| anyhow!(err))
    }

    fn start_send(mut self: Pin<&mut Self>, item: Message) -> Result<()> {
        if let Message::Binary(bytes) = &item {
            match self.mode {
                RtcMode::DataChannel => {
                    if is_video_packet(bytes) {
                        self.video.push(bytes.clone());
                        return Ok(());
                    }
                }
                RtcMode::Media => {
                    if is_video_packet(bytes) {
                        if let Some(track) = self.video_track.clone() {
                            let fps = self.fps;
                            let payload =
                                media_video_payload(self.codec, bytes, VIDEO_PAYLOAD_OFFSET);
                            write_track_payload(track, payload, fps);
                        }
                        return Ok(());
                    }
                    if bytes.first() == Some(&AUDIO_FRAME_TAG) && self.audio_in_track {
                        // The native Opus track is fed by a dedicated encoder; drop
                        // the session's AAC audio frame. Without a track the frame
                        // falls through to the ctrl channel (WebCodecs) below.
                        return Ok(());
                    }
                }
            }
        }
        self.ctrl.start_send_unpin(item).map_err(|err| anyhow!(err))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        self.ctrl.poll_flush_unpin(cx).map_err(|err| anyhow!(err))
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        self.video.close();
        self.ctrl.poll_close_unpin(cx).map_err(|err| anyhow!(err))
    }
}

/// Prepares the encoded video payload for a media track. The WebRTC H.264/H.265
/// RTP packetizers expect Annex-B (start-code-delimited) NAL units, but our
/// packed frames carry the length-prefixed (AVCC) form the WebCodecs clients use,
/// so convert it back. VP8/VP9/AV1 frames are passed through unchanged.
fn media_video_payload(codec: CodecKind, packet: &Bytes, payload_offset: usize) -> Bytes {
    if packet.len() <= payload_offset {
        return Bytes::new();
    }
    let payload = packet.slice(payload_offset..);
    match codec {
        CodecKind::H264 | CodecKind::H265 => Bytes::from(avcc_to_annex_b(&payload)),
        CodecKind::Vp8 | CodecKind::Vp9 | CodecKind::Av1 => payload,
    }
}

/// Converts a length-prefixed (AVCC: `[u32 BE len][NAL]...`) access unit into
/// Annex-B (`[00 00 00 01][NAL]...`). Any SPS/PPS units present in the access
/// unit are preserved, which is what lets the browser decode mid-stream
/// keyframes. Returns the input unchanged if it doesn't parse as AVCC.
fn avcc_to_annex_b(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 16);
    let mut pos = 0;
    while pos + 4 <= data.len() {
        let len = u32::from_be_bytes([data[pos], data[pos + 1], data[pos + 2], data[pos + 3]])
            as usize;
        pos += 4;
        if len == 0 || pos + len > data.len() {
            // Not valid AVCC framing; fall back to the raw payload.
            return data.to_vec();
        }
        out.extend_from_slice(&[0, 0, 0, 1]);
        out.extend_from_slice(&data[pos..pos + len]);
        pos += len;
    }
    if pos == data.len() && !out.is_empty() {
        out
    } else {
        data.to_vec()
    }
}

/// Feeds a native Opus media track from a dedicated low-latency Opus encoder.
///
/// WebRTC media tracks require Opus, but the shared capture pipeline encodes AAC
/// (for the WebCodecs clients), so when the client opts to carry audio in the
/// video stream we run a separate `ffmpeg -c:a libopus -f opus` process, demux the
/// Ogg-Opus bitstream into raw Opus packets, and write each as a 20 ms sample —
/// sequentially, so packet order is preserved. Tied to `shutdown`, so it stops
/// (and kills ffmpeg) when the peer connection ends.
fn spawn_opus_track_feed(
    track: Arc<TrackLocalStaticSample>,
    server: ServerConfig,
    audio_config: AudioStreamConfig,
    shutdown: Arc<Notify>,
) {
    tokio::spawn(async move {
        let mut child = match crate::ffmpeg::spawn_opus_audio_capture(&server, &audio_config).await {
            Ok(child) => child,
            Err(err) => {
                warn!(error = %err, "failed to start opus audio capture for webrtc media");
                return;
            }
        };
        let Some(mut stdout) = child.stdout.take() else {
            let _ = child.kill().await;
            return;
        };
        let mut demux = OggOpusDemux::default();
        let mut buf = vec![0u8; 8192];
        loop {
            tokio::select! {
                _ = shutdown.notified() => break,
                read = stdout.read(&mut buf) => match read {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        for packet in demux.push(&buf[..n]) {
                            if track
                                .write_sample(&Sample {
                                    data: packet,
                                    duration: OPUS_FRAME_DURATION,
                                    ..Default::default()
                                })
                                .await
                                .is_err()
                            {
                                break;
                            }
                        }
                    }
                },
            }
        }
        let _ = child.kill().await;
    });
}

/// Streaming Ogg demuxer specialized for the `ffmpeg -f opus` bitstream. It yields
/// raw Opus packets, skipping the two mandatory Ogg-Opus header packets (OpusHead,
/// OpusTags). Our encoder emits small CBR 20 ms frames that never span Ogg pages,
/// so cross-page packet continuation is intentionally not reassembled.
#[derive(Default)]
struct OggOpusDemux {
    buf: Vec<u8>,
    headers_skipped: usize,
}

impl OggOpusDemux {
    fn push(&mut self, data: &[u8]) -> Vec<Bytes> {
        self.buf.extend_from_slice(data);
        let mut packets = Vec::new();
        let mut consumed = 0;
        loop {
            let rest = &self.buf[consumed..];
            // Each Ogg page starts with a fixed 27-byte header.
            if rest.len() < 27 {
                break;
            }
            if &rest[0..4] != b"OggS" {
                // Resync to the next capture pattern, or drop everything but a
                // possible partial "OggS" tail.
                match find_subslice(&rest[1..], b"OggS") {
                    Some(pos) => {
                        consumed += 1 + pos;
                        continue;
                    }
                    None => {
                        consumed = self.buf.len().saturating_sub(3);
                        break;
                    }
                }
            }
            let n_seg = rest[26] as usize;
            let header_len = 27 + n_seg;
            if rest.len() < header_len {
                break;
            }
            let seg_table = &rest[27..header_len];
            let body_len: usize = seg_table.iter().map(|&v| v as usize).sum();
            if rest.len() < header_len + body_len {
                break;
            }
            let body = &rest[header_len..header_len + body_len];
            let mut off = 0;
            let mut pkt_len = 0;
            for &lace in seg_table {
                pkt_len += lace as usize;
                if lace < 255 {
                    if self.headers_skipped < 2 {
                        self.headers_skipped += 1;
                    } else if pkt_len > 0 {
                        packets.push(Bytes::copy_from_slice(&body[off..off + pkt_len]));
                    }
                    off += pkt_len;
                    pkt_len = 0;
                }
            }
            consumed += header_len + body_len;
        }
        if consumed > 0 {
            self.buf.drain(0..consumed);
        }
        packets
    }
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Writes an already-extracted encoded payload to a media track. `write_sample`
/// is async; we spawn so the synchronous `start_send` never blocks (samples are
/// independently timestamped by the track).
fn write_track_payload(track: Arc<TrackLocalStaticSample>, payload: Bytes, rate_hz: u32) {
    if payload.is_empty() {
        return;
    }
    let duration = Duration::from_secs_f64(1.0 / rate_hz.max(1) as f64);
    tokio::spawn(async move {
        let _ = track
            .write_sample(&Sample {
                data: payload,
                duration,
                ..Default::default()
            })
            .await;
    });
}

#[cfg(test)]
mod tests {
    use super::{OggOpusDemux, avcc_to_annex_b};

    /// Builds one Ogg page carrying the given packets (with correct lacing).
    fn ogg_page(packets: &[&[u8]]) -> Vec<u8> {
        let mut seg = Vec::new();
        let mut body = Vec::new();
        for p in packets {
            let mut len = p.len();
            loop {
                if len >= 255 {
                    seg.push(255);
                    len -= 255;
                } else {
                    seg.push(len as u8);
                    break;
                }
            }
            body.extend_from_slice(p);
        }
        let mut page = Vec::new();
        page.extend_from_slice(b"OggS");
        page.extend_from_slice(&[0u8; 22]); // version, type, granule, serial, seq, crc
        page.push(seg.len() as u8);
        page.extend_from_slice(&seg);
        page.extend_from_slice(&body);
        page
    }

    #[test]
    fn ogg_opus_demux_skips_headers_and_yields_packets() {
        let mut demux = OggOpusDemux::default();
        let mut stream = ogg_page(&[b"OpusHead"]);
        stream.extend(ogg_page(&[b"OpusTags"]));
        stream.extend(ogg_page(&[b"\x01audio-one", b"\x02audio-two"]));
        // Feed split across a page boundary to exercise the streaming buffer.
        let mid = stream.len() / 2;
        let mut out = demux.push(&stream[..mid]);
        out.extend(demux.push(&stream[mid..]));
        assert_eq!(out.len(), 2);
        assert_eq!(&out[0][..], b"\x01audio-one");
        assert_eq!(&out[1][..], b"\x02audio-two");
    }

    #[test]
    fn avcc_converts_each_nal_to_start_code_prefixed() {
        // Two NALs: [len=3][aa bb cc][len=2][dd ee].
        let avcc = [0, 0, 0, 3, 0xaa, 0xbb, 0xcc, 0, 0, 0, 2, 0xdd, 0xee];
        let annexb = avcc_to_annex_b(&avcc);
        assert_eq!(
            annexb,
            vec![0, 0, 0, 1, 0xaa, 0xbb, 0xcc, 0, 0, 0, 1, 0xdd, 0xee]
        );
    }

    #[test]
    fn avcc_passthrough_on_invalid_framing() {
        // A declared length that overruns the buffer is not valid AVCC.
        let bogus = [0, 0, 0, 9, 0x01, 0x02];
        assert_eq!(avcc_to_annex_b(&bogus), bogus.to_vec());
    }
}
