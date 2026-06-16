//! WebTransport (HTTP/3 over QUIC) backend.
//!
//! This mirrors the WebSocket `/ws` endpoint: the browser opens one
//! WebTransport session per role, carrying the exact same query parameters in
//! the request path.
//!
//! Control traffic (the JSON hello/stats/pong messages and inbound pointer/key
//! events) rides a single bidirectional QUIC stream, framed into the same
//! [`axum::extract::ws::Message`] values the WebSocket path produces, so the
//! existing session handlers are reused verbatim.
//!
//! Video, however, does **not** share that bidirectional stream. Encoded video
//! frames are pushed onto a dedicated *unidirectional* stream through a
//! coalescing queue ([`VideoQueue`]). Two properties of that queue are what make
//! WebTransport video low-latency:
//!
//! * **Always the freshest decode point.** Whenever a new keyframe is enqueued,
//!   every frame queued before it is dropped — those frames are superseded by a
//!   newer independent decode point, so forwarding them would only add latency.
//!   Frames *between* keyframes are kept in order so the delta chain stays
//!   decodable. The client therefore always advances toward the newest frame the
//!   network can carry instead of grinding through a stale backlog.
//! * **No head-of-line blocking against control.** Because video lives on its
//!   own stream, a congested video backlog never stalls the hello/stats/input
//!   messages on the bidirectional stream (and vice versa).

use std::collections::VecDeque;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::{net::SocketAddr, time::Duration};

use anyhow::{Error, Result, anyhow};
use axum::extract::ws::Message;
use base64::Engine;
use bytes::Bytes;
use futures_util::{Sink, SinkExt, stream};
use tokio::io::AsyncWriteExt;
use tokio::sync::{Notify, mpsc};
use tokio_util::sync::PollSender;
use tracing::warn;
use wtransport::config::QuicTransportConfig;
use wtransport::endpoint::endpoint_side::Server;
use wtransport::error::StreamReadExactError;
use wtransport::quinn::VarInt;
use wtransport::quinn::congestion::BbrConfig;
use wtransport::{Connection, Endpoint, Identity, RecvStream, SendStream, ServerConfig};

use crate::transport::{WireSink, WireStream};

/// Server-side WebTransport endpoint.
pub type WtEndpoint = Endpoint<Server>;

// Frame kinds for the length-prefixed message framing carried on the QUIC
// streams. Layout: [u8 kind][u32 big-endian length][payload].
const FRAME_BINARY: u8 = 0;
const FRAME_TEXT: u8 = 1;
const FRAME_CLOSE: u8 = 2;
// A no-op frame. Browsers do not make a freshly opened bidirectional stream
// visible to the server (our `accept_bi`) until the client writes at least one
// byte to it. The client sends a noop immediately after opening so that
// read-only streams (video/audio) start flowing without waiting for input.
const FRAME_NOOP: u8 = 3;

/// Upper bound on a single framed message; matches the WebSocket limit.
const MAX_FRAME_LEN: u32 = 64 * 1024 * 1024;

/// Leading byte of an encoded video packet (see `streamer::pack_frame`). Audio
/// packets use tag `2`; everything else is control JSON.
const VIDEO_FRAME_TAG: u8 = 1;
/// Byte offset of the keyframe flag within a video packet.
const VIDEO_KEYFRAME_OFFSET: usize = 1;

/// Outbound dispatcher channel depth for the bidirectional (control) stream.
/// Video never flows through this channel — it is offloaded to [`VideoQueue`]
/// instantly — so this only ever buffers the rare hello/stats/close messages.
const DISPATCH_CHANNEL_CAPACITY: usize = 16;

/// Hard cap on frames retained in the video queue. The keyframe-coalescing logic
/// already bounds the backlog to a single group-of-pictures; this is only a
/// safety valve in case keyframes dry up, preventing unbounded memory growth.
const MAX_QUEUED_VIDEO_FRAMES: usize = 300;

/// A ready-to-serve WebTransport endpoint plus the metadata browsers need to
/// connect to it with a self-signed certificate.
pub struct WtSetup {
    pub endpoint: WtEndpoint,
    /// Base64-encoded SHA-256 digests of the DER certificates, for the browser's
    /// `serverCertificateHashes` option.
    pub cert_hashes: Vec<String>,
    pub local_addr: SocketAddr,
}

