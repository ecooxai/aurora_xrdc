use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::Result;
use bytes::Bytes;
use tokio::{io::AsyncReadExt, process::Child, sync::broadcast};

use crate::{
    annexb::{AnnexBParser, EncodedFrame, IvfParser},
    ffmpeg::{EncoderChoice, choose_encoder, spawn_capture},
    settings::{CodecKind, ServerConfig, StreamConfig},
};

#[derive(Debug, Clone)]
pub struct StreamFrame {
    pub packet: Bytes,
    pub description_b64: Option<String>,
}

pub async fn start(
    server: ServerConfig,
    config: StreamConfig,
    tx: broadcast::Sender<StreamFrame>,
) -> Result<(EncoderChoice, Child)> {
    let encoder = choose_encoder(config.codec, config.encode_preference).await?;
    let mut child = spawn_capture(&server, &config, &encoder)?;
    let mut stdout = child.stdout.take().expect("ffmpeg stdout missing");
    tokio::spawn(async move {
        let mut buf = [0u8; 64 * 1024];
        let mut h26x = AnnexBParser::new(config.codec);
        let mut ivf = IvfParser::new(config.codec);
        let mut sequence = 0u32;
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
            for mut frame in frames {
                frame.sequence = sequence;
                sequence = sequence.wrapping_add(1);
                let _ = tx.send(pack_frame(frame));
            }
        }
    });
    Ok((encoder, child))
}

fn pack_frame(frame: EncodedFrame) -> StreamFrame {
    let sent_at_ms = now_ms();
    let mut packet = Vec::with_capacity(18 + frame.data.len());
    packet.push(1);
    packet.push(u8::from(frame.keyframe));
    packet.extend_from_slice(&sent_at_ms.to_le_bytes());
    packet.extend_from_slice(&frame.sequence.to_le_bytes());
    packet.extend_from_slice(&(frame.data.len() as u32).to_le_bytes());
    packet.extend_from_slice(&frame.data);
    StreamFrame {
        packet: Bytes::from(packet),
        description_b64: frame.description_b64,
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_millis() as u64
}
