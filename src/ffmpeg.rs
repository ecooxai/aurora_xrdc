use std::{ffi::OsStr, path::Path, process::Stdio};

use anyhow::{Context, Result, anyhow};
use tokio::{
    io::AsyncReadExt,
    process::{ChildStdin, Command},
    time::{Duration, sleep},
};

use crate::settings::{CodecKind, ServerConfig, StreamConfig};

#[derive(Debug, Clone)]
pub struct EncoderChoice {
    pub ffmpeg_encoder: String,
    pub mode: &'static str,
    pub output_format: &'static str,
}

pub struct MicInputHandle {
    pub child: tokio::process::Child,
    pub stdin: ChildStdin,
}

const VIRTUAL_MIC_SOURCE_NAME: &str = "Viberdeskmic";
const VIRTUAL_MIC_SINK_NAME: &str = "vibe_rdesk_virtual_mic_sink";
const VIRTUAL_CAMERA_LABEL: &str = "viberdeskcamera";
const VIRTUAL_CAMERA_NR: &str = "42";

pub async fn choose_encoder(codec: CodecKind) -> Result<EncoderChoice> {
    let encoders = ffmpeg_list_encoders().await?;
    let choice = match codec {
        CodecKind::H264 if has_working_encoder(&encoders, "h264_nvenc").await => EncoderChoice {
            ffmpeg_encoder: "h264_nvenc".into(),
            mode: "gpu",
            output_format: "h264",
        },
        CodecKind::H265 if has_working_encoder(&encoders, "hevc_nvenc").await => EncoderChoice {
            ffmpeg_encoder: "hevc_nvenc".into(),
            mode: "gpu",
            output_format: "hevc",
        },
        CodecKind::H264 => EncoderChoice {
            ffmpeg_encoder: "libx264".into(),
            mode: "cpu",
            output_format: "h264",
        },
        CodecKind::H265 => EncoderChoice {
            ffmpeg_encoder: "libx265".into(),
            mode: "cpu",
            output_format: "hevc",
        },
        CodecKind::Vp8 => EncoderChoice {
            ffmpeg_encoder: "libvpx".into(),
            mode: "cpu",
            output_format: "ivf",
        },
    };
    Ok(choice)
}

async fn has_working_encoder(encoders: &str, encoder: &str) -> bool {
    if !encoders.contains(&format!(" {encoder} ")) {
        return false;
    }
    ffmpeg_probe_encoder(encoder).await.unwrap_or(false)
}

