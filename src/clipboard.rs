use std::{path::Path, process::Stdio};

use anyhow::{Context, Result, anyhow};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, Serialize};
use tokio::{fs, io::AsyncWriteExt, process::Command};
use tracing::warn;

pub const CLIPBOARD_HISTORY_LIMIT: usize = 100;
const CLIPBOARD_HISTORY_PATH: &str = "/tmp/vibe_rdesk_clipboard_history.json";

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClipboardPayload {
    pub text: Option<String>,
    pub image_png_b64: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClipboardHistoryEntry {
    pub side: String,
    pub payload: ClipboardPayload,
}

pub async fn read_remote_clipboard(display: &str) -> Result<ClipboardPayload> {
    let targets = xclip_output(display, &["-selection", "clipboard", "-t", "TARGETS", "-o"])
        .await
        .unwrap_or_default();
    let has_png = targets.lines().any(|line| line.trim() == "image/png");
    let image_png_b64 = if has_png {
        match xclip_output_bytes(display, &["-selection", "clipboard", "-t", "image/png", "-o"]).await {
            Ok(bytes) if !bytes.is_empty() => Some(STANDARD.encode(bytes)),
            _ => None,
        }
    } else {
        None
    };
    let text = match xclip_output(display, &["-selection", "clipboard", "-o"]).await {
        Ok(text) if !text.trim().is_empty() => Some(text),
        _ => None,
    };
    Ok(ClipboardPayload { text, image_png_b64 })
}

pub async fn write_remote_clipboard(display: &str, payload: &ClipboardPayload) -> Result<()> {
    if let Some(image_png_b64) = &payload.image_png_b64 {
        let bytes = STANDARD
            .decode(image_png_b64)
            .context("clipboard image was not valid base64")?;
        xclip_input_bytes(display, &["-selection", "clipboard", "-t", "image/png", "-i"], &bytes).await?;
        return Ok(());
    }
    let text = payload.text.clone().unwrap_or_default();
    xclip_input_bytes(display, &["-selection", "clipboard", "-i"], text.as_bytes()).await
}

pub async fn ensure_upload_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path)
        .await
        .with_context(|| format!("failed to create upload dir {}", path.display()))
}

pub async fn read_clipboard_history() -> Result<Vec<ClipboardHistoryEntry>> {
    let bytes = match fs::read(CLIPBOARD_HISTORY_PATH).await {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => {
            return Err(err)
                .with_context(|| format!("failed to read clipboard history {}", CLIPBOARD_HISTORY_PATH));
        }
    };
    serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to parse clipboard history {}", CLIPBOARD_HISTORY_PATH))
}

pub async fn write_clipboard_history(entries: &[ClipboardHistoryEntry]) -> Result<()> {
    let trimmed: Vec<ClipboardHistoryEntry> = entries
        .iter()
        .take(CLIPBOARD_HISTORY_LIMIT)
        .cloned()
        .collect();
    let bytes = serde_json::to_vec(&trimmed).context("failed to serialize clipboard history")?;
    fs::write(CLIPBOARD_HISTORY_PATH, bytes)
        .await
        .with_context(|| format!("failed to write clipboard history {}", CLIPBOARD_HISTORY_PATH))
}

async fn xclip_output(display: &str, args: &[&str]) -> Result<String> {
    let bytes = xclip_output_bytes(display, args).await?;
    String::from_utf8(bytes).context("xclip output was not utf-8")
}

async fn xclip_output_bytes(display: &str, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("xclip")
        .env("DISPLAY", display)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .context("failed to run xclip")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("xclip exited with {}: {}", output.status, stderr.trim()));
    }
    Ok(output.stdout)
}

async fn xclip_input_bytes(display: &str, args: &[&str], bytes: &[u8]) -> Result<()> {
    let mut child = Command::new("xclip")
        .env("DISPLAY", display)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .context("failed to spawn xclip")?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(bytes)
            .await
            .context("failed to write xclip stdin")?;
    }
    tokio::spawn(async move {
        match child.wait_with_output().await {
            Ok(output) if output.status.success() => {}
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!("xclip exited with {}: {}", output.status, stderr.trim());
            }
            Err(err) => {
                warn!("failed to wait for xclip: {err}");
            }
        }
    });
    Ok(())
}
