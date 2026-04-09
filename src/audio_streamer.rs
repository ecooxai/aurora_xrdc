use anyhow::Result;
use tokio::{io::AsyncReadExt, process::Child, sync::watch};

use crate::{
    audio::{AdtsParser, AudioFrame},
    ffmpeg::spawn_audio_capture,
    settings::ServerConfig,
};

#[derive(Debug)]
pub struct AudioStreamHandle {
    pub rx: watch::Receiver<Option<AudioFrame>>,
}

pub async fn start(server: &ServerConfig) -> Result<(AudioStreamHandle, Child)> {
    let mut child = spawn_audio_capture(server).await?;
    let mut stdout = child.stdout.take().expect("ffmpeg audio stdout missing");
    let (tx, rx) = watch::channel(None);
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
                if tx.send(Some(frame)).is_err() {
                    return;
                }
            }
        }
    });
    Ok((AudioStreamHandle { rx }, child))
}
