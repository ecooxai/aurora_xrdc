use crate::{
    clipboard::ClipboardPayload,
    settings::{CodecKind, StreamConfig},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerMessage {
    Hello {
        session_id: String,
        server_time_ms: u64,
        display: String,
        config: StreamConfig,
        active_encoder: String,
        encoder_mode: &'static str,
        codec_string: String,
        description_b64: Option<String>,
        audio_enabled: bool,
    },
    Stats {
        capture_fps: f32,
        bitrate_kbps: u32,
        queue_depth: usize,
        active_encoder: String,
        encoder_mode: &'static str,
        codec: CodecKind,
        cpu_usage: f32,
        memory_used_mb: u64,
        memory_total_mb: u64,
        swap_used_mb: u64,
        swap_total_mb: u64,
        net_tx_kbps: f32,
        net_rx_kbps: f32,
    },
    Pong {
        seq: u64,
        server_time_ms: u64,
    },
    Error {
        code: &'static str,
        message: String,
    },
    Clipboard {
        side: &'static str,
        payload: ClipboardPayload,
    },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ClientMessage {
    PointerMove { dx: f64, dy: f64 },
    PointerAbsolute { x: i32, y: i32 },
    PointerButton { button: u8, down: bool },
    PointerWheel { delta_y: i32 },
    TouchTap,
    Key { key: String, down: bool },
    KeyState { pressed_keys: Vec<String> },
    TextInput { text: String },
    Paste,
    PasteClipboard { payload: ClipboardPayload },
    ResetInput,
    Ping { seq: u64 },
    ClipboardSet { payload: ClipboardPayload },
    ClipboardGet,
}
