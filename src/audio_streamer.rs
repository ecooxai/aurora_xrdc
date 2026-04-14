use anyhow::Result;
use tokio::{io::AsyncReadExt, process::Child, sync::broadcast};

use crate::{
    audio::{AdtsParser, AudioFrame},
    ffmpeg::spawn_audio_capture,
    settings::{AudioStreamConfig, ServerConfig},
};

pub async fn start(
    server: &ServerConfig,
    config: &AudioStreamConfig,
    tx: broadcast::Sender<AudioFrame>,
) -> Result<Child> {
    let mut child = spawn_audio_capture(server, config).await?;
    let mut stdout = child.stdout.take().expect("ffmpeg audio stdout missing");
    let stream_tx = tx.clone();
    tokio::spawn(async move {
        let mut parser = AdtsParser::new();
        let mut buf = [0u8; 16 * 1024];
        loop {
            let read = match stdout.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => break,
            };
            for frame in parser.push(&buf[..read]) {
                let _ = stream_tx.send(frame);
            }
        }
    });
    Ok(child)
}
