//! WebTransport (HTTP/3 over QUIC) backend.
//!
//! This mirrors the WebSocket `/ws` endpoint: the browser opens one
//! WebTransport session per role, carrying the exact same query parameters in
//! the request path. Each session uses a single bidirectional QUIC stream whose
//! byte flow is framed into the same [`axum::extract::ws::Message`] values the
//! WebSocket path produces, so the existing session handlers are reused verbatim.

use std::{net::SocketAddr, time::Duration};

use anyhow::{Result, anyhow};
use axum::extract::ws::Message;
use base64::Engine;
use bytes::Bytes;
use futures_util::{SinkExt, stream};
use tokio::io::AsyncWriteExt;
use tokio::sync::mpsc;
use tokio_util::sync::PollSender;
use tracing::warn;
use wtransport::endpoint::endpoint_side::Server;
use wtransport::error::StreamReadExactError;
use wtransport::{Endpoint, Identity, RecvStream, SendStream, ServerConfig};

use crate::transport::{WireSink, WireStream};

/// Server-side WebTransport endpoint.
pub type WtEndpoint = Endpoint<Server>;

// Frame kinds for the length-prefixed message framing carried on the QUIC
// bidirectional stream. Layout: [u8 kind][u32 big-endian length][payload].
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

/// Outbound message channel depth before back-pressure kicks in.
const SINK_CHANNEL_CAPACITY: usize = 64;

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
        .with_identity(identity)
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

/// Wraps a WebTransport bidirectional stream into the transport-agnostic
/// sink/stream pair the session layer consumes.
pub fn wire_from_bi(mut send: SendStream, recv: RecvStream) -> (WireSink, WireStream) {
    let (tx, mut rx) = mpsc::channel::<Message>(SINK_CHANNEL_CAPACITY);
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
    });
    let sink: WireSink = Box::pin(PollSender::new(tx).sink_map_err(|err| anyhow!(err)));

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
