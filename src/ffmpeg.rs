use std::{ffi::OsStr, fs, path::PathBuf, process::Stdio};

use anyhow::{Context, Result, anyhow};
use serde::Serialize;
use tokio::{
    io::AsyncReadExt,
    process::{Child, ChildStdin, Command},
    time::{Duration, sleep},
};
use tracing::warn;

use crate::settings::{
    AudioStreamConfig, CodecKind, EncodePreference, EncoderLatencyMode, EncoderQualityMode,
    ServerConfig, StreamConfig, VideoScale,
};
use crate::x11_input::screen_size;

#[derive(Debug, Clone)]
pub struct EncoderChoice {
    pub ffmpeg_encoder: String,
    pub mode: &'static str,
    pub output_format: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct AvailableEncoderOption {
    pub value: EncodePreference,
    pub label: String,
    pub mode: &'static str,
    pub ffmpeg_encoder: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AvailableCodecOption {
    pub value: CodecKind,
    pub label: &'static str,
}

#[derive(Debug, Clone, Serialize)]
pub struct AudioOutputDevice {
    pub name: String,
    pub description: String,
    pub is_virtual: bool,
    pub is_default: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EncoderBackend {
    Cpu,
    NvidiaNvenc,
    IntelQsv,
    Vaapi,
}

#[derive(Debug, Clone, Copy)]
struct EncoderProfile {
    ffmpeg_encoder: &'static str,
    mode: &'static str,
    output_format: &'static str,
    backend: EncoderBackend,
}

pub struct MicInputHandle {
    pub child: Child,
    pub stdin: ChildStdin,
}

pub struct VirtualCameraRelayHandle {
    pub child: Child,
    pub stdin: ChildStdin,
}

pub struct VirtualCameraPlaceholderHandle {
    pub child: Child,
}

const VIRTUAL_MIC_SOURCE_NAME: &str = "Viberdeskmic";
const VIRTUAL_MIC_SINK_NAME: &str = "vibe_rdesk_virtual_mic_sink";
const VIRTUAL_CAMERA_LABEL: &str = "VibeRDesk Camera";
const LEGACY_VIRTUAL_CAMERA_LABELS: &[&str] = &["viberdeskcamera", "vibedeskcamera"];
const VIRTUAL_CAMERA_NR: &str = "42";
const DISPLAY_WAKE_RETRY_DELAY: Duration = Duration::from_millis(150);

const H264_GPU_ENCODERS: &[EncoderProfile] = &[
    EncoderProfile {
        ffmpeg_encoder: "h264_nvenc",
        mode: "gpu",
        output_format: "h264",
        backend: EncoderBackend::NvidiaNvenc,
    },
    EncoderProfile {
        ffmpeg_encoder: "h264_qsv",
        mode: "gpu",
        output_format: "h264",
        backend: EncoderBackend::IntelQsv,
    },
    EncoderProfile {
        ffmpeg_encoder: "h264_vaapi",
        mode: "gpu",
        output_format: "h264",
        backend: EncoderBackend::Vaapi,
    },
    EncoderProfile {
        ffmpeg_encoder: "libx264",
        mode: "cpu",
        output_format: "h264",
        backend: EncoderBackend::Cpu,
    },
];

const H264_CPU_ENCODERS: &[EncoderProfile] = &[EncoderProfile {
    ffmpeg_encoder: "libx264",
    mode: "cpu",
    output_format: "h264",
    backend: EncoderBackend::Cpu,
}];

const H265_GPU_ENCODERS: &[EncoderProfile] = &[
    EncoderProfile {
        ffmpeg_encoder: "hevc_nvenc",
        mode: "gpu",
        output_format: "hevc",
        backend: EncoderBackend::NvidiaNvenc,
    },
    EncoderProfile {
        ffmpeg_encoder: "hevc_qsv",
        mode: "gpu",
        output_format: "hevc",
        backend: EncoderBackend::IntelQsv,
    },
    EncoderProfile {
        ffmpeg_encoder: "hevc_vaapi",
        mode: "gpu",
        output_format: "hevc",
        backend: EncoderBackend::Vaapi,
    },
    EncoderProfile {
        ffmpeg_encoder: "libx265",
        mode: "cpu",
        output_format: "hevc",
        backend: EncoderBackend::Cpu,
    },
];

const H265_CPU_ENCODERS: &[EncoderProfile] = &[EncoderProfile {
    ffmpeg_encoder: "libx265",
    mode: "cpu",
    output_format: "hevc",
    backend: EncoderBackend::Cpu,
}];

const VP8_ENCODERS: &[EncoderProfile] = &[EncoderProfile {
    ffmpeg_encoder: "libvpx",
    mode: "cpu",
    output_format: "ivf",
    backend: EncoderBackend::Cpu,
}];

const VP9_GPU_ENCODERS: &[EncoderProfile] = &[
    EncoderProfile {
        ffmpeg_encoder: "vp9_qsv",
        mode: "gpu",
        output_format: "ivf",
        backend: EncoderBackend::IntelQsv,
    },
    EncoderProfile {
        ffmpeg_encoder: "vp9_vaapi",
        mode: "gpu",
        output_format: "ivf",
        backend: EncoderBackend::Vaapi,
    },
    EncoderProfile {
        ffmpeg_encoder: "libvpx-vp9",
        mode: "cpu",
        output_format: "ivf",
        backend: EncoderBackend::Cpu,
    },
];

const VP9_CPU_ENCODERS: &[EncoderProfile] = &[EncoderProfile {
    ffmpeg_encoder: "libvpx-vp9",
    mode: "cpu",
    output_format: "ivf",
    backend: EncoderBackend::Cpu,
}];

const AV1_GPU_ENCODERS: &[EncoderProfile] = &[
    EncoderProfile {
        ffmpeg_encoder: "av1_nvenc",
        mode: "gpu",
        output_format: "ivf",
        backend: EncoderBackend::NvidiaNvenc,
    },
    EncoderProfile {
        ffmpeg_encoder: "av1_qsv",
        mode: "gpu",
        output_format: "ivf",
        backend: EncoderBackend::IntelQsv,
    },
    EncoderProfile {
        ffmpeg_encoder: "av1_vaapi",
        mode: "gpu",
        output_format: "ivf",
        backend: EncoderBackend::Vaapi,
    },
    EncoderProfile {
        ffmpeg_encoder: "libsvtav1",
        mode: "cpu",
        output_format: "ivf",
        backend: EncoderBackend::Cpu,
    },
    EncoderProfile {
        ffmpeg_encoder: "libaom-av1",
        mode: "cpu",
        output_format: "ivf",
        backend: EncoderBackend::Cpu,
    },
];

const AV1_CPU_ENCODERS: &[EncoderProfile] = &[
    EncoderProfile {
        ffmpeg_encoder: "libsvtav1",
        mode: "cpu",
        output_format: "ivf",
        backend: EncoderBackend::Cpu,
    },
    EncoderProfile {
        ffmpeg_encoder: "libaom-av1",
        mode: "cpu",
        output_format: "ivf",
        backend: EncoderBackend::Cpu,
    },
];

pub async fn choose_encoder(
    codec: CodecKind,
    encode_preference: EncodePreference,
) -> Result<EncoderChoice> {
    let encoders = ffmpeg_list_encoders().await?;
    for profile in preferred_encoders(codec, encode_preference) {
        if has_working_encoder(&encoders, profile).await {
            return Ok(EncoderChoice {
                ffmpeg_encoder: profile.ffmpeg_encoder.into(),
                mode: profile.mode,
                output_format: profile.output_format,
            });
        }
    }

    Err(anyhow!(
        "no working ffmpeg encoder available for requested codec {codec:?}"
    ))
}

pub async fn available_encoder_options(codec: CodecKind) -> Result<Vec<AvailableEncoderOption>> {
    let encoders = ffmpeg_list_encoders().await?;
    let mut options = Vec::new();
    for profile in available_profiles(codec, &encoders).await {
        options.push(AvailableEncoderOption {
            value: specific_preference(profile.ffmpeg_encoder)
                .ok_or_else(|| anyhow!("unsupported encoder {}", profile.ffmpeg_encoder))?,
            label: format!(
                "{} ({})",
                profile.ffmpeg_encoder,
                profile.mode.to_ascii_uppercase()
            ),
            mode: profile.mode,
            ffmpeg_encoder: Some(profile.ffmpeg_encoder.into()),
        });
    }
    Ok(options)
}

pub async fn available_codec_options() -> Result<Vec<AvailableCodecOption>> {
    let encoders = ffmpeg_list_encoders().await?;
    let mut options = Vec::new();
    for codec in [
        CodecKind::H264,
        CodecKind::H265,
        CodecKind::Vp8,
        CodecKind::Vp9,
        CodecKind::Av1,
    ] {
        if !available_profiles(codec, &encoders).await.is_empty() {
            options.push(AvailableCodecOption {
                value: codec,
                label: codec.label(),
            });
        }
    }
    Ok(options)
}

async fn available_profiles(codec: CodecKind, encoders: &str) -> Vec<EncoderProfile> {
    let mut profiles = Vec::new();
    for profile in all_profiles(codec) {
        if has_working_encoder(encoders, *profile).await {
            profiles.push(*profile);
        }
    }
    profiles
}

fn preferred_encoders(
    codec: CodecKind,
    encode_preference: EncodePreference,
) -> Vec<EncoderProfile> {
    if let Some(profile) = specific_profile(codec, encode_preference) {
        return vec![profile];
    }

    match (codec, encode_preference) {
        (CodecKind::H264, EncodePreference::Nvidia) => H264_GPU_ENCODERS.to_vec(),
        (CodecKind::H264, EncodePreference::Gpu) => H264_GPU_ENCODERS.to_vec(),
        (CodecKind::H264, EncodePreference::Cpu) => H264_CPU_ENCODERS.to_vec(),
        (CodecKind::H265, EncodePreference::Nvidia) => H265_GPU_ENCODERS.to_vec(),
        (CodecKind::H265, EncodePreference::Gpu) => H265_GPU_ENCODERS.to_vec(),
        (CodecKind::H265, EncodePreference::Cpu) => H265_CPU_ENCODERS.to_vec(),
        (CodecKind::Vp8, _) => VP8_ENCODERS.to_vec(),
        (CodecKind::Vp9, EncodePreference::Gpu) => VP9_GPU_ENCODERS.to_vec(),
        (CodecKind::Vp9, EncodePreference::Cpu) => VP9_CPU_ENCODERS.to_vec(),
        (CodecKind::Vp9, _) => VP9_GPU_ENCODERS.to_vec(),
        (CodecKind::Av1, EncodePreference::Nvidia) => AV1_GPU_ENCODERS.to_vec(),
        (CodecKind::Av1, EncodePreference::Gpu) => AV1_GPU_ENCODERS.to_vec(),
        (CodecKind::Av1, EncodePreference::Cpu) => AV1_CPU_ENCODERS.to_vec(),
        _ => Vec::new(),
    }
}

fn all_profiles(codec: CodecKind) -> &'static [EncoderProfile] {
    match codec {
        CodecKind::H264 => H264_GPU_ENCODERS,
        CodecKind::H265 => H265_GPU_ENCODERS,
        CodecKind::Vp8 => VP8_ENCODERS,
        CodecKind::Vp9 => VP9_GPU_ENCODERS,
        CodecKind::Av1 => AV1_GPU_ENCODERS,
    }
}

fn specific_profile(
    codec: CodecKind,
    encode_preference: EncodePreference,
) -> Option<EncoderProfile> {
    let ffmpeg_encoder = specific_ffmpeg_encoder(codec, encode_preference)?;
    all_profiles(codec)
        .iter()
        .copied()
        .find(|profile| profile.ffmpeg_encoder == ffmpeg_encoder)
}

fn specific_ffmpeg_encoder(
    codec: CodecKind,
    encode_preference: EncodePreference,
) -> Option<&'static str> {
    match (codec, encode_preference) {
        (CodecKind::H264, EncodePreference::H264Nvenc) => Some("h264_nvenc"),
        (CodecKind::H264, EncodePreference::H264Qsv) => Some("h264_qsv"),
        (CodecKind::H264, EncodePreference::H264Vaapi) => Some("h264_vaapi"),
        (CodecKind::H264, EncodePreference::Libx264) => Some("libx264"),
        (CodecKind::H265, EncodePreference::HevcNvenc) => Some("hevc_nvenc"),
        (CodecKind::H265, EncodePreference::HevcQsv) => Some("hevc_qsv"),
        (CodecKind::H265, EncodePreference::HevcVaapi) => Some("hevc_vaapi"),
        (CodecKind::H265, EncodePreference::Libx265) => Some("libx265"),
        (CodecKind::Vp8, EncodePreference::Libvpx) => Some("libvpx"),
        (CodecKind::Vp9, EncodePreference::Vp9Qsv) => Some("vp9_qsv"),
        (CodecKind::Vp9, EncodePreference::Vp9Vaapi) => Some("vp9_vaapi"),
        (CodecKind::Vp9, EncodePreference::LibvpxVp9) => Some("libvpx-vp9"),
        (CodecKind::Av1, EncodePreference::Av1Nvenc) => Some("av1_nvenc"),
        (CodecKind::Av1, EncodePreference::Av1Qsv) => Some("av1_qsv"),
        (CodecKind::Av1, EncodePreference::Av1Vaapi) => Some("av1_vaapi"),
        (CodecKind::Av1, EncodePreference::LibSvtAv1) => Some("libsvtav1"),
        (CodecKind::Av1, EncodePreference::LibAomAv1) => Some("libaom-av1"),
        _ => None,
    }
}

fn specific_preference(ffmpeg_encoder: &str) -> Option<EncodePreference> {
    match ffmpeg_encoder {
        "h264_nvenc" => Some(EncodePreference::H264Nvenc),
        "h264_qsv" => Some(EncodePreference::H264Qsv),
        "h264_vaapi" => Some(EncodePreference::H264Vaapi),
        "libx264" => Some(EncodePreference::Libx264),
        "hevc_nvenc" => Some(EncodePreference::HevcNvenc),
        "hevc_qsv" => Some(EncodePreference::HevcQsv),
        "hevc_vaapi" => Some(EncodePreference::HevcVaapi),
        "libx265" => Some(EncodePreference::Libx265),
        "libvpx" => Some(EncodePreference::Libvpx),
        "vp9_qsv" => Some(EncodePreference::Vp9Qsv),
        "vp9_vaapi" => Some(EncodePreference::Vp9Vaapi),
        "libvpx-vp9" => Some(EncodePreference::LibvpxVp9),
        "av1_nvenc" => Some(EncodePreference::Av1Nvenc),
        "av1_qsv" => Some(EncodePreference::Av1Qsv),
        "av1_vaapi" => Some(EncodePreference::Av1Vaapi),
        "libsvtav1" => Some(EncodePreference::LibSvtAv1),
        "libaom-av1" => Some(EncodePreference::LibAomAv1),
        _ => None,
    }
}

async fn has_working_encoder(encoders: &str, profile: EncoderProfile) -> bool {
    if !encoders.contains(&format!(" {} ", profile.ffmpeg_encoder)) {
        return false;
    }
    ffmpeg_probe_encoder(profile).await.unwrap_or(false)
}

async fn ffmpeg_probe_encoder(profile: EncoderProfile) -> Result<bool> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args(["-loglevel", "error"]);
    append_hw_device_args(&mut cmd, profile.backend)?;
    cmd.args([
        "-f",
        "lavfi",
        "-i",
        "color=size=128x128:rate=1:color=black",
        "-frames:v",
        "1",
        "-an",
        "-sn",
    ]);
    append_video_filter_args(&mut cmd, profile.backend, VideoScale::Native);
    cmd.args(["-c:v", profile.ffmpeg_encoder]);
    append_bitrate_args(&mut cmd, 200, 500, EncoderQualityMode::Balanced);
    append_gop_args(&mut cmd, "1", profile.backend);
    append_encoder_specific_args(
        &mut cmd,
        profile.ffmpeg_encoder,
        EncoderLatencyMode::Low,
        EncoderQualityMode::Balanced,
    );
    let output = cmd
        .args(["-f", "null", "-"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .with_context(|| format!("failed to probe ffmpeg encoder {}", profile.ffmpeg_encoder))?;
    Ok(output.status.success())
}

pub fn spawn_capture(
    server: &ServerConfig,
    stream: &StreamConfig,
    encoder: &EncoderChoice,
) -> Result<tokio::process::Child> {
    let fps = stream.fps.to_string();
    let gop = video_gop_frames(stream.fps, stream.performance.gop_ms).to_string();
    let backend = encoder_backend(&encoder.ffmpeg_encoder);
    let mut cmd = Command::new("ffmpeg");
    cmd.env("DISPLAY", &server.display).args([
        "-loglevel",
        "error",
        "-probesize",
        "32",
        "-analyzeduration",
        "0",
        "-fflags",
        "nobuffer",
        "-avioflags",
        "direct",
        "-flush_packets",
        "1",
        "-max_delay",
        "0",
        "-flags",
        "low_delay",
    ]);
    append_hw_device_args(&mut cmd, backend)?;
    let capture_size = screen_size(&server.display)
        .map(|(width, height)| format!("{width}x{height}"))
        .with_context(|| format!("failed to query X11 screen size for {}", server.display))?;
    cmd.args([
        "-f",
        "x11grab",
        "-framerate",
        &fps,
        "-video_size",
        &capture_size,
        "-i",
        &server.display,
        "-an",
        "-sn",
    ]);
    append_video_filter_args(&mut cmd, backend, stream.performance.scale);
    cmd.args(["-c:v", &encoder.ffmpeg_encoder]);
    append_bitrate_args(
        &mut cmd,
        stream.bitrate_kbps,
        stream.performance.buffer_ms,
        stream.performance.encoder_quality,
    );
    append_gop_args(&mut cmd, &gop, backend);
    append_encoder_specific_args(
        &mut cmd,
        &encoder.ffmpeg_encoder,
        stream.performance.encoder_latency,
        stream.performance.encoder_quality,
    );
    append_bitstream_filter_args(&mut cmd, encoder.output_format);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    cmd.args(["-f", encoder.output_format, "pipe:1"]);
    cmd.spawn().context("failed to spawn ffmpeg capture")
}

fn append_hw_device_args(cmd: &mut Command, backend: EncoderBackend) -> Result<()> {
    let Some(render_device) = matches!(backend, EncoderBackend::IntelQsv | EncoderBackend::Vaapi)
        .then(render_device_path)
        .transpose()?
    else {
        return Ok(());
    };

    let render_device = render_device.to_string_lossy().into_owned();
    match backend {
        EncoderBackend::IntelQsv => {
            cmd.args([
                "-init_hw_device",
                &format!("qsv=hw:{render_device}"),
                "-filter_hw_device",
                "hw",
            ]);
        }
        EncoderBackend::Vaapi => {
            cmd.args(["-vaapi_device", &render_device]);
        }
        EncoderBackend::Cpu | EncoderBackend::NvidiaNvenc => {}
    }
    Ok(())
}

fn append_video_filter_args(cmd: &mut Command, backend: EncoderBackend, scale: VideoScale) {
    let scale_filter = scale
        .target_height()
        .map(|height| format!("scale=-2:min(ih\\,{height})"));
    match backend {
        EncoderBackend::Cpu | EncoderBackend::NvidiaNvenc => {
            if let Some(scale_filter) = scale_filter {
                cmd.args(["-vf", &scale_filter]);
            }
            cmd.args(["-pix_fmt", "yuv420p"]);
        }
        EncoderBackend::IntelQsv | EncoderBackend::Vaapi => {
            let filter = match scale_filter {
                Some(scale_filter) => {
                    format!("{scale_filter},format=nv12,hwupload=extra_hw_frames=64")
                }
                None => "format=nv12,hwupload=extra_hw_frames=64".into(),
            };
            cmd.args(["-vf", &filter]);
        }
    }
}

fn append_bitrate_args(
    cmd: &mut Command,
    bitrate_kbps: u32,
    buffer_ms: u32,
    quality: EncoderQualityMode,
) {
    let bitrate = format!("{bitrate_kbps}k");
    let maxrate = format!("{}k", video_maxrate_kbps(bitrate_kbps, quality));
    let buffer = video_buffer_size(video_maxrate_kbps(bitrate_kbps, quality), buffer_ms);
    cmd.args(["-b:v", &bitrate, "-maxrate", &maxrate, "-bufsize", &buffer]);
}

fn append_gop_args(cmd: &mut Command, gop: &str, backend: EncoderBackend) {
    cmd.args(["-g", gop]);
    match backend {
        EncoderBackend::Cpu | EncoderBackend::NvidiaNvenc => {
            cmd.args(["-keyint_min", gop, "-threads", "2"]);
        }
        EncoderBackend::IntelQsv | EncoderBackend::Vaapi => {}
    }
}

fn append_encoder_specific_args(
    cmd: &mut Command,
    encoder: &str,
    latency: EncoderLatencyMode,
    quality: EncoderQualityMode,
) {
    match encoder {
        "h264_nvenc" | "hevc_nvenc" | "av1_nvenc" => {
            let tune = match latency {
                EncoderLatencyMode::UltraLow => "ull",
                EncoderLatencyMode::Low | EncoderLatencyMode::Balanced => "ll",
            };
            let preset = match (latency, quality) {
                (EncoderLatencyMode::UltraLow, _) => "p1",
                (_, EncoderQualityMode::Fast) => "p2",
                (EncoderLatencyMode::Low, EncoderQualityMode::Balanced) => "p3",
                (EncoderLatencyMode::Balanced, EncoderQualityMode::Balanced)
                | (_, EncoderQualityMode::SharpText) => "p4",
            };
            let rc = match (latency, quality) {
                (_, EncoderQualityMode::SharpText) => "vbr_hq",
                (EncoderLatencyMode::Balanced, _) => "cbr_hq",
                (EncoderLatencyMode::UltraLow | EncoderLatencyMode::Low, _) => "cbr_ld_hq",
            };
            let rc_lookahead = match (latency, quality) {
                (EncoderLatencyMode::UltraLow, _) | (_, EncoderQualityMode::Fast) => "0",
                (_, EncoderQualityMode::Balanced) => "4",
                (_, EncoderQualityMode::SharpText) => "8",
            };
            cmd.args([
                "-preset",
                preset,
                "-tune",
                tune,
                "-rc",
                rc,
                "-zerolatency",
                "1",
                "-strict_gop",
                "1",
                "-bf",
                "0",
                "-delay",
                "0",
                "-rc-lookahead",
                rc_lookahead,
                "-aud",
                "1",
            ]);
            if matches!(latency, EncoderLatencyMode::UltraLow) {
                cmd.args(["-surfaces", "2"]);
            } else if !matches!(quality, EncoderQualityMode::Fast) {
                let aq_strength = match quality {
                    EncoderQualityMode::Fast => "6",
                    EncoderQualityMode::Balanced => "8",
                    EncoderQualityMode::SharpText => "12",
                };
                cmd.args([
                    "-spatial-aq",
                    "1",
                    "-temporal-aq",
                    "1",
                    "-aq-strength",
                    aq_strength,
                ]);
            }
        }
        "libx264" => {
            let preset = match (latency, quality) {
                (EncoderLatencyMode::UltraLow, _) | (_, EncoderQualityMode::Fast) => "ultrafast",
                (EncoderLatencyMode::Low, EncoderQualityMode::Balanced) => "veryfast",
                (EncoderLatencyMode::Balanced, EncoderQualityMode::Balanced)
                | (EncoderLatencyMode::Low, EncoderQualityMode::SharpText) => "faster",
                (EncoderLatencyMode::Balanced, EncoderQualityMode::SharpText) => "fast",
            };
            let params = match latency {
                EncoderLatencyMode::UltraLow => {
                    "repeat-headers=1:aud=1:scenecut=0:sliced-threads=1"
                }
                EncoderLatencyMode::Low | EncoderLatencyMode::Balanced => "repeat-headers=1:aud=1",
            };
            cmd.args([
                "-preset",
                preset,
                "-tune",
                "zerolatency",
                "-bf",
                "0",
                "-x264-params",
                params,
            ]);
        }
        "libx265" => {
            let preset = match (latency, quality) {
                (EncoderLatencyMode::UltraLow, _) | (_, EncoderQualityMode::Fast) => "ultrafast",
                (EncoderLatencyMode::Low, EncoderQualityMode::Balanced) => "veryfast",
                (EncoderLatencyMode::Balanced, EncoderQualityMode::Balanced)
                | (EncoderLatencyMode::Low, EncoderQualityMode::SharpText) => "faster",
                (EncoderLatencyMode::Balanced, EncoderQualityMode::SharpText) => "fast",
            };
            cmd.args([
                "-preset",
                preset,
                "-tune",
                "zerolatency",
                "-bf",
                "0",
                "-x265-params",
                "repeat-headers=1:aud=1",
            ]);
        }
        "h264_qsv" | "hevc_qsv" | "vp9_qsv" | "av1_qsv" => {
            let preset = match (latency, quality) {
                (EncoderLatencyMode::UltraLow, _) | (_, EncoderQualityMode::Fast) => "veryfast",
                (EncoderLatencyMode::Low, EncoderQualityMode::Balanced) => "faster",
                (EncoderLatencyMode::Balanced, EncoderQualityMode::Balanced)
                | (_, EncoderQualityMode::SharpText) => "fast",
            };
            cmd.args(["-preset", preset, "-async_depth", "1", "-bf", "0"]);
            if matches!(quality, EncoderQualityMode::Fast) {
                cmd.args(["-look_ahead", "0"]);
            } else {
                cmd.args(["-look_ahead", "1"]);
            }
            if matches!(latency, EncoderLatencyMode::UltraLow) {
                cmd.args(["-low_power", "1", "-low_delay_brc", "1"]);
            }
        }
        "h264_vaapi" | "hevc_vaapi" | "vp9_vaapi" | "av1_vaapi" => {
            cmd.args(["-bf", "0"]);
            match (latency, quality) {
                (EncoderLatencyMode::UltraLow, _) | (_, EncoderQualityMode::Fast) => {
                    cmd.args(["-low_power", "1", "-quality", "7"]);
                }
                (EncoderLatencyMode::Low, EncoderQualityMode::Balanced) => {
                    cmd.args(["-quality", "6"]);
                }
                (EncoderLatencyMode::Balanced, EncoderQualityMode::Balanced) => {
                    cmd.args(["-quality", "4"]);
                }
                (_, EncoderQualityMode::SharpText) => {
                    cmd.args(["-quality", "2"]);
                }
            };
            if matches!(encoder, "h264_vaapi" | "hevc_vaapi") {
                cmd.args(["-aud", "1"]);
            }
        }
        "libvpx" => {
            let cpu_used = if matches!(quality, EncoderQualityMode::SharpText) {
                "6"
            } else {
                "8"
            };
            cmd.args(["-deadline", "realtime", "-cpu-used", cpu_used]);
        }
        "libvpx-vp9" => {
            let cpu_used = if matches!(quality, EncoderQualityMode::SharpText) {
                "6"
            } else {
                "8"
            };
            cmd.args([
                "-deadline",
                "realtime",
                "-cpu-used",
                cpu_used,
                "-row-mt",
                "1",
            ]);
        }
        "libsvtav1" => {
            cmd.args(["-preset", "12", "-svtav1-params", "scd=0:lookahead=0"]);
        }
        "libaom-av1" => {
            cmd.args([
                "-cpu-used",
                "8",
                "-usage",
                "realtime",
                "-row-mt",
                "1",
                "-tiles",
                "2x1",
            ]);
        }
        _ => {}
    }
}

fn append_bitstream_filter_args(cmd: &mut Command, output_format: &str) {
    match output_format {
        // The Annex B parser splits access units on AUD NALs. QSV and other
        // hardware encoders do not consistently emit them unless requested.
        "h264" => cmd.args(["-bsf:v", "h264_metadata=aud=insert"]),
        "hevc" => cmd.args(["-bsf:v", "hevc_metadata=aud=insert"]),
        _ => cmd,
    };
}

fn encoder_backend(encoder: &str) -> EncoderBackend {
    match encoder {
        "h264_nvenc" | "hevc_nvenc" | "av1_nvenc" => EncoderBackend::NvidiaNvenc,
        "h264_qsv" | "hevc_qsv" | "vp9_qsv" | "av1_qsv" => EncoderBackend::IntelQsv,
        "h264_vaapi" | "hevc_vaapi" | "vp9_vaapi" | "av1_vaapi" => EncoderBackend::Vaapi,
        _ => EncoderBackend::Cpu,
    }
}

fn render_device_path() -> Result<PathBuf> {
    let entries = fs::read_dir("/dev/dri").context("failed to inspect /dev/dri")?;
    let mut devices = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("renderD"))
        })
        .collect::<Vec<_>>();
    devices.sort();
    devices
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("no GPU render device found under /dev/dri"))
}

