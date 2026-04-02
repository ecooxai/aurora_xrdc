use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::{ffi::OsString, path::PathBuf};

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
                .unwrap_or_else(|| "0.0.0.0:8001".into()),
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
    match bind.rsplit_once(':') {
        Some((host, _)) if !host.is_empty() => format!("{host}:{port}"),
        _ => format!("0.0.0.0:{port}"),
    }
}

#[cfg(test)]
mod tests {
    use super::{CodecKind, ServerConfig, StreamConfig};
    use std::path::PathBuf;

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
        let cfg = ServerConfig::from_env_with(
            None,
            None,
            None,
            Some(PathBuf::from("/tmp/vibe-rdesk-home")),
        );
        assert_eq!(cfg.bind, "0.0.0.0:8001");
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
        assert_eq!(cfg.bind, "0.0.0.0:9000");
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
        let mut cfg = ServerConfig::from_env_with(Some("127.0.0.1:3000".into()), None, None, None);
        cfg.bind = super::bind_with_port(&cfg.bind, 8001);
        assert_eq!(cfg.bind, "127.0.0.1:8001");
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
