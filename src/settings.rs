use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CodecKind {
    H264,
    H265,
    Vp8,
}

impl CodecKind {
    pub fn as_webcodec(self) -> &'static str {
        match self {
            Self::H264 => "avc1.64001f",
            Self::H265 => "hvc1.1.6.L93.B0",
            Self::Vp8 => "vp8",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {
    pub codec: CodecKind,
    pub bitrate_kbps: u32,
    pub fps: u32,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            codec: CodecKind::H264,
            bitrate_kbps: 4_000,
            fps: 16,
        }
    }
}

impl StreamConfig {
    pub fn normalized(mut self) -> Self {
        self.bitrate_kbps = self.bitrate_kbps.clamp(250, 25_000);
        self.fps = self.fps.clamp(1, 60);
        self
    }
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub bind: String,
    pub display: String,
}

impl ServerConfig {
    pub fn from_env() -> Self {
        Self::from_env_with(std::env::var_os("VIBE_RDESK_BIND"), std::env::var_os("DISPLAY"))
    }

    fn from_env_with(bind: Option<std::ffi::OsString>, display: Option<std::ffi::OsString>) -> Self {
        Self {
            bind: bind
                .and_then(|value| value.into_string().ok())
                .unwrap_or_else(|| "0.0.0.0:8001".into()),
            display: display
                .and_then(|value| value.into_string().ok())
                .unwrap_or_else(|| ":0.0".into()),
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
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "-p" | "--port" => {
                    let value = args
                        .next()
                        .context("missing value for -p/--port")?;
                    let port = parse_port(&value)?;
                    server.bind = bind_with_port(&server.bind, port);
                }
                _ => {}
            }
        }
        Ok(server)
    }
}

fn parse_port(value: &str) -> Result<u16> {
    value
        .parse::<u16>()
        .with_context(|| format!("invalid port: {value}"))
}

fn bind_with_port(bind: &str, port: u16) -> String {
    match bind.rsplit_once(':') {
        Some((host, _)) if !host.is_empty() => format!("{host}:{port}"),
        _ => format!("0.0.0.0:{port}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{CodecKind, ServerConfig, StreamConfig};

    #[test]
    fn stream_config_clamps_values() {
        let cfg = StreamConfig {
            codec: CodecKind::H264,
            bitrate_kbps: 99,
            fps: 99,
        }
        .normalized();
        assert_eq!(cfg.bitrate_kbps, 250);
        assert_eq!(cfg.fps, 60);
    }

    #[test]
    fn server_config_defaults_to_port_8001() {
        let cfg = ServerConfig::from_env_with(None, None);
        assert_eq!(cfg.bind, "0.0.0.0:8001");
        assert_eq!(cfg.display, ":0.0");
    }

    #[test]
    fn server_config_applies_cli_port_override() {
        let cfg = ServerConfig::from_args_with(
            ["vibe_rdesk", "-p", "9000"],
            ServerConfig::from_env_with(None, None),
        )
        .unwrap();
        assert_eq!(cfg.bind, "0.0.0.0:9000");
    }

    #[test]
    fn server_config_keeps_host_when_port_overridden() {
        let mut cfg = ServerConfig::from_env_with(Some("127.0.0.1:3000".into()), None);
        cfg.bind = super::bind_with_port(&cfg.bind, 8001);
        assert_eq!(cfg.bind, "127.0.0.1:8001");
    }
}
