use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{ffi::OsString, path::PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CodecKind {
    H264,
    H265,
    Vp8,
    Vp9,
    Av1,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EncodePreference {
    #[default]
    Cpu,
    Gpu,
    #[serde(rename = "nvidia")]
    Nvidia,
    #[serde(rename = "h264_nvenc")]
    H264Nvenc,
    #[serde(rename = "h264_qsv")]
    H264Qsv,
    #[serde(rename = "h264_vaapi")]
    H264Vaapi,
    #[serde(rename = "libx264")]
    Libx264,
    #[serde(rename = "hevc_nvenc")]
    HevcNvenc,
    #[serde(rename = "hevc_qsv")]
    HevcQsv,
    #[serde(rename = "hevc_vaapi")]
    HevcVaapi,
    #[serde(rename = "libx265")]
    Libx265,
    #[serde(rename = "libvpx")]
    Libvpx,
    #[serde(rename = "vp9_qsv")]
    Vp9Qsv,
    #[serde(rename = "vp9_vaapi")]
    Vp9Vaapi,
    #[serde(rename = "libvpx-vp9")]
    LibvpxVp9,
    #[serde(rename = "av1_nvenc")]
    Av1Nvenc,
    #[serde(rename = "av1_qsv")]
    Av1Qsv,
    #[serde(rename = "av1_vaapi")]
    Av1Vaapi,
    #[serde(rename = "libsvtav1")]
    LibSvtAv1,
    #[serde(rename = "libaom-av1")]
    LibAomAv1,
}

impl EncodePreference {
    pub fn normalized_for_codec(self, codec: CodecKind) -> Self {
        match codec {
            CodecKind::H264 => match self {
                Self::Gpu
                | Self::Cpu
                | Self::Nvidia
                | Self::H264Nvenc
                | Self::H264Qsv
                | Self::H264Vaapi
                | Self::Libx264 => self,
                Self::Libx265
                | Self::Libvpx
                | Self::LibvpxVp9
                | Self::LibSvtAv1
                | Self::LibAomAv1 => Self::Cpu,
                Self::HevcNvenc
                | Self::HevcQsv
                | Self::HevcVaapi
                | Self::Vp9Qsv
                | Self::Vp9Vaapi
                | Self::Av1Nvenc
                | Self::Av1Qsv
                | Self::Av1Vaapi => Self::Gpu,
            },
            CodecKind::H265 => match self {
                Self::Gpu
                | Self::Cpu
                | Self::Nvidia
                | Self::HevcNvenc
                | Self::HevcQsv
                | Self::HevcVaapi
                | Self::Libx265 => self,
                Self::Libx264
                | Self::Libvpx
                | Self::LibvpxVp9
                | Self::LibSvtAv1
                | Self::LibAomAv1 => Self::Cpu,
                Self::H264Nvenc
                | Self::H264Qsv
                | Self::H264Vaapi
                | Self::Vp9Qsv
                | Self::Vp9Vaapi
                | Self::Av1Nvenc
                | Self::Av1Qsv
                | Self::Av1Vaapi => Self::Gpu,
            },
            CodecKind::Vp8 => match self {
                Self::Cpu | Self::Libvpx => self,
                _ => Self::Cpu,
            },
            CodecKind::Vp9 => match self {
                Self::Gpu | Self::Cpu | Self::Vp9Qsv | Self::Vp9Vaapi | Self::LibvpxVp9 => self,
                Self::Nvidia | Self::HevcNvenc | Self::H264Nvenc | Self::Av1Nvenc => Self::Gpu,
                _ => Self::Cpu,
            },
            CodecKind::Av1 => match self {
                Self::Gpu
                | Self::Cpu
                | Self::Nvidia
                | Self::Av1Nvenc
                | Self::Av1Qsv
                | Self::Av1Vaapi
                | Self::LibSvtAv1
                | Self::LibAomAv1 => self,
                Self::Libx264 | Self::Libx265 | Self::Libvpx | Self::LibvpxVp9 => Self::Cpu,
                Self::H264Nvenc
                | Self::H264Qsv
                | Self::H264Vaapi
                | Self::HevcNvenc
                | Self::HevcQsv
                | Self::HevcVaapi
                | Self::Vp9Qsv
                | Self::Vp9Vaapi => Self::Gpu,
            },
        }
    }
}

impl CodecKind {
    pub fn as_webcodec(self) -> &'static str {
        match self {
            Self::H264 => "avc1.64001f",
            Self::H265 => "hvc1.1.6.L93.B0",
            Self::Vp8 => "vp8",
            Self::Vp9 => "vp09.00.10.08",
            Self::Av1 => "av01.0.08M.08",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::H264 => "H.264",
            Self::H265 => "H.265",
            Self::Vp8 => "VP8",
            Self::Vp9 => "VP9",
            Self::Av1 => "AV1",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StreamConfig {
    pub codec: CodecKind,
    pub bitrate_kbps: u32,
    pub fps: u32,
    #[serde(default)]
    pub encode_preference: EncodePreference,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            codec: CodecKind::H264,
            bitrate_kbps: 2_000,
            fps: 23,
            encode_preference: EncodePreference::Cpu,
        }
    }
}

impl StreamConfig {
    pub fn normalized(mut self) -> Self {
        self.bitrate_kbps = self.bitrate_kbps.clamp(128, 25_000);
        self.fps = self.fps.clamp(1, 60);
        self.encode_preference = self.encode_preference.normalized_for_codec(self.codec);
        self
    }

    pub fn h264_cpu_fallback(&self) -> Self {
        Self {
            codec: CodecKind::H264,
            bitrate_kbps: self.bitrate_kbps,
            fps: self.fps,
            encode_preference: EncodePreference::Cpu,
        }
        .normalized()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AudioStreamConfig {
    pub bitrate_kbps: u32,
}

impl Default for AudioStreamConfig {
    fn default() -> Self {
        Self { bitrate_kbps: 128 }
    }
}

impl AudioStreamConfig {
    pub fn normalized(mut self) -> Self {
        self.bitrate_kbps = self.bitrate_kbps.clamp(16, 320);
        self
    }
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub bind: String,
    pub display: String,
    pub upload_dir: String,
    pub passwd: String,
}

impl ServerConfig {
    pub fn from_env() -> Self {
        Self::from_env_with(
            std::env::var_os("VIBE_RDESK_BIND"),
            std::env::var_os("DISPLAY"),
            std::env::var_os("VIBE_RDESK_UPLOAD_DIR"),
            std::env::var_os("HOME").map(PathBuf::from),
        )
    }

    fn from_env_with(
        bind: Option<OsString>,
        display: Option<OsString>,
        upload_dir: Option<OsString>,
        home_dir: Option<PathBuf>,
    ) -> Self {
        Self {
            bind: bind
                .and_then(|value| value.into_string().ok())
                .unwrap_or_else(|| default_bind(8001)),
            display: display
                .and_then(|value| value.into_string().ok())
                .unwrap_or_else(|| ":0.0".into()),
            upload_dir: resolve_upload_dir(upload_dir, home_dir)
                .to_string_lossy()
                .into_owned(),
            passwd: String::new(),
        }
    }

    pub fn from_args<I>(args: I) -> Result<Self>
    where
        I: IntoIterator,
        I::Item: Into<String>,
    {
        Self::from_args_with(args, Self::from_env())
    }

    fn from_args_with<I>(args: I, mut server: Self) -> Result<Self>
    where
        I: IntoIterator,
        I::Item: Into<String>,
    {
        let mut args = args.into_iter().map(Into::into);
        let _ = args.next();
        let mut passwd = None;
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "-p" | "--port" => {
                    let value = args.next().context("missing value for -p/--port")?;
                    let port = parse_port(&value)?;
                    server.bind = bind_with_port(&server.bind, port);
                }
                "--passwd" => {
                    let value = args.next().context("missing value for --passwd")?;
                    if value.trim().is_empty() {
                        anyhow::bail!("--passwd cannot be empty");
                    }
                    passwd = Some(value);
                }
                _ => {}
            }
        }
        server.passwd = passwd
            .context("missing required --passwd; start the server with --passwd <password>")?;
        Ok(server)
    }
}

fn resolve_upload_dir(upload_dir: Option<OsString>, home_dir: Option<PathBuf>) -> PathBuf {
    match upload_dir {
        Some(path) => expand_home_path(PathBuf::from(path), home_dir),
        None => default_upload_dir(home_dir),
    }
}

fn expand_home_path(path: PathBuf, home_dir: Option<PathBuf>) -> PathBuf {
    let Some(path_str) = path.to_str() else {
        return path;
    };
    match path_str {
        "~" => home_dir.unwrap_or(path),
        _ => match path_str.strip_prefix("~/") {
            Some(suffix) => match home_dir {
                Some(home_dir) => home_dir.join(suffix),
                None => path,
            },
            None => path,
        },
    }
}

fn default_upload_dir(home_dir: Option<PathBuf>) -> PathBuf {
    home_dir
        .map(|home_dir| home_dir.join("Desktop"))
        .unwrap_or_else(|| PathBuf::from("Desktop"))
}

fn parse_port(value: &str) -> Result<u16> {
    value
        .parse::<u16>()
        .with_context(|| format!("invalid port: {value}"))
}

fn bind_with_port(bind: &str, port: u16) -> String {
    let binds = split_bind_list(bind);
    if binds.is_empty() {
        return default_bind(port);
    }

    binds
        .into_iter()
        .map(|bind| match bind.rsplit_once(':') {
            Some((host, _)) if !host.is_empty() => format!("{host}:{port}"),
            _ => format!("0.0.0.0:{port}"),
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn default_bind(port: u16) -> String {
    format!("0.0.0.0:{port},[::]:{port}")
}

fn split_bind_list(bind: &str) -> Vec<&str> {
    bind.split(',')
        .map(str::trim)
        .filter(|bind| !bind.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{AudioStreamConfig, CodecKind, EncodePreference, ServerConfig, StreamConfig};
    use std::path::PathBuf;

    #[test]
    fn stream_config_clamps_values() {
        let cfg = StreamConfig {
            codec: CodecKind::H264,
            bitrate_kbps: 99,
            fps: 99,
            encode_preference: EncodePreference::Gpu,
        }
        .normalized();
        assert_eq!(cfg.bitrate_kbps, 128);
        assert_eq!(cfg.fps, 60);
    }

    #[test]
    fn stream_config_defaults_to_cpu_preference() {
        assert_eq!(
            StreamConfig::default().encode_preference,
            EncodePreference::Cpu
        );
    }

    #[test]
    fn invalid_specific_encoder_falls_back_for_codec() {
        let cfg = StreamConfig {
            codec: CodecKind::Vp8,
            bitrate_kbps: 4_000,
            fps: 16,
            encode_preference: EncodePreference::H264Qsv,
        }
        .normalized();
        assert_eq!(cfg.encode_preference, EncodePreference::Cpu);
    }

    #[test]
    fn nvidia_preference_is_preserved_for_supported_codecs() {
        let cfg = StreamConfig {
            codec: CodecKind::H264,
            bitrate_kbps: 4_000,
            fps: 16,
            encode_preference: EncodePreference::Nvidia,
        }
        .normalized();
        assert_eq!(cfg.encode_preference, EncodePreference::Nvidia);
    }

    #[test]
    fn nvidia_preference_falls_back_for_vp8() {
        let cfg = StreamConfig {
            codec: CodecKind::Vp8,
            bitrate_kbps: 4_000,
            fps: 16,
            encode_preference: EncodePreference::Nvidia,
        }
        .normalized();
        assert_eq!(cfg.encode_preference, EncodePreference::Cpu);
    }

    #[test]
    fn av1_keeps_explicit_av1_encoder_preference() {
        let cfg = StreamConfig {
            codec: CodecKind::Av1,
            bitrate_kbps: 4_000,
            fps: 16,
            encode_preference: EncodePreference::Av1Qsv,
        }
        .normalized();
        assert_eq!(cfg.encode_preference, EncodePreference::Av1Qsv);
    }

    #[test]
    fn h264_cpu_fallback_preserves_rate_settings() {
        let cfg = StreamConfig {
            codec: CodecKind::H265,
            bitrate_kbps: 6_000,
            fps: 24,
            encode_preference: EncodePreference::HevcQsv,
        }
        .h264_cpu_fallback();
        assert_eq!(cfg.codec, CodecKind::H264);
        assert_eq!(cfg.bitrate_kbps, 6_000);
        assert_eq!(cfg.fps, 24);
        assert_eq!(cfg.encode_preference, EncodePreference::Cpu);
    }

    #[test]
    fn audio_stream_config_clamps_values() {
        let cfg = AudioStreamConfig { bitrate_kbps: 999 }.normalized();
        assert_eq!(cfg.bitrate_kbps, 320);
        let cfg = AudioStreamConfig { bitrate_kbps: 1 }.normalized();
        assert_eq!(cfg.bitrate_kbps, 16);
    }

    #[test]
    fn server_config_defaults_to_port_8001() {
        let cfg = ServerConfig::from_env_with(
            None,
            None,
            None,
            Some(PathBuf::from("/tmp/vibe-rdesk-home")),
        );
        assert_eq!(cfg.bind, "0.0.0.0:8001,[::]:8001");
        assert_eq!(cfg.display, ":0.0");
        assert_eq!(cfg.upload_dir, "/tmp/vibe-rdesk-home/Desktop");
        assert!(cfg.passwd.is_empty());
    }

    #[test]
    fn server_config_applies_cli_port_override() {
        let cfg = ServerConfig::from_args_with(
            ["vibe_rdesk", "-p", "9000", "--passwd", "secret"],
            ServerConfig::from_env_with(None, None, None, None),
        )
        .unwrap();
        assert_eq!(cfg.bind, "0.0.0.0:9000,[::]:9000");
        assert_eq!(cfg.passwd, "secret");
    }

    #[test]
    fn server_config_requires_passwd_flag() {
        let err = ServerConfig::from_args_with(
            ["vibe_rdesk", "-p", "9000"],
            ServerConfig::from_env_with(None, None, None, None),
        )
        .unwrap_err();
        assert!(err.to_string().contains("--passwd"));
    }

    #[test]
    fn server_config_keeps_host_when_port_overridden() {
        let mut cfg =
            ServerConfig::from_env_with(Some("127.0.0.1:3000,[::1]:3000".into()), None, None, None);
        cfg.bind = super::bind_with_port(&cfg.bind, 8001);
        assert_eq!(cfg.bind, "127.0.0.1:8001,[::1]:8001");
    }

    #[test]
    fn server_config_expands_tilde_upload_dir() {
        let cfg = ServerConfig::from_env_with(
            None,
            None,
            Some("~/Desktop/uploads".into()),
            Some(PathBuf::from("/home/tester")),
        );
        assert_eq!(cfg.upload_dir, "/home/tester/Desktop/uploads");
    }
}
