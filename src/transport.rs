//! Transport-agnostic message plumbing shared by the WebSocket and
//! WebTransport session backends.
//!
//! The session logic only ever speaks in terms of [`axum::extract::ws::Message`]
//! values flowing over an outbound sink and an inbound stream. By erasing the
//! concrete transport behind boxed [`Sink`]/[`Stream`] trait objects we can feed
//! the exact same session handlers from either a classic WebSocket upgrade or a
//! WebTransport (HTTP/3 over QUIC) bidirectional stream.

use std::pin::Pin;

use anyhow::Error;
use axum::extract::ws::{Message, WebSocket};
use futures_util::{Sink, SinkExt, Stream, StreamExt};

/// Outbound half of a session transport: a sink of WebSocket-style messages.
pub type WireSink = Pin<Box<dyn Sink<Message, Error = Error> + Send>>;

/// Inbound half of a session transport: a stream of WebSocket-style messages.
pub type WireStream = Pin<Box<dyn Stream<Item = Result<Message, Error>> + Send>>;

/// Splits an axum [`WebSocket`] into the transport-agnostic sink/stream pair the
/// session layer expects.
pub fn from_websocket(socket: WebSocket) -> (WireSink, WireStream) {
    let (sink, stream) = socket.split();
    let sink = sink.sink_map_err(Error::from);
    let stream = stream.map(|result| result.map_err(Error::from));
    (Box::pin(sink), Box::pin(stream))
}
