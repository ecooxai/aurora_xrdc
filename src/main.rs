mod audio;
mod audio_streamer;
mod annexb;
mod app;
mod clipboard;
mod ffmpeg;
mod messages;
mod session;
mod settings;
mod streamer;
mod system_stats;

use anyhow::Result;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_target(false)
        .compact()
        .init();
    let server = settings::ServerConfig::from_args(std::env::args())?;
    app::run(server).await
}
