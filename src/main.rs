mod annexb;
mod app;
mod audio;
mod audio_streamer;
mod camera;
mod client_manager;
mod clipboard;
mod ffmpeg;
mod media;
mod messages;
mod rtc;
mod session;
mod settings;
mod streamer;
mod system_stats;
mod transport;
mod uinput;
mod webtransport;
mod x11_input;

use anyhow::Result;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // rustls 0.23 requires a process-level crypto provider, and our dependency
    // tree pulls in both `ring` (via webrtc's DTLS) and `aws-lc-rs` (rustls'
    // default, via axum-server/quinn), so it cannot auto-select one. Without this
    // the WebRTC DTLS handshake (and in-binary TLS) panic on a worker thread mid
    // handshake, leaving peer connections stuck "connecting". Install `ring`
    // explicitly; it backs every TLS/DTLS/QUIC path we use.
    if rustls::crypto::ring::default_provider()
        .install_default()
        .is_err()
    {
        // Already installed (e.g. by a dependency); nothing to do.
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .compact()
        .init();
    let server = settings::ServerConfig::from_args(std::env::args())?;
    app::run(server).await
}