/// Builds a WebTransport endpoint bound to `addr` with a fresh self-signed
/// ECDSA P-256 certificate (14-day validity) suitable for browser
/// `serverCertificateHashes`.
pub fn setup(addr: SocketAddr) -> Result<WtSetup> {
    let identity = Identity::self_signed(["localhost", "127.0.0.1", "::1"])
        .map_err(|err| anyhow!("failed to build WebTransport identity: {err}"))?;

    let cert_hashes = identity
        .certificate_chain()
        .as_ref()
        .iter()
        .map(|certificate| {
            let digest = certificate.hash();
            base64::engine::general_purpose::STANDARD.encode(digest.as_ref())
        })
        .collect();

    let config = ServerConfig::builder()
        .with_bind_address(addr)
        .with_custom_transport(identity, tuned_transport_config())
        .keep_alive_interval(Some(Duration::from_secs(3)))
        .max_idle_timeout(Some(Duration::from_secs(30)))
        .map_err(|err| anyhow!("invalid WebTransport idle timeout: {err}"))?
        .build();

    let endpoint = Endpoint::server(config)?;
    let local_addr = endpoint.local_addr()?;
    Ok(WtSetup {
        endpoint,
        cert_hashes,
        local_addr,
    })
}

/// QUIC transport tuning aimed at low-latency interactive video.
///
/// The defaults are tuned for bulk file transfer; for a live screen stream we
/// care about keeping the pipe full and reacting quickly:
///
/// * **BBR congestion control.** The default (CUBIC) repeatedly halves its
///   window on any packet loss, which on a lossy/jittery Wi-Fi or remote link
///   produces exactly the stuttering the user sees — and is the main reason
///   plain TCP/WebSocket can feel smoother there. BBR paces to the measured
///   bottleneck bandwidth instead, so throughput (and thus frame freshness)
///   stays stable under loss. On a clean link the two behave the same.
/// * **Large send window.** Stops the server from self-throttling the video
///   stream on higher bandwidth-delay-product links; the congestion controller
///   still bounds what is actually in flight.
/// * **Lower initial RTT estimate.** The 333 ms default makes loss recovery and
///   MTU probing sluggish on the low-latency links a remote desktop targets.
fn tuned_transport_config() -> QuicTransportConfig {
    let mut transport = QuicTransportConfig::default();
    transport.congestion_controller_factory(Arc::new(BbrConfig::default()));
    transport.send_window(32 * 1024 * 1024);
    transport.stream_receive_window(VarInt::from_u32(8 * 1024 * 1024));
    transport.receive_window(VarInt::from_u32(32 * 1024 * 1024));
    transport.initial_rtt(Duration::from_millis(80));
    transport
}

/// Wraps a WebTransport session into the transport-agnostic sink/stream pair the
/// session layer consumes.
///
/// `send`/`recv` are the bidirectional control stream; `connection` is retained
/// so the video writer can open its dedicated unidirectional stream on demand.
pub fn wire_from_bi(
    connection: Connection,
    mut send: SendStream,
    recv: RecvStream,
) -> (WireSink, WireStream) {
    let video = Arc::new(VideoQueue::new());

    // Video rides its own unidirectional stream, coalesced to the freshest
    // decodable frame, so it never head-of-line-blocks control traffic below.
    spawn_video_writer(connection, video.clone());

    let (tx, mut rx) = mpsc::channel::<Message>(DISPATCH_CHANNEL_CAPACITY);
    {
        let video = video.clone();
        tokio::spawn(async move {
            while let Some(message) = rx.recv().await {
                let closing = matches!(message, Message::Close(_));
                if let Err(err) = write_frame(&mut send, &message).await {
                    warn!("webtransport frame write error: {err}");
                    break;
                }
                if closing {
                    break;
                }
            }
            let _ = send.finish().await;
            // The control stream is gone (session ended); release the video
            // writer so it drains, finishes its stream and exits.
            video.close();
        });
    }

    let sink: WireSink = Box::pin(WtSink {
        video,
        bidi: PollSender::new(tx),
    });

    let stream: WireStream = Box::pin(stream::unfold(RxState::Active(recv), |state| async move {
        match state {
            RxState::Active(mut recv) => match read_frame(&mut recv).await {
                Ok(Some(message)) => Some((Ok(message), RxState::Active(recv))),
                // Stream finished cleanly: surface a logical close so the session
                // loops terminate just like a WebSocket Close frame.
                Ok(None) => Some((Ok(Message::Close(None)), RxState::Done)),
                Err(err) => {
                    warn!("webtransport frame read error: {err}");
                    Some((Ok(Message::Close(None)), RxState::Done))
                }
            },
            RxState::Done => None,
        }
    }));

    (sink, stream)
}

