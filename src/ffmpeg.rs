use std::{ffi::OsStr, process::Stdio};

use anyhow::{Context, Result, anyhow};
use tokio::{io::AsyncReadExt, process::Command};

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
            "-fflags",
            "nobuffer",
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
            if encoder.mode == "gpu" { "p1" } else { "ultrafast" },
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

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn missing_encoder_is_not_reported_as_working() {
        assert!(!super::has_working_encoder("", "h264_nvenc").await);
    }
}