fn video_gop_frames(fps: u32, gop_ms: u32) -> u32 {
    fps.saturating_mul(gop_ms).saturating_add(999) / 1_000
}

fn video_maxrate_kbps(bitrate_kbps: u32, quality: EncoderQualityMode) -> u32 {
    let (num, den) = match quality {
        EncoderQualityMode::Fast => (1, 1),
        EncoderQualityMode::Balanced => (3, 2),
        EncoderQualityMode::SharpText => (5, 2),
    };
    bitrate_kbps.saturating_mul(num).saturating_add(den - 1) / den
}

fn video_buffer_size(bitrate_kbps: u32, buffer_ms: u32) -> String {
    let buffer_kbits = ((bitrate_kbps as u64)
        .saturating_mul(buffer_ms as u64)
        .saturating_add(999)
        / 1_000)
        .max(1);
    format!("{buffer_kbits}k")
}

pub async fn spawn_audio_capture(
    server: &ServerConfig,
    config: &AudioStreamConfig,
) -> Result<tokio::process::Child> {
    let source = ensure_pulse_monitor_source(server).await?;
    let bitrate = format!("{}k", config.bitrate_kbps);
    let mut cmd = Command::new("ffmpeg");
    cmd.env("DISPLAY", &server.display)
        .args([
            "-loglevel",
            "error",
            "-probesize",
            "32",
            "-analyzeduration",
            "0",
            "-fflags",
            "nobuffer",
            "-avioflags",
            "direct",
            "-flush_packets",
            "1",
            "-max_delay",
            "0",
            "-flags",
            "low_delay",
            "-f",
            "pulse",
            "-sample_rate",
            "48000",
            "-channels",
            "2",
            "-frame_size",
            "1024",
            "-fragment_size",
            "4096",
            "-i",
            &source,
            "-vn",
            "-sn",
            "-ac",
            "2",
            "-ar",
            "48000",
            "-c:a",
            "aac",
            "-profile:a",
            "aac_low",
            "-b:a",
            &bitrate,
            "-f",
            "adts",
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd.spawn().context("failed to spawn ffmpeg audio capture")
}

/// Spawns an ffmpeg process that captures the desktop audio monitor and encodes
/// it as a low-latency Ogg-Opus bitstream on stdout. Used by the WebRTC media
/// transport when the client opts to carry audio as a native Opus RTP track:
/// WebRTC media tracks must be Opus, but the shared capture above is AAC for the
/// WebCodecs clients, so this is a separate, on-demand encoder.
pub async fn spawn_opus_audio_capture(
    server: &ServerConfig,
    config: &AudioStreamConfig,
) -> Result<tokio::process::Child> {
    let source = ensure_pulse_monitor_source(server).await?;
    let bitrate = format!("{}k", config.bitrate_kbps);
    let mut cmd = Command::new("ffmpeg");
    cmd.env("DISPLAY", &server.display)
        .args([
            "-loglevel",
            "error",
            "-probesize",
            "32",
            "-analyzeduration",
            "0",
            "-fflags",
            "nobuffer",
            "-avioflags",
            "direct",
            "-flush_packets",
            "1",
            "-max_delay",
            "0",
            "-flags",
            "low_delay",
            "-f",
            "pulse",
            "-sample_rate",
            "48000",
            "-channels",
            "2",
            "-frame_size",
            "1024",
            "-fragment_size",
            "4096",
            "-i",
            &source,
            "-vn",
            "-sn",
            "-ac",
            "2",
            "-ar",
            "48000",
            "-c:a",
            "libopus",
            "-application",
            "lowdelay",
            "-frame_duration",
            "20",
            "-vbr",
            "off",
            "-b:a",
            &bitrate,
            "-f",
            "opus",
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd.spawn()
        .context("failed to spawn ffmpeg opus audio capture")
}

pub async fn warm_audio_stack(server: &ServerConfig) -> Result<()> {
    ensure_pulse_server().await?;
    ensure_virtual_sink(server).await?;
    ensure_virtual_mic_source().await?;
    Ok(())
}

pub async fn ensure_virtual_camera_device() -> Result<String> {
    if let Some(device) = find_virtual_camera_device()? {
        return Ok(device);
    }

    let output = Command::new("modprobe")
        .args([
            "v4l2loopback",
            "devices=1",
            "exclusive_caps=1",
            &format!("card_label={VIRTUAL_CAMERA_LABEL}"),
            &format!("video_nr={VIRTUAL_CAMERA_NR}"),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .context("failed to run modprobe for v4l2loopback")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "failed to create virtual camera {VIRTUAL_CAMERA_LABEL}: {}",
            stderr.trim()
        ));
    }

    wait_for_virtual_camera_device().await
}

pub fn spawn_virtual_camera_relay(device: &str) -> Result<VirtualCameraRelayHandle> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args([
        "-loglevel",
        "error",
        "-re",
        "-fflags",
        "nobuffer",
        "-avioflags",
        "direct",
        "-i",
        "pipe:0",
        "-map",
        "0:v:0",
        "-an",
        "-sn",
        "-pix_fmt",
        "yuv420p",
        "-f",
        "v4l2",
        device,
    ])
    .stdin(Stdio::piped())
    .stdout(Stdio::null())
    .stderr(Stdio::piped())
    .kill_on_drop(true);

    let mut child = cmd
        .spawn()
        .with_context(|| format!("failed to spawn ffmpeg virtual camera relay for {device}"))?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("ffmpeg virtual camera relay stdin missing"))?;
    Ok(VirtualCameraRelayHandle { child, stdin })
}

pub fn spawn_virtual_camera_placeholder(device: &str) -> Result<VirtualCameraPlaceholderHandle> {
    let mut cmd = Command::new("ffmpeg");
    cmd.args([
        "-loglevel",
        "error",
        "-re",
        "-f",
        "lavfi",
        "-i",
        "color=c=black:size=1280x720:rate=15",
        "-pix_fmt",
        "yuv420p",
        "-f",
        "v4l2",
        device,
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::piped())
    .kill_on_drop(true);

    let child = cmd.spawn().with_context(|| {
        format!("failed to spawn ffmpeg virtual camera placeholder for {device}")
    })?;
    Ok(VirtualCameraPlaceholderHandle { child })
}

pub async fn refresh_virtual_camera_desktop_services(device: &str) {
    let Some(device_name) = std::path::Path::new(device).file_name() else {
        warn!("virtual camera desktop refresh skipped for invalid device path {device}");
        return;
    };
    let sys_path = format!("/sys/class/video4linux/{}", device_name.to_string_lossy());

    match trigger_virtual_camera_udev_change(&sys_path).await {
        Ok(()) => {}
        Err(err) => {
            warn!(
                "virtual camera udev refresh failed for {device}: {err}; PipeWire apps may need a manual `sudo udevadm trigger --action=change {sys_path}`"
            );
            return;
        }
    }

    match Command::new("systemctl")
        .args(["--user", "restart", "wireplumber"])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
    {
        Ok(output) if output.status.success() => {}
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!(
                "WirePlumber refresh failed with {}: {}; restart WirePlumber or reopen camera apps if the virtual camera is missing",
                output.status,
                stderr.trim()
            );
        }
        Err(err) => {
            warn!("failed to refresh WirePlumber after virtual camera activation: {err}");
        }
    }
}

async fn trigger_virtual_camera_udev_change(sys_path: &str) -> Result<()> {
    let direct = Command::new("udevadm")
        .args(["trigger", "--action=change", sys_path])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await;
    match direct {
        Ok(output) if output.status.success() => return Ok(()),
        Ok(output) if !permission_denied_stderr(&output.stderr) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!(
                "udevadm trigger exited with {}: {}",
                output.status,
                stderr.trim()
            ));
        }
        Ok(_) | Err(_) => {}
    }

    let output = Command::new("sudo")
        .args(["-n", "udevadm", "trigger", "--action=change", sys_path])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .context("failed to run sudo udevadm trigger")?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(anyhow!(
        "sudo udevadm trigger exited with {}: {}",
        output.status,
        stderr.trim()
    ))
}