enum RxState {
    Active(RecvStream),
    Done,
}

/// Outbound sink that routes encoded video onto the coalescing [`VideoQueue`]
/// (its dedicated unidirectional stream) and everything else onto the
/// bidirectional control stream.
struct WtSink {
    video: Arc<VideoQueue>,
    bidi: PollSender<Message>,
}

impl Sink<Message> for WtSink {
    type Error = Error;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        // Only the control channel can apply back-pressure; video is accepted
        // unconditionally and coalesced, so the upstream never stalls on it.
        self.bidi.poll_ready_unpin(cx).map_err(|err| anyhow!(err))
    }

    fn start_send(mut self: Pin<&mut Self>, item: Message) -> Result<()> {
        if let Message::Binary(bytes) = &item {
            if is_video_packet(bytes) {
                self.video.push(bytes.clone());
                return Ok(());
            }
        }
        self.bidi.start_send_unpin(item).map_err(|err| anyhow!(err))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        self.bidi.poll_flush_unpin(cx).map_err(|err| anyhow!(err))
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<()>> {
        self.video.close();
        self.bidi.poll_close_unpin(cx).map_err(|err| anyhow!(err))
    }
}

fn is_video_packet(bytes: &Bytes) -> bool {
    bytes.first() == Some(&VIDEO_FRAME_TAG)
}

fn is_video_keyframe(frame: &Bytes) -> bool {
    frame.len() > VIDEO_KEYFRAME_OFFSET && frame[VIDEO_KEYFRAME_OFFSET] == 1
}

/// A coalescing queue of encoded video frames awaiting transmission on the
/// dedicated unidirectional stream.
struct VideoQueue {
    inner: Mutex<VideoQueueInner>,
    notify: Notify,
}

struct VideoQueueInner {
    frames: VecDeque<Bytes>,
    closed: bool,
}

impl VideoQueue {
    fn new() -> Self {
        Self {
            inner: Mutex::new(VideoQueueInner {
                frames: VecDeque::new(),
                closed: false,
            }),
            notify: Notify::new(),
        }
    }

    /// Enqueues a frame, then drops any now-superseded backlog so the consumer
    /// always pulls the freshest decodable sequence.
    fn push(&self, frame: Bytes) {
        {
            let mut inner = self.inner.lock().expect("video queue poisoned");
            if inner.closed {
                return;
            }
            inner.frames.push_back(frame);
            coalesce_to_latest_keyframe(&mut inner.frames);
            // Safety valve: should keyframes ever stop arriving, bound memory by
            // dropping the oldest frames (the client will resync on the next
            // keyframe).
            while inner.frames.len() > MAX_QUEUED_VIDEO_FRAMES {
                inner.frames.pop_front();
            }
        }
        self.notify.notify_one();
    }

    fn close(&self) {
        {
            let mut inner = self.inner.lock().expect("video queue poisoned");
            inner.closed = true;
        }
        self.notify.notify_one();
    }

    /// Waits for and returns the next frame to send, or `None` once the queue is
    /// closed and drained.
    async fn next(&self) -> Option<Bytes> {
        loop {
            {
                let mut inner = self.inner.lock().expect("video queue poisoned");
                if let Some(frame) = inner.frames.pop_front() {
                    return Some(frame);
                }
                if inner.closed {
                    return None;
                }
            }
            self.notify.notified().await;
        }
    }
}

