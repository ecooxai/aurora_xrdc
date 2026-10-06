use std::{ffi::OsStr, process::Stdio};
use anyhow::{Context, Result, anyhow};
use serde::Serialize;
use tokio::{io::AsyncReadExt, process::{Child, ChildStdin, Command}, time::Duration};

use crate::settings::{AudioStreamConfig, CodecKind, EncodePreference, ServerConfig, StreamConfig};

fn ffmpeg_program() -> std::ffi::OsString {
    std::env::var_os("AURORA_FFMPEG").filter(|p| !p.is_empty()).unwrap_or_else(|| "ffmpeg".into())
}

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

pub struct MicInputHandle { pub child: Child, pub stdin: ChildStdin }
pub struct VirtualCameraRelayHandle { pub child: Child, pub stdin: ChildStdin }
pub struct VirtualCameraPlaceholderHandle { pub child: Child }

pub async fn choose_encoder(codec: CodecKind, pref: EncodePreference) -> Result<EncoderChoice> {
    let encoders = ffmpeg_list_encoders().await?;
    let candidates: &[(&str,&str,&str)] = match codec {
        CodecKind::H264 => &[("h264_videotoolbox","gpu","h264"),("libx264","cpu","h264")],
        CodecKind::H265 => &[("hevc_videotoolbox","gpu","hevc"),("libx265","cpu","hevc")],
        CodecKind::Vp8 => &[("libvpx","cpu","ivf")],
        CodecKind::Vp9 => &[("libvpx-vp9","cpu","ivf")],
        CodecKind::Av1 => &[("libaom-av1","cpu","ivf"),("libsvtav1","cpu","ivf")],
    };
    let want_cpu = matches!(pref, EncodePreference::Cpu | EncodePreference::Libx264 | EncodePreference::Libx265 | EncodePreference::Libvpx | EncodePreference::LibvpxVp9 | EncodePreference::LibAomAv1 | EncodePreference::LibSvtAv1);
    for (name, mode, fmt) in candidates {
        if want_cpu && *mode == "gpu" { continue; }
        if encoders.contains(&format!(" {} ", name)) {
            return Ok(EncoderChoice { ffmpeg_encoder:(*name).into(), mode, output_format:fmt });
        }
    }
    Err(anyhow!("no working ffmpeg encoder available for requested codec {codec:?}"))
}

pub async fn available_encoder_options(codec: CodecKind) -> Result<Vec<AvailableEncoderOption>> {
    let encoders = ffmpeg_list_encoders().await?;
    let mut out=Vec::new();
    match codec {
        CodecKind::H264 => {
            if encoders.contains(" h264_videotoolbox ") { out.push(AvailableEncoderOption{value:EncodePreference::Gpu,label:"h264_videotoolbox (GPU)".into(),mode:"gpu",ffmpeg_encoder:Some("h264_videotoolbox".into())}); }
            if encoders.contains(" libx264 ") { out.push(AvailableEncoderOption{value:EncodePreference::Cpu,label:"libx264 (CPU)".into(),mode:"cpu",ffmpeg_encoder:Some("libx264".into())}); }
        }
        CodecKind::H265 => {
            if encoders.contains(" hevc_videotoolbox ") { out.push(AvailableEncoderOption{value:EncodePreference::Gpu,label:"hevc_videotoolbox (GPU)".into(),mode:"gpu",ffmpeg_encoder:Some("hevc_videotoolbox".into())}); }
            if encoders.contains(" libx265 ") { out.push(AvailableEncoderOption{value:EncodePreference::Cpu,label:"libx265 (CPU)".into(),mode:"cpu",ffmpeg_encoder:Some("libx265".into())}); }
        }
        CodecKind::Vp8 => if encoders.contains(" libvpx ") { out.push(AvailableEncoderOption{value:EncodePreference::Cpu,label:"libvpx (CPU)".into(),mode:"cpu",ffmpeg_encoder:Some("libvpx".into())}); },
        CodecKind::Vp9 => if encoders.contains(" libvpx-vp9 ") { out.push(AvailableEncoderOption{value:EncodePreference::Cpu,label:"libvpx-vp9 (CPU)".into(),mode:"cpu",ffmpeg_encoder:Some("libvpx-vp9".into())}); },
        CodecKind::Av1 => if encoders.contains(" libaom-av1 ") { out.push(AvailableEncoderOption{value:EncodePreference::Cpu,label:"libaom-av1 (CPU)".into(),mode:"cpu",ffmpeg_encoder:Some("libaom-av1".into())}); },
    }
    Ok(out)
}