fn permission_denied_stderr(stderr: &[u8]) -> bool {
    String::from_utf8_lossy(stderr)
        .to_ascii_lowercase()
        .contains("permission denied")
}

pub async fn spawn_mic_input_injector(server: &ServerConfig) -> Result<MicInputHandle> {
    ensure_virtual_mic_source().await?;
    let mic_sink_name = virtual_mic_sink_name();
    let mut cmd = Command::new("ffmpeg");
    cmd.env("DISPLAY", &server.display)
        .args([
            "-loglevel",
            "error",
            "-fflags",
            "nobuffer",
            "-probesize",
            "32",
            "-analyzeduration",
            "0",
            "-f",
            "webm",
            "-i",
            "pipe:0",
            "-vn",
            "-sn",
            "-ac",
            "1",
            "-ar",
            "48000",
            "-c:a",
            "pcm_s16le",
            "-f",
            "pulse",
            "-buffer_duration",
            "15",
            "-buffer_size",
            "2048",
            "-prebuf",
            "0",
            "-minreq",
            "0",
            "-device",
            &mic_sink_name,
            "-stream_name",
            "VibeRDesk Mic Input",
            "vibe_rdesk_mic_input",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = cmd
        .spawn()
        .context("failed to spawn ffmpeg microphone injector")?;
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("ffmpeg microphone injector stdin missing"))?;
    Ok(MicInputHandle { child, stdin })
}

async fn ffmpeg_list_encoders() -> Result<String> {
    let output = Command::new("ffmpeg")
        .arg("-encoders")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .context("failed to list ffmpeg encoders")?;
    if !output.status.success() {
        return Err(anyhow!("ffmpeg -encoders exited with {}", output.status));
    }
    String::from_utf8(output.stdout).context("ffmpeg encoder list was not utf-8")
}

pub async fn run_xdotool<I, S>(display: &str, args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let status = Command::new("xdotool")
        .env("DISPLAY", display)
        .args(args)
        .status()
        .await
        .context("failed to run xdotool")?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("xdotool exited with {}", status))
    }
}