/// Drops every frame preceding the most recent keyframe in the queue. Those
/// frames belong to an older group-of-pictures that the newer keyframe makes
/// obsolete, so discarding them lets the client jump straight to the fresh
/// decode point instead of replaying stale history.
fn coalesce_to_latest_keyframe(frames: &mut VecDeque<Bytes>) {
    let last_keyframe = frames
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, frame)| is_video_keyframe(frame).then_some(index));
    if let Some(index) = last_keyframe {
        for _ in 0..index {
            frames.pop_front();
        }
    }
}

/// Drains [`VideoQueue`] onto a dedicated unidirectional QUIC stream, opening it
/// lazily on the first frame.
fn spawn_video_writer(connection: Connection, queue: Arc<VideoQueue>) {
    tokio::spawn(async move {
        let mut send: Option<SendStream> = None;
        while let Some(frame) = queue.next().await {
            if send.is_none() {
                match open_video_stream(&connection).await {
                    Ok(stream) => send = Some(stream),
                    Err(err) => {
                        warn!("webtransport video stream open error: {err}");
                        break;
                    }
                }
            }
            let stream = send.as_mut().expect("video stream initialized above");
            if let Err(err) = write_payload(stream, FRAME_BINARY, frame.as_ref()).await {
                warn!("webtransport video frame write error: {err}");
                break;
            }
        }
        if let Some(mut stream) = send {
            let _ = stream.finish().await;
        }
    });
}

async fn open_video_stream(connection: &Connection) -> Result<SendStream> {
    let opening = connection
        .open_uni()
        .await
        .map_err(|err| anyhow!("open video stream: {err}"))?;
    let stream = opening
        .await
        .map_err(|err| anyhow!("initialize video stream: {err}"))?;
    Ok(stream)
}

async fn write_frame(send: &mut SendStream, message: &Message) -> Result<()> {
    match message {
        Message::Binary(bytes) => write_payload(send, FRAME_BINARY, bytes.as_ref()).await,
        Message::Text(text) => write_payload(send, FRAME_TEXT, text.as_str().as_bytes()).await,
        Message::Close(_) => write_payload(send, FRAME_CLOSE, &[]).await,
        // WebSocket ping/pong have no WebTransport analogue (QUIC keep-alive
        // handles liveness); silently drop them.
        Message::Ping(_) | Message::Pong(_) => Ok(()),
    }
}

async fn write_payload(send: &mut SendStream, kind: u8, payload: &[u8]) -> Result<()> {
    let len = u32::try_from(payload.len()).map_err(|_| anyhow!("frame payload too large"))?;
    let mut header = [0u8; 5];
    header[0] = kind;
    header[1..].copy_from_slice(&len.to_be_bytes());
    send.write_all(&header).await?;
    if !payload.is_empty() {
        send.write_all(payload).await?;
    }
    send.flush().await?;
    Ok(())
}

async fn read_frame(recv: &mut RecvStream) -> Result<Option<Message>> {
    loop {
        let mut header = [0u8; 5];
        match recv.read_exact(&mut header).await {
            Ok(()) => {}
            // The peer finished the stream at a frame boundary: a clean shutdown.
            Err(StreamReadExactError::FinishedEarly(_)) => return Ok(None),
            Err(err) => return Err(err.into()),
        }
        let kind = header[0];
        let len = u32::from_be_bytes([header[1], header[2], header[3], header[4]]);
        if len > MAX_FRAME_LEN {
            return Err(anyhow!("webtransport frame too large: {len} bytes"));
        }
        let mut payload = vec![0u8; len as usize];
        if len > 0 {
            recv.read_exact(&mut payload).await?;
        }
        let message = match kind {
            FRAME_BINARY => Message::Binary(Bytes::from(payload)),
            FRAME_TEXT => Message::Text(String::from_utf8(payload)?.into()),
            FRAME_CLOSE => Message::Close(None),
            // Stream-activation/keepalive frames carry no message; keep reading.
            FRAME_NOOP => continue,
            other => return Err(anyhow!("unknown webtransport frame kind {other}")),
        };
        return Ok(Some(message));
    }
}
