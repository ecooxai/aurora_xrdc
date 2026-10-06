use std::path::Path;
use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use tokio::{fs, io::AsyncWriteExt, process::Command};

pub const CLIPBOARD_HISTORY_LIMIT: usize = 100;
const CLIPBOARD_HISTORY_PATH: &str = "/tmp/apple_xrdc_clipboard_history.json";

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

pub async fn read_remote_clipboard(_display: &str) -> Result<ClipboardPayload> {
    let text = Command::new("pbpaste").output().await.context("failed to run pbpaste")?;
    let text = if text.status.success() {
        let value = String::from_utf8(text.stdout).context("macOS clipboard text was not UTF-8")?;
        (!value.is_empty()).then_some(value)
    } else { None };

    // Text clipboard is native and reliable through pbpaste/pbcopy. PNG clipboard
    // support will use NSPasteboard in the next native-backend pass.
    Ok(ClipboardPayload { text, image_png_b64: None })
}

pub async fn write_remote_clipboard(_display: &str, payload: &ClipboardPayload) -> Result<()> {
    if let Some(text) = &payload.text {
        let mut child = Command::new("pbcopy").stdin(std::process::Stdio::piped()).spawn().context("failed to run pbcopy")?;
        child.stdin.as_mut().ok_or_else(|| anyhow!("pbcopy stdin unavailable"))?.write_all(text.as_bytes()).await?;
        let status = child.wait().await?;
        if !status.success() { anyhow::bail!("pbcopy exited with {status}"); }
        return Ok(());
    }
    if payload.image_png_b64.is_some() {
        anyhow::bail!("PNG clipboard writes are not yet supported on macOS");
    }
    Ok(())
}

pub async fn ensure_upload_dir(path: &Path) -> Result<()> {
    fs::create_dir_all(path).await.with_context(|| format!("failed to create upload dir {}", path.display()))
}

pub async fn read_clipboard_history() -> Result<Vec<ClipboardHistoryEntry>> {
    let bytes = match fs::read(CLIPBOARD_HISTORY_PATH).await {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err).context("failed to read clipboard history"),
    };
    serde_json::from_slice(&bytes).context("failed to parse clipboard history")
}

pub async fn write_clipboard_history(entries: &[ClipboardHistoryEntry]) -> Result<()> {
    let trimmed: Vec<_> = entries.iter().take(CLIPBOARD_HISTORY_LIMIT).cloned().collect();
    fs::write(CLIPBOARD_HISTORY_PATH, serde_json::to_vec(&trimmed)?).await.context("failed to write clipboard history")
}