pub async fn wake_display(display: &str) -> Result<()> {
    let mut woke = false;
    let mut errors = Vec::new();

    woke |= wake_display_once(display, &mut errors).await;
    sleep(DISPLAY_WAKE_RETRY_DELAY).await;
    woke |= wake_display_once(display, &mut errors).await;

    if woke {
        Ok(())
    } else {
        Err(anyhow!(
            "all display wake attempts failed: {}",
            errors.join("; ")
        ))
    }
}

async fn run_xset<I, S>(display: &str, args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let status = Command::new("xset")
        .env("DISPLAY", display)
        .args(args)
        .status()
        .await
        .context("failed to run xset")?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("xset exited with {}", status))
    }
}

async fn wake_display_once(display: &str, errors: &mut Vec<String>) -> bool {
    let mut woke = false;
    let headless_display = std::env::var_os("VIBE_RDESK_HEADLESS_DISPLAY_ACTIVE").is_some();

    record_wake_result(
        "xset s reset",
        run_xset(display, ["s", "reset"]).await,
        &mut woke,
        errors,
    );
    if !headless_display {
        record_wake_result(
            "xset dpms force on",
            run_xset(display, ["dpms", "force", "on"]).await,
            &mut woke,
            errors,
        );
        record_wake_result(
            "dbus-send org.freedesktop.ScreenSaver.SimulateUserActivity",
            run_dbus_send(
                display,
                [
                    "--session",
                    "--type=method_call",
                    "--dest=org.freedesktop.ScreenSaver",
                    "/ScreenSaver",
                    "org.freedesktop.ScreenSaver.SimulateUserActivity",
                ],
            )
            .await,
            &mut woke,
            errors,
        );
        record_wake_result(
            "dbus-send org.gnome.ScreenSaver.SimulateUserActivity",
            run_dbus_send(
                display,
                [
                    "--session",
                    "--type=method_call",
                    "--dest=org.gnome.ScreenSaver",
                    "/org/gnome/ScreenSaver",
                    "org.gnome.ScreenSaver.SimulateUserActivity",
                ],
            )
            .await,
            &mut woke,
            errors,
        );
    }
    record_wake_result(
        "xdotool pointer wiggle",
        wiggle_pointer(display).await,
        &mut woke,
        errors,
    );
    woke
}

