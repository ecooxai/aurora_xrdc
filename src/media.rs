use std::sync::Arc;

use anyhow::Result;
use tokio::{
    process::Child,
    sync::{Mutex, broadcast, watch},
};
use tracing::{info, warn};

use crate::{
    audio::AudioFrame,
    audio_streamer,
    ffmpeg::read_stderr,
    settings::{AudioStreamConfig, ServerConfig, StreamConfig},
    streamer::{self, StreamFrame},
};

#[derive(Debug, Clone)]
pub struct ActiveVideoState {
    pub config: StreamConfig,
    pub encoder: crate::ffmpeg::EncoderChoice,
}

#[derive(Debug, Clone)]
pub struct ActiveAudioState {
    pub config: AudioStreamConfig,
}

#[derive(Clone)]
pub struct MediaHub {
    video: Arc<VideoHub>,
    audio: Arc<AudioHub>,
}

struct VideoHub {
    server: ServerConfig,
    frames: watch::Sender<Option<StreamFrame>>,
    state: watch::Sender<Option<ActiveVideoState>>,
    runtime: Mutex<VideoRuntime>,
}

struct AudioHub {
    server: ServerConfig,
    frames: broadcast::Sender<AudioFrame>,
    state: watch::Sender<Option<ActiveAudioState>>,
    runtime: Mutex<AudioRuntime>,
}

struct VideoRuntime {
    active: Option<VideoProcess>,
    subscribers: usize,
    desired_config: StreamConfig,
    configured: bool,
}

struct AudioRuntime {
    active: Option<AudioProcess>,
    subscribers: usize,
    desired_config: AudioStreamConfig,
    configured: bool,
}

struct VideoProcess {
    state: ActiveVideoState,
    child: Arc<Mutex<Child>>,
}

struct AudioProcess {
    state: ActiveAudioState,
    child: Arc<Mutex<Child>>,
}

pub struct VideoLease {
    pub rx: watch::Receiver<Option<StreamFrame>>,
    pub state_rx: watch::Receiver<Option<ActiveVideoState>>,
    release: Option<ReleaseVideo>,
}

pub struct AudioLease {
    rx: Option<broadcast::Receiver<AudioFrame>>,
    pub state_rx: watch::Receiver<Option<ActiveAudioState>>,
    release: Option<ReleaseAudio>,
}

struct ReleaseVideo {
    hub: Arc<VideoHub>,
}

struct ReleaseAudio {
    hub: Arc<AudioHub>,
}

impl MediaHub {
    pub fn new(server: ServerConfig) -> Self {
        let (video_frames, _) = watch::channel(None);
        let (video_state, _) = watch::channel(None);
        let (audio_frames, _) = broadcast::channel(256);
        let (audio_state, _) = watch::channel(None);
        Self {
            video: Arc::new(VideoHub {
                server: server.clone(),
                frames: video_frames,
                state: video_state,
                runtime: Mutex::new(VideoRuntime {
                    active: None,
                    subscribers: 0,
                    desired_config: StreamConfig::default(),
                    configured: false,
                }),
            }),
            audio: Arc::new(AudioHub {
                server,
                frames: audio_frames,
                state: audio_state,
                runtime: Mutex::new(AudioRuntime {
                    active: None,
                    subscribers: 0,
                    desired_config: AudioStreamConfig::default(),
                    configured: false,
                }),
            }),
        }
    }

    pub async fn acquire_video(&self, requested: StreamConfig) -> Result<VideoLease> {
        self.video.acquire(requested).await
    }

    pub async fn acquire_audio(&self, requested: AudioStreamConfig) -> Result<AudioLease> {
        self.audio.acquire(requested).await
    }

    pub async fn update_stream_settings(
        &self,
        video_config: StreamConfig,
        audio_config: AudioStreamConfig,
    ) -> Result<()> {
        self.video.update_config(video_config).await?;
        self.audio.update_config(audio_config).await?;
        Ok(())
    }
}

impl VideoHub {
    async fn acquire(self: &Arc<Self>, requested: StreamConfig) -> Result<VideoLease> {
        let mut runtime = self.runtime.lock().await;
        if !runtime.configured {
            runtime.desired_config = requested;
            runtime.configured = true;
        }
        runtime.subscribers += 1;
        if let Err(err) = self.ensure_active(&mut runtime).await {
            runtime.subscribers = runtime.subscribers.saturating_sub(1);
            return Err(err);
        }
        Ok(VideoLease {
            rx: self.frames.subscribe(),
            state_rx: self.state.subscribe(),
            release: Some(ReleaseVideo {
                hub: Arc::clone(self),
            }),
        })
    }

    async fn update_config(&self, config: StreamConfig) -> Result<()> {
        let mut runtime = self.runtime.lock().await;
        runtime.desired_config = config;
        runtime.configured = true;
        self.ensure_active(&mut runtime).await
    }

    async fn ensure_active(&self, runtime: &mut VideoRuntime) -> Result<()> {
        let desired = runtime.desired_config.clone();
        if let Some(active) = runtime.active.as_ref() {
            if active.state.config == desired {
                return Ok(());
            }
        }

        if let Some(active) = runtime.active.take() {
            info!(
                old = ?active.state.config,
                new = ?desired,
                "restarting shared video capture"
            );
            self.state.send_replace(None);
            self.frames.send_replace(None);
            shutdown_child(
                "video",
                Some(&active.state.config),
                Arc::clone(&active.child),
            )
            .await;
        } else if runtime.subscribers == 0 {
            return Ok(());
        } else {
            info!(config = ?desired, "starting shared video capture");
        }

        if runtime.subscribers == 0 {
            return Ok(());
        }

        match streamer::start(self.server.clone(), desired.clone(), self.frames.clone()).await {
            Ok((encoder, child)) => {
                let state = ActiveVideoState {
                    config: desired,
                    encoder,
                };
                self.state.send_replace(Some(state.clone()));
                runtime.active = Some(VideoProcess {
                    state,
                    child: Arc::new(Mutex::new(child)),
                });
                Ok(())
            }
            Err(err) => {
                self.state.send_replace(None);
                self.frames.send_replace(None);
                Err(err)
            }
        }
    }

