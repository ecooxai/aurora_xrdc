use std::{
    collections::VecDeque,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Result, anyhow};
use tokio::{
    io::AsyncWriteExt,
    sync::{Mutex, Notify},
    time::sleep,
};
use tracing::warn;

use crate::ffmpeg::{
    VirtualCameraPlaceholderHandle, VirtualCameraRelayHandle, ensure_virtual_camera_device,
    refresh_virtual_camera_desktop_services, spawn_virtual_camera_placeholder,
    spawn_virtual_camera_relay,
};

const CAMERA_SESSION_STALE_AFTER: Duration = Duration::from_secs(10);
const CAMERA_PLACEHOLDER_RETRY_AFTER: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub struct CameraRelay {
    inner: Arc<CameraRelayInner>,
}

struct CameraRelayInner {
    state: Mutex<CameraRelayState>,
    notify: Notify,
}

struct CameraRelayState {
    queue: VecDeque<CameraRelayCommand>,
    active_session: Option<String>,
    last_activity: Instant,
}

enum CameraRelayCommand {
    Chunk { session_id: String, bytes: Vec<u8> },
    Stop { session_id: String },
}

struct ActiveCameraRelay {
    session_id: String,
    handle: VirtualCameraRelayHandle,
}

#[derive(Debug, Clone)]
pub struct CameraRelayStatus {
    pub device: String,
    pub queued_chunks: usize,
}