fn record_wake_result(step: &str, result: Result<()>, woke: &mut bool, errors: &mut Vec<String>) {
    match result {
        Ok(()) => *woke = true,
        Err(err) => {
            warn!(step, "display wake step failed: {err}");
            errors.push(format!("{step}: {err}"));
        }
    }
}

async fn run_dbus_send<I, S>(display: &str, args: I) -> Result<()>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let status = Command::new("dbus-send")
        .env("DISPLAY", display)
        .args(args)
        .status()
        .await
        .context("failed to run dbus-send")?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow!("dbus-send exited with {}", status))
    }
}

async fn wiggle_pointer(display: &str) -> Result<()> {
    run_xdotool(display, ["mousemove_relative", "--sync", "--", "1", "0"]).await?;
    run_xdotool(display, ["mousemove_relative", "--sync", "--", "-1", "0"]).await?;
    Ok(())
}

pub async fn read_stderr(child: &mut tokio::process::Child) -> String {
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        let _ = pipe.read_to_string(&mut stderr).await;
    }
    stderr
}

async fn ensure_pulse_monitor_source(server: &ServerConfig) -> Result<String> {
    if let Ok(source) = std::env::var("VIBE_RDESK_AUDIO_SOURCE") {
        let trimmed = source.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }
    let _ = ensure_pulse_server().await;
    if let Some(source) = default_monitor_source().await? {
        return Ok(source);
    }
    ensure_virtual_sink(server).await
}

