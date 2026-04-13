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
    pub codec: CodecKind,
    pub description_b64: Option<String>,
    pub sent_at_ms: u64,
}

#[derive(Debug)]
pub struct StreamHandle {
    pub rx: watch::Receiver<Option<StreamFrame>>,
    pub encoder: EncoderChoice,
}

pub async fn start(server: ServerConfig, config: StreamConfig) -> Result<(StreamHandle, Child)> {
    let encoder = choose_encoder(config.codec).await?;
    let mut child = spawn_capture(&server, &config, &encoder)?;
    let mut stdout = child.stdout.take().expect("ffmpeg stdout missing");
    let (tx, rx) = watch::channel(None);
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
                CodecKind::Vp8 => ivf.push(&buf[..read]),
                CodecKind::H264 | CodecKind::H265 => h26x.push(&buf[..read]),
            };
            for frame in frames {
                if tx.send(Some(pack_frame(config.codec, frame))).is_err() {
                    return;
                }
            }
        }
    });
    Ok((StreamHandle { rx, encoder }, child))
}

fn pack_frame(codec: CodecKind, frame: EncodedFrame) -> StreamFrame {
    StreamFrame {
        bytes: frame.data,
        keyframe: frame.keyframe,
        codec,
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