impl CameraRelay {
    pub fn new() -> Self {
        let relay = Self {
            inner: Arc::new(CameraRelayInner {
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

    pub async fn enqueue_media_chunk(
        &self,
        session_id: &str,
        _seq: u64,
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

        let queued_chunks = {
            let mut state = self.inner.state.lock().await;
            if let Some(active) = &state.active_session {
                let stale = state.last_activity.elapsed() >= CAMERA_SESSION_STALE_AFTER;
                if active != &session_id && !stale {
                    return Err(anyhow!(
                        "camera is already in use by another session; stop that uplink first"
                    ));
                }
            }
            state.active_session = Some(session_id.clone());
            state.last_activity = Instant::now();
            state
                .queue
                .push_back(CameraRelayCommand::Chunk { session_id, bytes });
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

        {
            let mut state = self.inner.state.lock().await;
            if state.active_session.as_deref() != Some(session_id.as_str()) {
                return Ok(());
            }
            state.active_session = None;
            state.last_activity = Instant::now();

            let mut retained = VecDeque::new();
            while let Some(command) = state.queue.pop_front() {
                match &command {
                    CameraRelayCommand::Chunk {
                        session_id: command_session_id,
                        ..
                    } if command_session_id == &session_id => {}
                    CameraRelayCommand::Stop {
                        session_id: command_session_id,
                    } if command_session_id == &session_id => {}
                    _ => retained.push_back(command),
                }
            }
            retained.push_back(CameraRelayCommand::Stop {
                session_id: session_id.clone(),
            });
            state.queue = retained;
        };

        self.inner.notify.notify_one();
        Ok(())
    }

    fn spawn_worker(&self) {
        let inner = Arc::clone(&self.inner);
        tokio::spawn(async move {
            let mut relay: Option<ActiveCameraRelay> = None;
            let mut placeholder: Option<VirtualCameraPlaceholderHandle> = None;
            let mut desktop_services_refreshed = false;
            loop {
                if relay.is_none() {
                    maybe_start_placeholder_relay(
                        &inner,
                        &mut placeholder,
                        &mut desktop_services_refreshed,
                    )
                    .await;
                }

                let Some(command) = next_command_or_stale_wait(&inner).await else {
                    expire_stale_session(&inner, &mut relay).await;
                    continue;
                };

                match command {
                    CameraRelayCommand::Chunk { session_id, bytes } => {
                        stop_placeholder_relay(&mut placeholder).await;
                        if let Err(err) = write_camera_chunk(&mut relay, session_id, bytes).await {
                            warn!("camera relay failed: {err}");
                        }
                    }
                    CameraRelayCommand::Stop { session_id } => {
                        if relay
                            .as_ref()
                            .is_some_and(|relay| relay.session_id == session_id)
                        {
                            stop_active_relay(&mut relay).await;
                        }
                    }
                }
            }
        });
    }
}

async fn maybe_start_placeholder_relay(
    inner: &CameraRelayInner,
    placeholder: &mut Option<VirtualCameraPlaceholderHandle>,
    desktop_services_refreshed: &mut bool,
) {
    if let Some(active) = placeholder.as_mut() {
        match active.child.try_wait() {
            Ok(None) => return,
            Ok(Some(status)) => {
                let mut stderr = String::new();
                if let Some(mut stream) = active.child.stderr.take() {
                    let _ = tokio::io::AsyncReadExt::read_to_string(&mut stream, &mut stderr).await;
                }
                warn!(
                    "virtual camera placeholder exited with {}: {}",
                    status,
                    stderr.trim()
                );
                *placeholder = None;
            }
            Err(err) => {
                warn!("failed to inspect virtual camera placeholder: {err}");
                *placeholder = None;
            }
        }
    }

    let should_start = {
        let state = inner.state.lock().await;
        state.active_session.is_none() && state.queue.is_empty()
    };
    if !should_start {
        return;
    }

    let device = match ensure_virtual_camera_device().await {
        Ok(device) => device,
        Err(err) => {
            warn!("virtual camera placeholder unavailable: {err}");
            return;
        }
    };

    match spawn_virtual_camera_placeholder(&device) {
        Ok(handle) => {
            *placeholder = Some(handle);
            if !*desktop_services_refreshed {
                refresh_virtual_camera_desktop_services(&device).await;
                *desktop_services_refreshed = true;
            }
        }
        Err(err) => {
            warn!("virtual camera placeholder failed: {err}");
        }
    }
}

async fn stop_placeholder_relay(placeholder: &mut Option<VirtualCameraPlaceholderHandle>) {
    let Some(mut active) = placeholder.take() else {
        return;
    };
    if let Err(err) = active.child.kill().await {
        warn!("failed to stop virtual camera placeholder: {err}");
    }
    if let Some(mut stream) = active.child.stderr.take() {
        let mut stderr = String::new();
        let _ = tokio::io::AsyncReadExt::read_to_string(&mut stream, &mut stderr).await;
        if !stderr.trim().is_empty() {
            warn!("virtual camera placeholder stderr: {}", stderr.trim());
        }
    }
}

async fn next_command_or_stale_wait(inner: &CameraRelayInner) -> Option<CameraRelayCommand> {
    let stale_in = {
        let mut state = inner.state.lock().await;
        if let Some(command) = state.queue.pop_front() {
            return Some(command);
        }
        state
            .active_session
            .as_ref()
            .map(|_| CAMERA_SESSION_STALE_AFTER.saturating_sub(state.last_activity.elapsed()))
    };

    match stale_in {
        Some(duration) if duration.is_zero() => None,
        Some(duration) => {
            tokio::select! {
                _ = inner.notify.notified() => {
                    let mut state = inner.state.lock().await;
                    state.queue.pop_front()
                }
                _ = sleep(duration) => None,
            }
        }
        None => {
            tokio::select! {
                _ = inner.notify.notified() => {
                    let mut state = inner.state.lock().await;
                    state.queue.pop_front()
                }
                _ = sleep(CAMERA_PLACEHOLDER_RETRY_AFTER) => None,
            }
        }
    }
}

async fn expire_stale_session(inner: &CameraRelayInner, relay: &mut Option<ActiveCameraRelay>) {
    let expired_session = {
        let mut state = inner.state.lock().await;
        match &state.active_session {
            Some(session_id) if state.last_activity.elapsed() >= CAMERA_SESSION_STALE_AFTER => {
                let session_id = session_id.clone();
                state.active_session = None;
                Some(session_id)
            }
            _ => None,
        }
    };

    if let Some(session_id) = expired_session {
        if relay
            .as_ref()
            .is_some_and(|relay| relay.session_id == session_id)
        {
            stop_active_relay(relay).await;
        }
    }
}

async fn write_camera_chunk(
    relay: &mut Option<ActiveCameraRelay>,
    session_id: String,
    bytes: Vec<u8>,
) -> Result<()> {
    if relay
        .as_ref()
        .is_none_or(|relay| relay.session_id != session_id)
    {
        stop_active_relay(relay).await;
        let device = ensure_virtual_camera_device().await?;
        *relay = Some(ActiveCameraRelay {
            session_id: session_id.clone(),
            handle: spawn_virtual_camera_relay(&device)?,
        });
    }

    if let Some(active) = relay.as_mut() {
        match active.handle.stdin.write_all(&bytes).await {
            Ok(()) => return Ok(()),
            Err(err) => {
                warn!("camera relay stdin write failed; restarting ffmpeg: {err}");
                stop_active_relay(relay).await;
            }
        }
    }

    let device = ensure_virtual_camera_device().await?;
    *relay = Some(ActiveCameraRelay {
        session_id,
        handle: spawn_virtual_camera_relay(&device)?,
    });
    if let Some(active) = relay.as_mut() {
        active.handle.stdin.write_all(&bytes).await?;
    }
    Ok(())
}

async fn stop_active_relay(relay: &mut Option<ActiveCameraRelay>) {
    let Some(mut active) = relay.take() else {
        return;
    };
    drop(active.handle.stdin);
    match active.handle.child.wait().await {
        Ok(status) if status.success() => {}
        Ok(status) => {
            let mut stderr = String::new();
            if let Some(mut stream) = active.handle.child.stderr.take() {
                let _ = tokio::io::AsyncReadExt::read_to_string(&mut stream, &mut stderr).await;
            }
            warn!(
                session_id = %active.session_id,
                "camera relay ffmpeg exited with {}: {}",
                status,
                stderr.trim()
            );
        }
        Err(err) => {
            warn!(session_id = %active.session_id, "failed to wait for camera relay ffmpeg: {err}");
        }
    }
}

fn sanitize_session_id(session_id: &str) -> String {
    session_id
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn sanitizes_camera_session_ids() {
        assert_eq!(super::sanitize_session_id("abc-123_DEF"), "abc-123_DEF");
        assert_eq!(super::sanitize_session_id("a/b c:d"), "abcd");
    }
}
