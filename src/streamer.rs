use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use tokio::{io::AsyncReadExt, process::Child, sync::watch};

use crate::{
    annexb::{AnnexBParser, EncodedFrame, IvfParser},
    ffmpeg::{EncoderChoice, choose_encoder, spawn_capture},
    settings::{CodecKind, ServerConfig, StreamConfig},
};

#[derive(Debug, Clone)]
pub struct StreamFrame {
    pub bytes: Vec<u8>,
    pub keyframe: bool,
    pub description_b64: Option<String>,
    pub sent_at_ms: u64,
}

pub async fn start(
    server: ServerConfig,
    config: StreamConfig,
    tx: watch::Sender<Option<StreamFrame>>,
) -> Result<(EncoderChoice, Child)> {
    let encoder = choose_encoder(config.codec, config.encode_preference).await?;
    let mut child = spawn_capture(&server, &config, &encoder)?;
    let mut stdout = child.stdout.take().expect("ffmpeg stdout missing");
    tokio::spawn(async move {
        let mut buf = [0u8; 64 * 1024];
        let mut h26x = AnnexBParser::new(config.codec);
        let mut ivf = IvfParser::new();
        loop {
            let read = match stdout.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => n,
                Err(_) => break,
            };
            let frames = match config.codec {
                CodecKind::Vp8 | CodecKind::Vp9 | CodecKind::Av1 => ivf.push(&buf[..read]),
                CodecKind::H264 | CodecKind::H265 => h26x.push(&buf[..read]),
            };
            for frame in frames {
                tx.send_replace(Some(pack_frame(frame)));
            }
        }
    });
    Ok((encoder, child))
}

fn pack_frame(frame: EncodedFrame) -> StreamFrame {
    StreamFrame {
        bytes: frame.data,
        keyframe: frame.keyframe,
        description_b64: frame.description_b64,
        sent_at_ms: now_ms(),
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis() as u64
}