pub fn project_virtual_audio_sink_name(server: &ServerConfig) -> String {
    std::env::var("VIBE_RDESK_AUDIO_SINK")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| format!("vibe_rdesk_{}", server.display.replace([':', '.'], "_")))
}

pub async fn list_audio_output_devices(server: &ServerConfig) -> Result<Vec<AudioOutputDevice>> {
    ensure_pulse_server().await?;
    ensure_virtual_sink_exists(server).await?;
    let virtual_sink = project_virtual_audio_sink_name(server);
    let default_sink = pactl(["get-default-sink"])
        .await
        .unwrap_or_default()
        .trim()
        .to_string();
    let sinks = pactl(["list", "short", "sinks"]).await?;
    Ok(sinks
        .lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let _index = parts.next()?;
            let name = parts.next()?.to_string();
            let is_virtual = is_project_virtual_sink(&name, &virtual_sink);
            Some(AudioOutputDevice {
                description: name.clone(),
                is_default: name == default_sink,
                is_virtual,
                name,
            })
        })
        .collect())
}

pub async fn set_audio_output_device(
    server: &ServerConfig,
    use_real_device: bool,
    requested_sink: Option<&str>,
) -> Result<AudioOutputDevice> {
    ensure_pulse_server().await?;
    ensure_virtual_sink_exists(server).await?;
    let virtual_sink = project_virtual_audio_sink_name(server);
    let devices = list_audio_output_devices(server).await?;
    let target = if use_real_device {
        let requested = requested_sink
            .map(str::trim)
            .filter(|sink| !sink.is_empty());
        devices
            .iter()
            .find(|device| !device.is_virtual && requested.is_some_and(|sink| sink == device.name))
            .or_else(|| devices.iter().find(|device| !device.is_virtual))
            .ok_or_else(|| anyhow!("no real audio output devices are available"))?
            .name
            .clone()
    } else {
        virtual_sink
    };
    pactl(["set-default-sink", &target]).await?;
    move_sink_inputs(&target).await?;
    let devices = list_audio_output_devices(server).await?;
    devices
        .into_iter()
        .find(|device| device.name == target)
        .ok_or_else(|| anyhow!("audio output sink {target} disappeared after switching"))
}

