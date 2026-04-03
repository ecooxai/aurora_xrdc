use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Result, anyhow};
use tokio::sync::{Mutex, Notify};
use tracing::warn;

use crate::{
    clipboard::ensure_upload_dir,
    ffmpeg::{ensure_virtual_camera_device, replay_mp4_to_virtual_camera},
};

const CAMERA_STAGING_DIR: &str = ".viberdeskcamera";
const CAMERA_SESSION_STALE_AFTER: Duration = Duration::from_secs(10);

#[derive(Clone)]
pub struct CameraRelay {
    inner: Arc<CameraRelayInner>,
}

struct CameraRelayInner {
    staging_dir: PathBuf,
    state: Mutex<CameraRelayState>,
    notify: Notify,
}

struct CameraRelayState {
    queue: VecDeque<QueuedCameraChunk>,
    active_session: Option<String>,
    last_activity: Instant,
}

struct QueuedCameraChunk {
    session_id: String,
    path: PathBuf,
}

#[derive(Debug, Clone)]
pub struct CameraRelayStatus {
    pub device: String,
    pub queued_chunks: usize,
}

impl CameraRelay {
    pub fn new(upload_root: impl Into<PathBuf>) -> Self {
        let staging_dir = upload_root.into().join(CAMERA_STAGING_DIR);
        let relay = Self {
            inner: Arc::new(CameraRelayInner {
                staging_dir,
                state: Mutex::new(CameraRelayState {
                    queue: VecDeque::new(),
                    active_session: None,
                    last_activity: Instant::now(),
                }),
                notify: Notify::new(),
            }),
        };
        relay.spawn_worker();
        relay
    }

    pub async fn enqueue_mp4_chunk(
        &self,
        session_id: &str,
        seq: u64,
        bytes: Vec<u8>,
    ) -> Result<CameraRelayStatus> {
        let session_id = sanitize_session_id(session_id);
        if session_id.is_empty() {
            return Err(anyhow!("camera session_id is required"));
        }
        if bytes.is_empty() {
            return Err(anyhow!("camera chunk is empty"));
        }
        let device = ensure_virtual_camera_device().await?;
        ensure_upload_dir(&self.inner.staging_dir).await?;

        let path = self
            .inner
            .staging_dir
            .join(format!("{}_{}.mp4", session_id, seq));
        tokio::fs::write(&path, bytes).await?;

        let queued_chunks = {
            let mut state = self.inner.state.lock().await;
            if let Some(active) = &state.active_session {
                let stale = state.queue.is_empty()
                    && state.last_activity.elapsed() >= CAMERA_SESSION_STALE_AFTER;
                if active != &session_id && !stale {
                    let _ = tokio::fs::remove_file(&path).await;
                    return Err(anyhow!(
                        "camera is already in use by another session; stop that uplink first"
                    ));
                }
            }
            state.active_session = Some(session_id.clone());
            state.last_activity = Instant::now();
            state.queue.push_back(QueuedCameraChunk { session_id, path });
            state.queue.len()
        };

        self.inner.notify.notify_one();
        Ok(CameraRelayStatus {
            device,
            queued_chunks,
        })
    }

    pub async fn stop_session(&self, session_id: &str) -> Result<()> {
        let session_id = sanitize_session_id(session_id);
        if session_id.is_empty() {
            return Ok(());
        }

        let paths = {
            let mut state = self.inner.state.lock().await;
            if state.active_session.as_deref() != Some(session_id.as_str()) {
                return Ok(());
            }
            state.active_session = None;
            state.last_activity = Instant::now();

            let mut retained = VecDeque::new();
            let mut removed = Vec::new();
            while let Some(chunk) = state.queue.pop_front() {
                if chunk.session_id == session_id {
                    removed.push(chunk.path);
                } else {
                    retained.push_back(chunk);
                }
            }
            state.queue = retained;
            removed
        };

        for path in paths {
            let _ = tokio::fs::remove_file(path).await;
        }
        self.inner.notify.notify_one();
        Ok(())
    }

    fn spawn_worker(&self) {
        let inner = Arc::clone(&self.inner);
        tokio::spawn(async move {
            loop {
                let next = {
                    let mut state = inner.state.lock().await;
                    state.queue.pop_front()
                };

                let Some(chunk) = next else {
                    inner.notify.notified().await;
                    continue;
                };

                let device = match ensure_virtual_camera_device().await {
                    Ok(device) => device,
                    Err(err) => {
                        warn!("virtual camera unavailable: {err}");
                        let _ = tokio::fs::remove_file(&chunk.path).await;
                        continue;
                    }
                };

                if let Err(err) = replay_mp4_to_virtual_camera(&chunk.path, &device).await {
                    warn!("camera relay failed for {}: {err}", chunk.path.display());
                }
                let _ = tokio::fs::remove_file(&chunk.path).await;

                let mut state = inner.state.lock().await;
                if state.queue.is_empty() && state.active_session.as_deref() == Some(&chunk.session_id)
                {
                    state.active_session = None;
                    state.last_activity = Instant::now();
                }
            }
        });
    }
}

fn sanitize_session_id(session_id: &str) -> String {
    session_id
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
        .collect()
}
