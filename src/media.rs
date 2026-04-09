use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use tokio::{
    process::Child,
    sync::{Mutex, watch},
};
use tracing::{info, warn};

use crate::{
    audio::AudioFrame,
    audio_streamer,
    ffmpeg::read_stderr,
    settings::{ServerConfig, StreamConfig},
    streamer::{self, StreamFrame},
};

#[derive(Clone)]
pub struct MediaHub {
    server: ServerConfig,
    state: Arc<Mutex<HubState>>,
}

struct HubState {
    videos: HashMap<StreamConfig, VideoSlot>,
    audio: Option<AudioSlot>,
}

struct VideoSlot {
    entry: Arc<VideoEntry>,
    subscribers: usize,
}

struct AudioSlot {
    entry: Arc<AudioEntry>,
    subscribers: usize,
}

struct VideoEntry {
    config: StreamConfig,
    encoder: crate::ffmpeg::EncoderChoice,
    rx: watch::Receiver<Option<StreamFrame>>,
    child: Arc<Mutex<Child>>,
}

struct AudioEntry {
    rx: watch::Receiver<Option<AudioFrame>>,
    child: Arc<Mutex<Child>>,
}

pub struct VideoLease {
    pub rx: watch::Receiver<Option<StreamFrame>>,
    pub encoder: crate::ffmpeg::EncoderChoice,
    release: Option<ReleaseVideo>,
}

pub struct AudioLease {
    pub rx: watch::Receiver<Option<AudioFrame>>,
    release: Option<ReleaseAudio>,
}

struct ReleaseVideo {
    hub: MediaHub,
    config: StreamConfig,
    entry: Arc<VideoEntry>,
}

struct ReleaseAudio {
    hub: MediaHub,
    entry: Arc<AudioEntry>,
}

impl MediaHub {
    pub fn new(server: ServerConfig) -> Self {
        Self {
            server,
            state: Arc::new(Mutex::new(HubState {
                videos: HashMap::new(),
                audio: None,
            })),
        }
    }

    pub async fn acquire_video(&self, config: StreamConfig) -> Result<VideoLease> {
        {
            let mut state = self.state.lock().await;
            if let Some(slot) = state.videos.get_mut(&config) {
                slot.subscribers += 1;
                info!(
                    ?config,
                    subscribers = slot.subscribers,
                    "reusing shared video capture"
                );
                return Ok(VideoLease {
                    rx: slot.entry.rx.clone(),
                    encoder: slot.entry.encoder.clone(),
                    release: Some(ReleaseVideo {
                        hub: self.clone(),
                        config,
                        entry: Arc::clone(&slot.entry),
                    }),
                });
            }
        }

        let (stream, child) = streamer::start(self.server.clone(), config.clone()).await?;
        let entry = Arc::new(VideoEntry {
            config: config.clone(),
            encoder: stream.encoder,
            rx: stream.rx,
            child: Arc::new(Mutex::new(child)),
        });

        let mut replace = None;
        let lease = {
            let mut state = self.state.lock().await;
            if let Some(slot) = state.videos.get_mut(&config) {
                slot.subscribers += 1;
                replace = Some(Arc::clone(&entry));
                VideoLease {
                    rx: slot.entry.rx.clone(),
                    encoder: slot.entry.encoder.clone(),
                    release: Some(ReleaseVideo {
                        hub: self.clone(),
                        config,
                        entry: Arc::clone(&slot.entry),
                    }),
                }
            } else {
                info!(?config, "starting shared video capture");
                state.videos.insert(
                    config.clone(),
                    VideoSlot {
                        entry: Arc::clone(&entry),
                        subscribers: 1,
                    },
                );
                VideoLease {
                    rx: entry.rx.clone(),
                    encoder: entry.encoder.clone(),
                    release: Some(ReleaseVideo {
                        hub: self.clone(),
                        config,
                        entry,
                    }),
                }
            }
        };

        if let Some(entry) = replace {
            shutdown_child("video", Some(&entry.config), Arc::clone(&entry.child)).await;
        }

        Ok(lease)
    }

    pub async fn acquire_audio(&self) -> Result<AudioLease> {
        {
            let mut state = self.state.lock().await;
            if let Some(slot) = state.audio.as_mut() {
                slot.subscribers += 1;
                info!(subscribers = slot.subscribers, "reusing shared audio capture");
                return Ok(AudioLease {
                    rx: slot.entry.rx.clone(),
                    release: Some(ReleaseAudio {
                        hub: self.clone(),
                        entry: Arc::clone(&slot.entry),
                    }),
                });
            }
        }

        let (stream, child) = audio_streamer::start(&self.server).await?;
        let entry = Arc::new(AudioEntry {
            rx: stream.rx,
            child: Arc::new(Mutex::new(child)),
        });

        let mut replace = None;
        let lease = {
            let mut state = self.state.lock().await;
            if let Some(slot) = state.audio.as_mut() {
                slot.subscribers += 1;
                replace = Some(Arc::clone(&entry));
                AudioLease {
                    rx: slot.entry.rx.clone(),
                    release: Some(ReleaseAudio {
                        hub: self.clone(),
                        entry: Arc::clone(&slot.entry),
                    }),
                }
            } else {
                info!("starting shared audio capture");
                state.audio = Some(AudioSlot {
                    entry: Arc::clone(&entry),
                    subscribers: 1,
                });
                AudioLease {
                    rx: entry.rx.clone(),
                    release: Some(ReleaseAudio {
                        hub: self.clone(),
                        entry,
                    }),
                }
            }
        };

        if let Some(entry) = replace {
            shutdown_child("audio", None, Arc::clone(&entry.child)).await;
        }

        Ok(lease)
    }

    async fn release_video(&self, config: StreamConfig, entry: Arc<VideoEntry>) {
        let removed = {
            let mut state = self.state.lock().await;
            let Some(slot) = state.videos.get_mut(&config) else {
                return;
            };
            if !Arc::ptr_eq(&slot.entry, &entry) {
                return;
            }
            slot.subscribers = slot.subscribers.saturating_sub(1);
            if slot.subscribers > 0 {
                info!(
                    ?config,
                    subscribers = slot.subscribers,
                    "shared video capture still in use"
                );
                None
            } else {
                info!(?config, "stopping shared video capture");
                state.videos.remove(&config).map(|slot| slot.entry)
            }
        };
        if let Some(entry) = removed {
            shutdown_child("video", Some(&config), Arc::clone(&entry.child)).await;
        }
    }

    async fn release_audio(&self, entry: Arc<AudioEntry>) {
        let removed = {
            let mut state = self.state.lock().await;
            let Some(slot) = state.audio.as_mut() else {
                return;
            };
            if !Arc::ptr_eq(&slot.entry, &entry) {
                return;
            }
            slot.subscribers = slot.subscribers.saturating_sub(1);
            if slot.subscribers > 0 {
                info!(subscribers = slot.subscribers, "shared audio capture still in use");
                None
            } else {
                info!("stopping shared audio capture");
                state.audio.take().map(|slot| slot.entry)
            }
        };
        if let Some(entry) = removed {
            shutdown_child("audio", None, Arc::clone(&entry.child)).await;
        }
    }
}

impl Drop for VideoLease {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            tokio::spawn(async move {
                release.hub.release_video(release.config, release.entry).await;
            });
        }
    }
}

impl Drop for AudioLease {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            tokio::spawn(async move {
                release.hub.release_audio(release.entry).await;
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
        warn!(kind = kind, ?config, stderr = stderr.trim(), "capture stderr");
    }
}