    async fn release(&self) {
        let removed = {
            let mut runtime = self.runtime.lock().await;
            runtime.subscribers = runtime.subscribers.saturating_sub(1);
            if runtime.subscribers > 0 {
                if let Some(active) = runtime.active.as_ref() {
                    info!(
                        config = ?active.state.config,
                        subscribers = runtime.subscribers,
                        "shared video capture still in use"
                    );
                }
                return;
            }
            let removed = runtime.active.take();
            if removed.is_some() {
                self.state.send_replace(None);
                self.frames.send_replace(None);
            }
            removed
        };
        if let Some(active) = removed {
            info!(config = ?active.state.config, "stopping shared video capture");
            shutdown_child(
                "video",
                Some(&active.state.config),
                Arc::clone(&active.child),
            )
            .await;
        }
    }
}

impl AudioHub {
    async fn acquire(self: &Arc<Self>, requested: AudioStreamConfig) -> Result<AudioLease> {
        let mut runtime = self.runtime.lock().await;
        if !runtime.configured {
            runtime.desired_config = requested;
            runtime.configured = true;
        }
        runtime.subscribers += 1;
        if let Err(err) = self.ensure_active(&mut runtime).await {
            runtime.subscribers = runtime.subscribers.saturating_sub(1);
            return Err(err);
        }
        Ok(AudioLease {
            rx: Some(self.frames.subscribe()),
            state_rx: self.state.subscribe(),
            release: Some(ReleaseAudio {
                hub: Arc::clone(self),
            }),
        })
    }

    async fn update_config(&self, config: AudioStreamConfig) -> Result<()> {
        let mut runtime = self.runtime.lock().await;
        runtime.desired_config = config;
        runtime.configured = true;
        self.ensure_active(&mut runtime).await
    }

    async fn ensure_active(&self, runtime: &mut AudioRuntime) -> Result<()> {
        let desired = runtime.desired_config.clone();
        if let Some(active) = runtime.active.as_ref() {
            if active.state.config == desired {
                return Ok(());
            }
        }

        if let Some(active) = runtime.active.take() {
            info!(
                old = ?active.state.config,
                new = ?desired,
                "restarting shared audio capture"
            );
            self.state.send_replace(None);
            shutdown_audio_child(Arc::clone(&active.child), &active.state.config).await;
        } else if runtime.subscribers == 0 {
            return Ok(());
        } else {
            info!(config = ?desired, "starting shared audio capture");
        }

        if runtime.subscribers == 0 {
            return Ok(());
        }

        match audio_streamer::start(&self.server, &desired, self.frames.clone()).await {
            Ok(child) => {
                let state = ActiveAudioState { config: desired };
                self.state.send_replace(Some(state.clone()));
                runtime.active = Some(AudioProcess {
                    state,
                    child: Arc::new(Mutex::new(child)),
                });
                Ok(())
            }
            Err(err) => {
                self.state.send_replace(None);
                Err(err)
            }
        }
    }

    async fn release(&self) {
        let removed = {
            let mut runtime = self.runtime.lock().await;
            runtime.subscribers = runtime.subscribers.saturating_sub(1);
            if runtime.subscribers > 0 {
                if let Some(active) = runtime.active.as_ref() {
                    info!(
                        config = ?active.state.config,
                        subscribers = runtime.subscribers,
                        "shared audio capture still in use"
                    );
                }
                return;
            }
            let removed = runtime.active.take();
            if removed.is_some() {
                self.state.send_replace(None);
            }
            removed
        };
        if let Some(active) = removed {
            info!(config = ?active.state.config, "stopping shared audio capture");
            shutdown_audio_child(Arc::clone(&active.child), &active.state.config).await;
        }
    }
}

impl AudioLease {
    pub fn take_audio_rx(&mut self) -> Option<broadcast::Receiver<AudioFrame>> {
        self.rx.take()
    }
}

impl Drop for VideoLease {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            tokio::spawn(async move {
                release.hub.release().await;
            });
        }
    }
}

impl Drop for AudioLease {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            tokio::spawn(async move {
                release.hub.release().await;
            });
        }
    }
}

async fn shutdown_child(kind: &str, config: Option<&StreamConfig>, child: Arc<Mutex<Child>>) {
    let mut child = child.lock().await;
    if let Err(err) = child.kill().await {
        warn!(kind = kind, ?config, "capture kill failed: {err}");
    }
    let stderr = read_stderr(&mut child).await;
    if !stderr.trim().is_empty() {
        warn!(
            kind = kind,
            ?config,
            stderr = stderr.trim(),
            "capture stderr"
        );
    }
}

async fn shutdown_audio_child(child: Arc<Mutex<Child>>, config: &AudioStreamConfig) {
    let mut child = child.lock().await;
    if let Err(err) = child.kill().await {
        warn!(kind = "audio", ?config, "capture kill failed: {err}");
    }
    let stderr = read_stderr(&mut child).await;
    if !stderr.trim().is_empty() {
        warn!(
            kind = "audio",
            ?config,
            stderr = stderr.trim(),
            "capture stderr"
        );
    }
}