fn find_virtual_camera_device() -> Result<Option<String>> {
    let dir = match std::fs::read_dir("/sys/class/video4linux") {
        Ok(dir) => dir,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err).context("failed to inspect /sys/class/video4linux"),
    };

    for entry in dir {
        let entry = entry.context("failed to read video4linux entry")?;
        let name = std::fs::read_to_string(entry.path().join("name")).unwrap_or_default();
        if !is_virtual_camera_label(name.trim()) {
            continue;
        }
        let device_name = entry.file_name();
        return Ok(Some(format!("/dev/{}", device_name.to_string_lossy())));
    }

    Ok(None)
}

fn is_virtual_camera_label(label: &str) -> bool {
    label == VIRTUAL_CAMERA_LABEL || LEGACY_VIRTUAL_CAMERA_LABELS.contains(&label)
}

async fn wait_for_virtual_camera_device() -> Result<String> {
    for _ in 0..20 {
        if let Some(device) = find_virtual_camera_device()? {
            return Ok(device);
        }
        sleep(Duration::from_millis(150)).await;
    }
    Err(anyhow!(
        "virtual camera {VIRTUAL_CAMERA_LABEL} did not appear; ensure v4l2loopback is installed"
    ))
}

async fn default_monitor_source() -> Result<Option<String>> {
    let sink = match pactl(["get-default-sink"]).await {
        Ok(sink) => sink,
        Err(_) => return Ok(None),
    };
    let sink = sink.trim();
    if sink.is_empty() {
        return Ok(None);
    }
    Ok(Some(format!("{sink}.monitor")))
}

async fn ensure_virtual_sink(server: &ServerConfig) -> Result<String> {
    let (sink_name, monitor_source) = ensure_virtual_sink_exists(server).await?;
    pactl(["set-default-sink", &sink_name]).await?;
    move_sink_inputs(&sink_name).await?;
    Ok(monitor_source)
}

async fn ensure_virtual_sink_exists(server: &ServerConfig) -> Result<(String, String)> {
    let _ = ensure_pulse_server().await;
    let mut sink_name = project_virtual_audio_sink_name(server);
    if !sink_exists(&sink_name).await? {
        if let Some(existing_sink) = pulse_device_by_description("sinks", "VibeRDesk").await? {
            sink_name = existing_sink;
        } else {
        let _module_id = pactl([
            "load-module",
            "module-null-sink",
            &format!("sink_name={sink_name}"),
            "sink_properties=device.description=VibeRDesk",
        ])
        .await?;
        wait_for_sink(&sink_name).await?;
        }
    }
    Ok((sink_name.clone(), format!("{sink_name}.monitor")))
}

fn is_project_virtual_sink(sink_name: &str, virtual_sink: &str) -> bool {
    sink_name == virtual_sink
        || sink_name == VIRTUAL_MIC_SINK_NAME
        || sink_name == virtual_mic_sink_name()
        || sink_name.starts_with("vibe_rdesk_")
}