async fn ffmpeg_probe_encoder(encoder: &str) -> Result<bool> {
    let output = Command::new("ffmpeg")
        .args([
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=size=16x16:rate=1:color=black",
            "-frames:v",
            "1",
            "-an",
            "-sn",
            "-c:v",
            encoder,
            "-f",
            "null",
            "-",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .with_context(|| format!("failed to probe ffmpeg encoder {encoder}"))?;
    Ok(output.status.success())
}

pub fn spawn_capture(
    server: &ServerConfig,
    stream: &StreamConfig,
    encoder: &EncoderChoice,
) -> Result<tokio::process::Child> {
    let bitrate = format!("{}k", stream.bitrate_kbps);
    let fps = stream.fps.to_string();
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
            "x11grab",
            "-framerate",
            &fps,
            "-i",
            &server.display,
            "-an",
            "-sn",
            "-pix_fmt",
            "yuv420p",
            "-preset",
            if encoder.mode == "gpu" {
                "p1"
            } else {
                "ultrafast"
            },
            "-tune",
            "zerolatency",
            "-b:v",
            &bitrate,
            "-maxrate",
            &bitrate,
            "-bufsize",
            &format!("{}k", stream.bitrate_kbps / 2),
            "-g",
            &fps,
            "-keyint_min",
            &fps,
            "-force_key_frames",
            "expr:gte(t,n_forced*2)",
            "-threads",
            "2",
            "-c:v",
            &encoder.ffmpeg_encoder,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    match stream.codec {
        CodecKind::H264 => {
            cmd.args(["-x264-params", "repeat-headers=1:aud=1"]);
            if encoder.mode == "gpu" {
                cmd.args(["-rc", "cbr_ld_hq", "-tune", "ll"]);
            }
        }
        CodecKind::H265 => {
            cmd.args(["-x265-params", "repeat-headers=1:aud=1"]);
        }
        CodecKind::Vp8 => {
            cmd.args(["-deadline", "realtime", "-cpu-used", "8"]);
        }
    }
    cmd.args(["-f", encoder.output_format, "pipe:1"]);
    cmd.spawn().context("failed to spawn ffmpeg capture")
}

pub async fn spawn_audio_capture(server: &ServerConfig) -> Result<tokio::process::Child> {
    let source = ensure_pulse_monitor_source(server).await?;
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
            "128k",
            "-f",
            "adts",
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd.spawn().context("failed to spawn ffmpeg audio capture")
}

pub async fn warm_audio_stack() -> Result<()> {
    ensure_pulse_server().await?;
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

pub async fn replay_mp4_to_virtual_camera(path: &Path, device: &str) -> Result<()> {
    let output = Command::new("ffmpeg")
        .args(["-loglevel", "error", "-re", "-i"])
        .arg(path)
        .args([
            "-map", "0:v:0", "-an", "-sn", "-pix_fmt", "yuv420p", "-f", "v4l2", device,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .await
        .with_context(|| format!("failed to relay {} into {device}", path.display()))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    Err(anyhow!(
        "ffmpeg virtual camera relay exited with {}: {}",
        output.status,
        stderr.trim()
    ))
}

pub async fn spawn_mic_input_injector(
    server: &ServerConfig,
    sample_rate: u32,
    channels: u8,
) -> Result<MicInputHandle> {
    ensure_virtual_mic_source().await?;
    let mut cmd = Command::new("ffmpeg");
    let input_sample_rate = sample_rate.max(8_000).to_string();
    let input_channels = channels.clamp(1, 2).to_string();
    cmd.env("DISPLAY", &server.display)
        .args([
            "-loglevel",
            "error",
            "-fflags",
            "nobuffer",
            "-avioflags",
            "direct",
            "-f",
            "s16le",
            "-ar",
            &input_sample_rate,
            "-ac",
            &input_channels,
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
            VIRTUAL_MIC_SINK_NAME,
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

fn find_virtual_camera_device() -> Result<Option<String>> {
    let dir = match std::fs::read_dir("/sys/class/video4linux") {
        Ok(dir) => dir,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err).context("failed to inspect /sys/class/video4linux"),
    };

    for entry in dir {
        let entry = entry.context("failed to read video4linux entry")?;
        let name = std::fs::read_to_string(entry.path().join("name")).unwrap_or_default();
        if name.trim() != VIRTUAL_CAMERA_LABEL {
            continue;
        }
        let device_name = entry.file_name();
        return Ok(Some(format!("/dev/{}", device_name.to_string_lossy())));
    }

    Ok(None)
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
    let _ = ensure_pulse_server().await;
    let sink_name = std::env::var("VIBE_RDESK_AUDIO_SINK")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("vibe_rdesk_{}", server.display.replace([':', '.'], "_")));
    if !sink_exists(&sink_name).await? {
        let _module_id = pactl([
            "load-module",
            "module-null-sink",
            &format!("sink_name={sink_name}"),
            "sink_properties=device.description=VibeRDesk",
        ])
        .await?;
        wait_for_sink(&sink_name).await?;
    }
    pactl(["set-default-sink", &sink_name]).await?;
    move_sink_inputs(&sink_name).await?;
    Ok(format!("{sink_name}.monitor"))
}

async fn ensure_virtual_mic_source() -> Result<String> {
    let _ = ensure_pulse_server().await;
    if source_exists(VIRTUAL_MIC_SOURCE_NAME).await? {
        return Ok(VIRTUAL_MIC_SOURCE_NAME.into());
    }
    if !sink_exists(VIRTUAL_MIC_SINK_NAME).await? {
        pactl([
            "load-module",
            "module-null-sink",
            &format!("sink_name={VIRTUAL_MIC_SINK_NAME}"),
            "sink_properties=device.description=VibeRDeskVirtualMicSink",
        ])
        .await?;
        wait_for_sink(VIRTUAL_MIC_SINK_NAME).await?;
    }
    if !source_exists(VIRTUAL_MIC_SOURCE_NAME).await? {
        pactl([
            "load-module",
            "module-remap-source",
            &format!("source_name={VIRTUAL_MIC_SOURCE_NAME}"),
            &format!("master={VIRTUAL_MIC_SINK_NAME}.monitor"),
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
    #[tokio::test]
    async fn missing_encoder_is_not_reported_as_working() {
        assert!(!super::has_working_encoder("", "h264_nvenc").await);
    }
}
