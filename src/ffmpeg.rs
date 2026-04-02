use std::{ffi::OsStr, process::Stdio};

use anyhow::{Context, Result, anyhow};
use tokio::{
    io::AsyncReadExt,
    process::Command,
    time::{Duration, sleep},
};

use crate::settings::{CodecKind, ServerConfig, StreamConfig};

#[derive(Debug, Clone)]
pub struct EncoderChoice {
    pub ffmpeg_encoder: String,
    pub mode: &'static str,
    pub output_format: &'static str,
}

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
    ensure_pulse_server().await
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

async fn wait_for_sink(sink_name: &str) -> Result<()> {
    for _ in 0..10 {
        if sink_exists(sink_name).await? {
            return Ok(());
        }
        sleep(Duration::from_millis(150)).await;
    }
    Err(anyhow!("virtual sink {sink_name} did not appear"))
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