fn virtual_mic_sink_name() -> String {
    std::env::var("VIBE_RDESK_VIRTUAL_MIC_SINK_NAME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| VIRTUAL_MIC_SINK_NAME.into())
}

async fn ensure_virtual_mic_source() -> Result<String> {
    let _ = ensure_pulse_server().await;
    if source_exists(VIRTUAL_MIC_SOURCE_NAME).await? {
        return Ok(VIRTUAL_MIC_SOURCE_NAME.into());
    }

    if let Some(existing_source) = pulse_device_by_description("sources", VIRTUAL_MIC_SOURCE_NAME).await? {
        return Ok(existing_source);
    }

    let mut mic_sink_name = virtual_mic_sink_name();
    if !sink_exists(&mic_sink_name).await? {
        if let Some(existing_sink) =
            pulse_device_by_description("sinks", "VibeRDeskVirtualMicSink").await?
        {
            mic_sink_name = existing_sink;
        } else {
        pactl([
            "load-module",
            "module-null-sink",
            &format!("sink_name={mic_sink_name}"),
            "sink_properties=device.description=VibeRDeskVirtualMicSink",
        ])
        .await?;
        wait_for_sink(&mic_sink_name).await?;
        }
    }
    if !source_exists(VIRTUAL_MIC_SOURCE_NAME).await? {
        pactl([
            "load-module",
            "module-remap-source",
            &format!("source_name={VIRTUAL_MIC_SOURCE_NAME}"),
            &format!("master={mic_sink_name}.monitor"),
            &format!("source_properties=device.description={VIRTUAL_MIC_SOURCE_NAME}"),
        ])
        .await?;
        wait_for_source(VIRTUAL_MIC_SOURCE_NAME).await?;
    }
    Ok(VIRTUAL_MIC_SOURCE_NAME.into())
}

async fn ensure_pulse_server() -> Result<()> {
    if wait_for_pulse_server().await {
        return Ok(());
    }
    let _ = run_best_effort(
        "systemctl",
        &[
            "--user",
            "start",
            "pipewire",
            "pipewire-pulse",
            "wireplumber",
        ],
    )
    .await;
    if wait_for_pulse_server().await {
        return Ok(());
    }
    let _ = run_status_best_effort("pulseaudio", &["--check"]).await;
    let _ = run_status_best_effort(
        "pulseaudio",
        &["--start", "--daemonize=yes", "--exit-idle-time=-1"],
    )
    .await;
    if wait_for_pulse_server().await {
        return Ok(());
    }
    let _ = run_best_effort("pipewire", &[]).await;
    let _ = run_best_effort("pipewire-pulse", &[]).await;
    let _ = run_best_effort("wireplumber", &[]).await;
    if wait_for_pulse_server().await {
        return Ok(());
    }
    Err(anyhow!(
        "PulseAudio/PipeWire server is not running and could not be started"
    ))
}

async fn wait_for_pulse_server() -> bool {
    for _ in 0..12 {
        if pactl(["info"]).await.is_ok() {
            return true;
        }
        sleep(Duration::from_millis(250)).await;
    }
    false
}

async fn sink_exists(sink_name: &str) -> Result<bool> {
    let sinks = pactl(["list", "short", "sinks"]).await?;
    Ok(sinks
        .lines()
        .any(|line| line.split_whitespace().nth(1) == Some(sink_name)))
}

async fn source_exists(source_name: &str) -> Result<bool> {
    let sources = pactl(["list", "short", "sources"]).await?;
    Ok(sources
        .lines()
        .any(|line| line.split_whitespace().nth(1) == Some(source_name)))
}

async fn pulse_device_by_description(kind: &str, description: &str) -> Result<Option<String>> {
    let output = pactl(["list", kind]).await?;
    let mut current_name: Option<String> = None;

    for line in output.lines() {
        let trimmed = line.trim();
        if let Some(name) = trimmed.strip_prefix("Name: ") {
            current_name = Some(name.to_string());
            continue;
        }

        let Some(value) = trimmed.strip_prefix("device.description = \"") else {
            continue;
        };
        let Some(value) = value.strip_suffix('"') else {
            continue;
        };
        if value == description {
            return Ok(current_name);
        }
    }

    Ok(None)
}

async fn wait_for_sink(sink_name: &str) -> Result<()> {
    for _ in 0..10 {
        if sink_exists(sink_name).await? {
            return Ok(());
        }
        sleep(Duration::from_millis(150)).await;
    }
    Err(anyhow!("virtual sink {sink_name} did not appear"))
}

async fn wait_for_source(source_name: &str) -> Result<()> {
    for _ in 0..10 {
        if source_exists(source_name).await? {
            return Ok(());
        }
        sleep(Duration::from_millis(150)).await;
    }
    Err(anyhow!("virtual source {source_name} did not appear"))
}

async fn move_sink_inputs(sink_name: &str) -> Result<()> {
    let sink_inputs = pactl(["list", "short", "sink-inputs"])
        .await
        .unwrap_or_default();
    for line in sink_inputs.lines() {
        let Some(input_id) = line.split_whitespace().next() else {
            continue;
        };
        let _ = pactl(["move-sink-input", input_id, sink_name]).await;
    }
    Ok(())
}

async fn pactl<const N: usize>(args: [&str; N]) -> Result<String> {
    let output = Command::new("pactl")
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .context("failed to run pactl")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "pactl exited with {}: {}",
            output.status,
            stderr.trim()
        ));
    }
    String::from_utf8(output.stdout).context("pactl output was not utf-8")
}

async fn run_best_effort(program: &str, args: &[&str]) -> Result<()> {
    let _ = Command::new(program)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("failed to spawn {program}"))?;
    Ok(())
}

async fn run_status_best_effort(program: &str, args: &[&str]) -> Result<()> {
    let _ = Command::new(program)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .with_context(|| format!("failed to run {program}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::settings::{CodecKind, EncodePreference};
    use tokio::process::Command;

    #[tokio::test]
    async fn missing_encoder_is_not_reported_as_working() {
        assert!(
            !super::has_working_encoder(
                "",
                super::EncoderProfile {
                    ffmpeg_encoder: "h264_nvenc",
                    mode: "gpu",
                    output_format: "h264",
                    backend: super::EncoderBackend::NvidiaNvenc,
                },
            )
            .await
        );
    }

    #[test]
    fn h265_gpu_candidates_never_include_h264() {
        let candidates = super::preferred_encoders(CodecKind::H265, EncodePreference::Gpu);
        assert_eq!(candidates[0].ffmpeg_encoder, "hevc_nvenc");
        assert_eq!(candidates[1].ffmpeg_encoder, "hevc_qsv");
        assert!(
            candidates
                .iter()
                .all(|candidate| !candidate.ffmpeg_encoder.contains("264"))
        );
    }

    #[test]
    fn cpu_preference_skips_gpu_encoders() {
        let candidates = super::preferred_encoders(CodecKind::H264, EncodePreference::Cpu);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].ffmpeg_encoder, "libx264");
        assert_eq!(candidates[0].mode, "cpu");
    }

    #[test]
    fn nvidia_preference_uses_nvenc_for_h264() {
        let candidates = super::preferred_encoders(CodecKind::H264, EncodePreference::Nvidia);
        assert_eq!(candidates[0].ffmpeg_encoder, "h264_nvenc");
        assert!(candidates.len() > 1);
    }

    #[test]
    fn nvidia_preference_uses_nvenc_for_h265() {
        let candidates = super::preferred_encoders(CodecKind::H265, EncodePreference::Nvidia);
        assert_eq!(candidates[0].ffmpeg_encoder, "hevc_nvenc");
        assert!(candidates.len() > 1);
    }

    #[test]
    fn specific_encoder_preference_uses_exact_encoder() {
        let candidates = super::preferred_encoders(CodecKind::H264, EncodePreference::H264Qsv);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].ffmpeg_encoder, "h264_qsv");
    }

    #[test]
    fn vp9_cpu_preference_uses_libvpx_vp9() {
        let candidates = super::preferred_encoders(CodecKind::Vp9, EncodePreference::Cpu);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].ffmpeg_encoder, "libvpx-vp9");
    }

    #[test]
    fn av1_nvidia_preference_uses_av1_nvenc_first() {
        let candidates = super::preferred_encoders(CodecKind::Av1, EncodePreference::Nvidia);
        assert_eq!(candidates[0].ffmpeg_encoder, "av1_nvenc");
        assert!(candidates.len() > 1);
    }

    #[test]
    fn h264_output_inserts_aud_bitstream_filter() {
        let mut cmd = Command::new("ffmpeg");
        super::append_bitstream_filter_args(&mut cmd, "h264");
        let args = cmd.as_std().get_args().collect::<Vec<_>>();
        assert_eq!(args, vec!["-bsf:v", "h264_metadata=aud=insert"]);
    }

    #[test]
    fn vp8_output_does_not_add_bitstream_filter() {
        let mut cmd = Command::new("ffmpeg");
        super::append_bitstream_filter_args(&mut cmd, "ivf");
        assert!(cmd.as_std().get_args().next().is_none());
    }

    #[test]
    fn video_gop_tracks_configured_milliseconds() {
        assert_eq!(super::video_gop_frames(1, 2_000), 2);
        assert_eq!(super::video_gop_frames(16, 1_000), 16);
        assert_eq!(super::video_gop_frames(60, 500), 30);
    }

    #[test]
    fn video_buffer_size_tracks_configured_milliseconds() {
        assert_eq!(super::video_buffer_size(2_000, 500), "1000k");
        assert_eq!(super::video_buffer_size(4_000, 100), "400k");
    }
}
