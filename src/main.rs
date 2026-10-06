mod annexb;
mod app;
mod audio;
mod audio_streamer;
mod camera;
mod client_manager;
#[cfg(target_os = "linux")]
mod clipboard;
#[cfg(target_os = "macos")]
#[path = "macos_clipboard.rs"]
mod clipboard;
#[cfg(target_os = "linux")]
mod ffmpeg;
#[cfg(target_os = "macos")]
#[path = "macos_ffmpeg.rs"]
mod ffmpeg;
mod media;
mod messages;
mod rtc;
mod session;
mod settings;
mod streamer;
mod system_stats;
mod transport;
#[cfg(target_os = "linux")]
mod uinput;
#[cfg(target_os = "macos")]
#[path = "macos_uinput.rs"]
mod uinput;
mod webtransport;
mod wheel;
#[cfg(target_os = "linux")]
mod x11_input;
#[cfg(target_os = "macos")]
#[path = "macos_x11_input.rs"]
mod x11_input;

use anyhow::Result;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    // One explicit ring provider backs TLS, DTLS and QUIC. The portable build
    // disables axum-server's second/default provider to avoid AWS-LC and its C build.
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