pub async fn available_codec_options() -> Result<Vec<AvailableCodecOption>> {
    let mut out=Vec::new();
    for codec in [CodecKind::H264,CodecKind::H265,CodecKind::Vp8,CodecKind::Vp9,CodecKind::Av1] {
        if !available_encoder_options(codec).await?.is_empty() { out.push(AvailableCodecOption{value:codec,label:codec.label()}); }
    }
    Ok(out)
}

pub fn spawn_capture(_server:&ServerConfig, stream:&StreamConfig, encoder:&EncoderChoice) -> Result<Child> {
    let fps=stream.fps.to_string();
    let device=std::env::var("AURORA_MAC_SCREEN_DEVICE").unwrap_or_else(|_| "1:none".into());
    let mut cmd=Command::new(ffmpeg_program());
    cmd.args(["-loglevel","error","-f","avfoundation","-capture_cursor","1","-framerate",&fps,"-i",&device,"-an","-sn","-c:v",&encoder.ffmpeg_encoder]);
    match encoder.ffmpeg_encoder.as_str() {
        "h264_videotoolbox"|"hevc_videotoolbox" => { cmd.args(["-realtime","1","-b:v",&format!("{}k",stream.bitrate_kbps)]); }
        _ => { cmd.args(["-preset","veryfast","-tune","zerolatency","-b:v",&format!("{}k",stream.bitrate_kbps)]); }
    }
    cmd.args(["-g",&(stream.fps.saturating_mul(2).max(1)).to_string(),"-pix_fmt","yuv420p","-f",encoder.output_format,"pipe:1"]);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).kill_on_drop(true);
    cmd.spawn().context("failed to spawn macOS ffmpeg screen capture")
}

pub async fn spawn_audio_capture(_server:&ServerConfig, _config:&AudioStreamConfig) -> Result<Child> {
    Err(anyhow!("system-audio capture is not yet implemented on macOS"))
}

pub async fn spawn_opus_audio_capture(_server:&ServerConfig, _config:&AudioStreamConfig) -> Result<Child> {\n    Err(anyhow!("system-audio Opus capture is not yet implemented on macOS"))\n}\n\npub async fn spawn_mic_input_injector(_server:&ServerConfig) -> Result<MicInputHandle> {
    Err(anyhow!("virtual microphone uplink is not yet implemented on macOS"))
}

pub async fn warm_audio_stack(_server:&ServerConfig) -> Result<()> { Ok(()) }
pub fn project_virtual_audio_sink_name(_server:&ServerConfig) -> String { "macos-system-audio".into() }
pub async fn list_audio_output_devices(_server:&ServerConfig) -> Result<Vec<AudioOutputDevice>> { Ok(Vec::new()) }
pub async fn set_audio_output_device(_server:&ServerConfig,_use_real_device:bool,_requested_sink:Option<&str>) -> Result<AudioOutputDevice> { Err(anyhow!("audio output switching is not yet implemented on macOS")) }

pub async fn ensure_virtual_camera_device() -> Result<String> { Err(anyhow!("virtual camera requires a signed macOS CoreMediaIO extension and is not available in this port yet")) }
pub fn spawn_virtual_camera_relay(_device:&str) -> Result<VirtualCameraRelayHandle> { Err(anyhow!("virtual camera unavailable on macOS")) }
pub fn spawn_virtual_camera_placeholder(_device:&str) -> Result<VirtualCameraPlaceholderHandle> { Err(anyhow!("virtual camera unavailable on macOS")) }
pub async fn refresh_virtual_camera_desktop_services(_device:&str) {}

pub async fn run_xdotool<I,S>(_display:&str,_args:I)->Result<()> where I:IntoIterator<Item=S>,S:AsRef<OsStr> {
    Err(anyhow!("xdotool is unavailable on macOS; CoreGraphics input backend should be used"))
}

pub async fn wake_display(_display:&str)->Result<()> {
    let status=Command::new("caffeinate").args(["-u","-t","1"]).status().await.context("failed to run caffeinate")?;
    if status.success(){Ok(())}else{Err(anyhow!("caffeinate exited with {status}"))}
}

pub async fn read_stderr(child:&mut Child)->String {
    let mut stderr=String::new();
    if let Some(mut pipe)=child.stderr.take(){ let _=pipe.read_to_string(&mut stderr).await; }
    stderr
}

async fn ffmpeg_list_encoders()->Result<String>{
    let output=Command::new(ffmpeg_program()).args(["-hide_banner","-encoders"]).output().await.context("failed to query ffmpeg encoders")?;
    if !output.status.success(){return Err(anyhow!("ffmpeg -encoders exited with {}",output.status));}
    String::from_utf8(output.stdout).context("ffmpeg encoder list was not utf-8")
}
