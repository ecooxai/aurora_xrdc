use anyhow::Result;
use axum::{
    Json, Router,
    extract::{ConnectInfo, Multipart, Path as AxumPath, Query, State, ws::WebSocketUpgrade},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
use axum_server::{Handle, tls_rustls::RustlsConfig};
use futures_util::future::try_join_all;
use serde::{Deserialize, Serialize};
use socket2::{Domain, Protocol, Socket, Type};
use std::{
    collections::HashSet,
    future::IntoFuture,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::{Mutex, watch};
use tracing::{info, warn};
use uuid::Uuid;

use crate::{
    camera::CameraRelay,
    client_manager::{ClientManager, WebClientInfo},
    clipboard::{
        ClipboardHistoryEntry, ClipboardPayload, ensure_upload_dir, read_clipboard_history,
        read_remote_clipboard, write_clipboard_history, write_remote_clipboard,
    },
    ffmpeg,
    media::MediaHub,
    session::{self, SessionRole},
    settings::{
        AudioStreamConfig, CodecKind, EncodePreference, EncoderLatencyMode, EncoderQualityMode,
        ServerConfig, StreamConfig, VideoPerformanceConfig, VideoScale,
    },
    transport, webtransport,
};
use wtransport::endpoint::IncomingSession;

const INDEX_HTML: &str = include_str!("../web/index.html");
const APP_JS: &str = include_str!("../web/app.js");
const APP_CSS: &str = include_str!("../web/app.css");
const VIDEO_RENDERER_WORKER_JS: &str = include_str!("../web/video_renderer_worker.js");
const WEB_MANIFEST: &str = include_str!("../web/manifest.webmanifest");
const SERVICE_WORKER_JS: &str = include_str!("../web/sw.js");
const ICON_SVG: &str = include_str!("../web/icon.svg");
const ICON_192_PNG: &[u8] = include_bytes!("../web/icon-192.png");
const ICON_512_PNG: &[u8] = include_bytes!("../web/icon-512.png");
const WS_MAX_MESSAGE_SIZE: usize = 64 * 1024 * 1024;

#[derive(Clone)]
struct AppState {
    server: ServerConfig,
    auth: Arc<AuthTracker>,
    camera: CameraRelay,
    media: MediaHub,
    clients: Arc<ClientManager>,
    wt_info: Option<WtInfo>,
}

/// Connection metadata advertised to browsers so they can reach the
/// WebTransport endpoint with a self-signed certificate.
#[derive(Clone, Debug)]
struct WtInfo {
    port: u16,
    cert_hashes: Vec<String>,
}

#[derive(Debug)]
struct AuthTracker {
    lockout_duration: Duration,
    state: Mutex<AuthTrackerState>,
}

#[derive(Debug, Default)]
struct AuthTrackerState {
    failed_attempts: u32,
    locked_until: Option<Instant>,
    session_tokens: HashSet<String>,
}

impl AuthTracker {
    fn new() -> Self {
        Self::new_with_lockout(Duration::from_secs(60 * 60))
    }

    fn new_with_lockout(lockout_duration: Duration) -> Self {
        Self {
            lockout_duration,
            state: Mutex::new(AuthTrackerState::default()),
        }
    }

    async fn require_passwd(
        &self,
        expected: &str,
        provided: Option<&str>,
        session_token: Option<&str>,
    ) -> Result<(), (StatusCode, String)> {
        if expected.is_empty() {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "server password is not configured".into(),
            ));
        }

        if self.session_matches(session_token).await {
            return Ok(());
        }

        if let Some(actual) = provided {
            self.verify_passwd(expected, actual).await?;
            return Ok(());
        }

        Err((StatusCode::UNAUTHORIZED, "authentication required".into()))
    }

    async fn authenticate(
        &self,
        expected: &str,
        provided: &str,
    ) -> Result<String, (StatusCode, String)> {
        if expected.is_empty() {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "server password is not configured".into(),
            ));
        }

        let now = Instant::now();
        let mut state = self.state.lock().await;

        if let Some(locked_until) = state.locked_until {
            if now < locked_until {
                let remaining = locked_until.saturating_duration_since(now);
                return Err((
                    StatusCode::TOO_MANY_REQUESTS,
                    format!(
                        "too many failed password attempts; try again in {}",
                        format_duration(remaining)
                    ),
                ));
            }
            state.locked_until = None;
            state.failed_attempts = 0;
        }

        if provided == expected {
            state.failed_attempts = 0;
            state.locked_until = None;
            let token = Uuid::new_v4().to_string();
            state.session_tokens.insert(token.clone());
            Ok(token)
        } else {
            state.failed_attempts = state.failed_attempts.saturating_add(1);
            if state.failed_attempts >= 20 {
                state.failed_attempts = 0;
                state.locked_until = Some(now + self.lockout_duration);
                Err((
                    StatusCode::TOO_MANY_REQUESTS,
                    "too many failed password attempts; locked for 1 hour".into(),
                ))
            } else {
                Err((
                    StatusCode::UNAUTHORIZED,
                    "invalid or missing passwd; start the server with --passwd <password>".into(),
                ))
            }
        }
    }

    async fn verify_passwd(
        &self,
        expected: &str,
        provided: &str,
    ) -> Result<(), (StatusCode, String)> {
        if expected.is_empty() {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "server password is not configured".into(),
            ));
        }

        let now = Instant::now();
        let mut state = self.state.lock().await;

        if let Some(locked_until) = state.locked_until {
            if now < locked_until {
                let remaining = locked_until.saturating_duration_since(now);
                return Err((
                    StatusCode::TOO_MANY_REQUESTS,
                    format!(
                        "too many failed password attempts; try again in {}",
                        format_duration(remaining)
                    ),
                ));
            }
            state.locked_until = None;
            state.failed_attempts = 0;
        }

        match provided {
            actual if actual == expected => {
                state.failed_attempts = 0;
                state.locked_until = None;
                Ok(())
            }
            _ => {
                state.failed_attempts = state.failed_attempts.saturating_add(1);
                if state.failed_attempts >= 20 {
                    state.failed_attempts = 0;
                    state.locked_until = Some(now + self.lockout_duration);
                    Err((
                        StatusCode::TOO_MANY_REQUESTS,
                        "too many failed password attempts; locked for 1 hour".into(),
                    ))
                } else {
                    Err((
                        StatusCode::UNAUTHORIZED,
                        "invalid or missing passwd; start the server with --passwd <password>"
                            .into(),
                    ))
                }
            }
        }
    }

    async fn session_matches(&self, token: Option<&str>) -> bool {
        let state = self.state.lock().await;
        token
            .map(|token| state.session_tokens.contains(token))
            .unwrap_or(false)
    }
}

#[derive(Debug, Deserialize)]
struct AuthQuery {
    passwd: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AuthBody {
    passwd: String,
}

#[derive(Debug, Deserialize)]
struct EncodersQuery {
    codec: Option<CodecKind>,
    passwd: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CodecsQuery {
    passwd: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct WsQuery {
    client_id: Option<String>,
    role: Option<SessionRole>,
    codec: Option<CodecKind>,
    bitrate_kbps: Option<u32>,
    audio_bitrate_kbps: Option<u32>,
    fps: Option<u32>,
    encode_preference: Option<EncodePreference>,
    encoder_latency: Option<EncoderLatencyMode>,
    encoder_quality: Option<EncoderQualityMode>,
    gop_ms: Option<u32>,
    buffer_ms: Option<u32>,
    scale: Option<VideoScale>,
    passwd: Option<String>,
}

impl WsQuery {
    /// Builds the normalized video stream configuration from the query.
    fn stream_config(&self) -> StreamConfig {
        StreamConfig {
            codec: self.codec.unwrap_or(CodecKind::H264),
            bitrate_kbps: self
                .bitrate_kbps
                .unwrap_or_else(|| StreamConfig::default().bitrate_kbps),
            fps: self.fps.unwrap_or_else(|| StreamConfig::default().fps),
            encode_preference: self.encode_preference.unwrap_or_default(),
            performance: VideoPerformanceConfig {
                encoder_latency: self.encoder_latency.unwrap_or_default(),
                encoder_quality: self.encoder_quality.unwrap_or_default(),
                gop_ms: self
                    .gop_ms
                    .unwrap_or_else(|| VideoPerformanceConfig::default().gop_ms),
                buffer_ms: self
                    .buffer_ms
                    .unwrap_or_else(|| VideoPerformanceConfig::default().buffer_ms),
                scale: self
                    .scale
                    .unwrap_or_else(|| VideoPerformanceConfig::default().scale),
            },
        }
        .normalized()
    }

    /// Builds the normalized audio stream configuration from the query.
    fn audio_config(&self) -> AudioStreamConfig {
        AudioStreamConfig {
            bitrate_kbps: self.audio_bitrate_kbps.unwrap_or(128),
        }
        .normalized()
    }

    /// Resolves the session role, defaulting to a combined session.
    fn role(&self) -> SessionRole {
        self.role.unwrap_or_default()
    }

    /// Resolves the client id, generating a fresh one when absent.
    fn resolve_client_id(&self) -> String {
        self.client_id
            .clone()
            .filter(|id| !id.trim().is_empty())
            .unwrap_or_else(|| Uuid::new_v4().to_string())
    }
}

#[derive(Debug, Deserialize)]
struct WebClientsQuery {
    client_id: Option<String>,
    passwd: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AudioOutputQuery {
    passwd: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AudioOutputRequest {
    use_real_device: bool,
    sink: Option<String>,
}

#[derive(Debug, Serialize)]
struct AudioOutputResponse {
    mode: &'static str,
    active_sink: Option<String>,
    virtual_sink: String,
    devices: Vec<ffmpeg::AudioOutputDevice>,
}

#[derive(Debug, Deserialize)]
struct CloseOthersRequest {
    client_id: String,
}

#[derive(Debug, Serialize)]
struct WebClientsResponse {
    current_client_id: Option<String>,
    count: usize,
    clients: Vec<WebClientInfo>,
}

#[derive(Debug, Serialize)]
struct CloseClientsResponse {
    closed: usize,
}

#[derive(Debug, Deserialize)]
struct CameraStopRequest {
    session_id: String,
}

pub async fn run(server: ServerConfig) -> Result<()> {
    let bind = server.bind.clone();
    let tls_paths = server.tls_cert.clone().zip(server.tls_key.clone());
    let camera = CameraRelay::new();
    let media = MediaHub::new(server.clone());
    if let Err(err) = ffmpeg::warm_audio_stack(&server).await {
        warn!("audio bootstrap failed during startup: {err}");
    }
    let wt_setup = setup_webtransport(&server.bind);
    let wt_meta = wt_setup.as_ref().map(|setup| WtInfo {
        port: setup.local_addr.port(),
        cert_hashes: setup.cert_hashes.clone(),
    });
    let state = Arc::new(AppState {
        server,
        auth: Arc::new(AuthTracker::new()),
        camera,
        media: media.clone(),
        clients: Arc::new(ClientManager::default()),
        wt_info: wt_meta,
    });
    if let Some(setup) = wt_setup {
        info!(
            "WebTransport listening on udp://{} ({} cert hash(es))",
            setup.local_addr,
            setup.cert_hashes.len()
        );
        let wt_state = state.clone();
        tokio::spawn(run_webtransport(setup.endpoint, wt_state));
    }
    let app = Router::new()
        .route("/", get(index))
        .route("/app.js", get(js))
        .route("/app.css", get(css))
        .route("/video_renderer_worker.js", get(video_renderer_worker_js))
        .route("/manifest.webmanifest", get(manifest))
        .route("/sw.js", get(service_worker_js))
        .route("/icon.svg", get(icon_svg))
        .route("/icon-192.png", get(icon_192_png))
        .route("/icon-512.png", get(icon_512_png))
        .route("/healthz", get(healthz))
        .route("/api/wt-info", get(wt_info))
        .route("/api/auth", get(auth_check).post(auth_login))
        .route("/api/codecs", get(codecs))
        .route("/api/encoders", get(encoders))
        .route(
            "/api/audio/output",
            get(audio_output).post(set_audio_output),
        )
        .route("/api/webclients", get(webclients))
        .route("/api/webclients/close-others", post(close_other_webclients))
        .route("/api/webclients/{client_id}/close", post(close_webclient))
        .route("/ws", get(ws))
        .route("/api/upload", post(upload))
        .route("/api/camera/chunk", post(upload_camera_chunk))
        .route("/api/camera/stop", post(stop_camera))
        .route(
            "/api/clipboard/history",
            get(get_clipboard_history).post(set_clipboard_history),
        )
        .route(
            "/api/clipboard/remote",
            get(get_remote_clipboard).post(set_remote_clipboard),
        )
        .layer(middleware::from_fn(cors))
        .with_state(state);
    let listeners = bind
        .split(',')
        .map(str::trim)
        .filter(|bind| !bind.is_empty())
        .map(bind_listener)
        .collect::<Result<Vec<_>>>()?;
    let listen_urls = listeners
        .iter()
        .map(|listener| {
            listener.local_addr().map(|addr| {
                let scheme = if tls_paths.is_some() { "https" } else { "http" };
                format!("{scheme}://{addr}")
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    info!("listening on {}", listen_urls.join(", "));
    let result = if let Some((cert_path, key_path)) = tls_paths {
        let tls_config = RustlsConfig::from_pem_file(&cert_path, &key_path).await?;
        let handles = listeners
            .iter()
            .map(|_| Handle::new())
            .collect::<Vec<Handle<SocketAddr>>>();
        let shutdown_handles = handles.clone();
        tokio::spawn(async move {
            shutdown_signal().await;
            for handle in shutdown_handles {
                handle.graceful_shutdown(Some(Duration::from_secs(10)));
            }
        });
        try_join_all(
            listeners
                .into_iter()
                .zip(handles)
                .map(|(listener, handle)| {
                    let app = app.clone();
                    let tls_config = tls_config.clone();
                    async move {
                        let listener = listener.into_std()?;
                        axum_server::from_tcp_rustls(listener, tls_config)?
                            .handle(handle)
                            .serve(app.into_make_service_with_connect_info::<SocketAddr>())
                            .await
                    }
                }),
        )
        .await
    } else {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        tokio::spawn(async move {
            shutdown_signal().await;
            let _ = shutdown_tx.send(true);
        });
        try_join_all(listeners.into_iter().map(|listener| {
            let app = app.clone();
            let mut shutdown_rx = shutdown_rx.clone();
            async move {
                axum::serve(
                    listener,
                    app.into_make_service_with_connect_info::<SocketAddr>(),
                )
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.changed().await;
                })
                .into_future()
                .await
            }
        }))
        .await
    };
    media.shutdown().await;
    result?;
    Ok(())
}

fn bind_listener(bind: &str) -> Result<tokio::net::TcpListener> {
    let addr: SocketAddr = bind.parse()?;
    let socket = match addr {
        SocketAddr::V4(_) => Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))?,
        SocketAddr::V6(_) => {
            let socket = Socket::new(Domain::IPV6, Type::STREAM, Some(Protocol::TCP))?;
            socket.set_only_v6(true)?;
            socket
        }
    };

    socket.set_reuse_address(true)?;
    socket.bind(&addr.into())?;
    socket.listen(1024)?;

    let listener = std::net::TcpListener::from(socket);
    listener.set_nonblocking(true)?;
    Ok(tokio::net::TcpListener::from_std(listener)?)
}

async fn shutdown_signal() {
    let ctrl_c = async {
        if let Err(err) = tokio::signal::ctrl_c().await {
            warn!("failed to install ctrl-c handler: {err}");
        }
    };

    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{SignalKind, signal};

        match signal(SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(err) => warn!("failed to install SIGTERM handler: {err}"),
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }

    info!("shutdown signal received");
}

async fn index() -> impl IntoResponse {
    Html(INDEX_HTML)
}

async fn js() -> Response {
    asset("application/javascript; charset=utf-8", APP_JS)
}

async fn css() -> Response {
    asset("text/css; charset=utf-8", APP_CSS)
}

async fn video_renderer_worker_js() -> Response {
    asset(
        "application/javascript; charset=utf-8",
        VIDEO_RENDERER_WORKER_JS,
    )
}

async fn manifest() -> Response {
    asset("application/manifest+json; charset=utf-8", WEB_MANIFEST)
}

async fn service_worker_js() -> Response {
    asset("application/javascript; charset=utf-8", SERVICE_WORKER_JS)
}

async fn icon_svg() -> Response {
    asset("image/svg+xml; charset=utf-8", ICON_SVG)
}

async fn icon_192_png() -> Response {
    binary_asset("image/png", ICON_192_PNG)
}

async fn icon_512_png() -> Response {
    binary_asset("image/png", ICON_512_PNG)
}

async fn healthz() -> impl IntoResponse {
    "ok"
}

async fn auth_check(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
) -> Result<&'static str, (StatusCode, String)> {
    let session_token = cookie_session_token(&headers);
    state
        .auth
        .require_passwd(&state.server.passwd, None, session_token.as_deref())
        .await?;
    Ok("ok")
}

async fn auth_login(
    State(state): State<Arc<AppState>>,
    Json(body): Json<AuthBody>,
) -> Result<Response, (StatusCode, String)> {
    let token = state
        .auth
        .authenticate(&state.server.passwd, body.passwd.as_str())
        .await?;
    let cookie = HeaderValue::from_str(&format!(
        "vibe_rdesk_session={token}; Path=/; HttpOnly; SameSite=Lax"
    ))
    .map_err(|err| internal_error(err.into()))?;
    let mut headers = HeaderMap::new();
    headers.insert(header::SET_COOKIE, cookie);
    Ok((StatusCode::OK, headers, "ok").into_response())
}

#[derive(Debug, Serialize)]
struct EncodersResponse {
    options: Vec<ffmpeg::AvailableEncoderOption>,
}

#[derive(Debug, Serialize)]
struct CodecsResponse {
    options: Vec<ffmpeg::AvailableCodecOption>,
}

async fn encoders(
    headers: HeaderMap,
    Query(query): Query<EncodersQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<EncodersResponse>, (StatusCode, String)> {
    let session_token = cookie_session_token(&headers);
    state
        .auth
        .require_passwd(
            &state.server.passwd,
            query.passwd.as_deref(),
            session_token.as_deref(),
        )
        .await?;
    let codec = query.codec.unwrap_or(CodecKind::H264);
    let options = ffmpeg::available_encoder_options(codec)
        .await
        .map_err(|err| {
            warn!(codec = ?codec, error = %err, "failed to load available encoder options");
            (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
        })?;
    Ok(Json(EncodersResponse { options }))
}

async fn codecs(
    headers: HeaderMap,
    Query(query): Query<CodecsQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<CodecsResponse>, (StatusCode, String)> {
    let session_token = cookie_session_token(&headers);
    state
        .auth
        .require_passwd(
            &state.server.passwd,
            query.passwd.as_deref(),
            session_token.as_deref(),
        )
        .await?;
    let options = ffmpeg::available_codec_options().await.map_err(|err| {
        warn!(error = %err, "failed to load available codec options");
        (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
    })?;
    Ok(Json(CodecsResponse { options }))
}

async fn audio_output(
    headers: HeaderMap,
    Query(query): Query<AudioOutputQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<AudioOutputResponse>, (StatusCode, String)> {
    let session_token = cookie_session_token(&headers);
    state
        .auth
        .require_passwd(
            &state.server.passwd,
            query.passwd.as_deref(),
            session_token.as_deref(),
        )
        .await?;
    audio_output_response(&state.server).await.map(Json)
}

async fn set_audio_output(
    headers: HeaderMap,
    Query(query): Query<AudioOutputQuery>,
    State(state): State<Arc<AppState>>,
    Json(body): Json<AudioOutputRequest>,
) -> Result<Json<AudioOutputResponse>, (StatusCode, String)> {
    let session_token = cookie_session_token(&headers);
    state
        .auth
        .require_passwd(
            &state.server.passwd,
            query.passwd.as_deref(),
            session_token.as_deref(),
        )
        .await?;
    ffmpeg::set_audio_output_device(&state.server, body.use_real_device, body.sink.as_deref())
        .await
        .map_err(internal_error)?;
    state.media.restart_audio().await.map_err(internal_error)?;
    audio_output_response(&state.server).await.map(Json)
}

async fn audio_output_response(
    server: &ServerConfig,
) -> Result<AudioOutputResponse, (StatusCode, String)> {
    let devices = ffmpeg::list_audio_output_devices(server)
        .await
        .map_err(internal_error)?;
    let virtual_sink = ffmpeg::project_virtual_audio_sink_name(server);
    let active_sink = devices
        .iter()
        .find(|device| device.is_default)
        .map(|device| device.name.clone());
    let mode = if active_sink.as_deref() == Some(virtual_sink.as_str()) {
        "virtual"
    } else {
        "real"
    };
    Ok(AudioOutputResponse {
        mode,
        active_sink,
        virtual_sink,
        devices,
    })
}

async fn ws(
    headers: HeaderMap,
    ws: WebSocketUpgrade,
    ConnectInfo(peer_addr): ConnectInfo<SocketAddr>,
    State(state): State<Arc<AppState>>,
    Query(query): Query<WsQuery>,
) -> Response {
    let session_token = cookie_session_token(&headers);
    if let Err(response) = state
        .auth
        .require_passwd(
            &state.server.passwd,
            query.passwd.as_deref(),
            session_token.as_deref(),
        )
        .await
    {
        return response.into_response();
    }
    let config = query.stream_config();
    let audio_config = query.audio_config();
    let role = query.role();
    let client_id = query.resolve_client_id();
    let lease = state.clients.register(client_id, peer_addr, role).await;
    let close_rx = lease.close_rx.clone();
    ws.max_message_size(WS_MAX_MESSAGE_SIZE)
        .max_frame_size(WS_MAX_MESSAGE_SIZE)
        .on_upgrade(move |socket| async move {
        let _lease = lease;
        let (sink, stream) = transport::from_websocket(socket);
        if let Err(err) = session::handle_socket(
            sink,
            stream,
            state.server.clone(),
            state.media.clone(),
            config,
            audio_config,
            role,
            close_rx,
            state.clients.clone(),
        )
        .await
        {
            warn!(error = %err, "websocket session ended with an error");
        }
    })
}

#[derive(Debug, Serialize)]
struct WtInfoResponse {
    enabled: bool,
    port: u16,
    cert_hashes: Vec<String>,
}

/// Advertises whether the WebTransport endpoint is available and how to reach
/// it (UDP port and the self-signed certificate hashes for the browser).
async fn wt_info(State(state): State<Arc<AppState>>) -> Json<WtInfoResponse> {
    match &state.wt_info {
        Some(info) => Json(WtInfoResponse {
            enabled: true,
            port: info.port,
            cert_hashes: info.cert_hashes.clone(),
        }),
        None => Json(WtInfoResponse {
            enabled: false,
            port: 0,
            cert_hashes: Vec::new(),
        }),
    }
}

/// Resolves the UDP socket address the WebTransport endpoint should bind to.
///
/// Defaults to the first HTTP bind address (UDP and TCP port namespaces are
/// independent, so reusing the port is fine) and can be overridden with
/// `VIBE_RDESK_WT_BIND`. Set `VIBE_RDESK_WT=0` to disable WebTransport entirely.
///
/// When the bind list contains both an IPv4 (`0.0.0.0`) and an IPv6 (`[::]`)
/// wildcard, the IPv6 one is preferred: on Linux the resulting QUIC/UDP socket
/// is dual-stack (IPV6_V6ONLY defaults to 0), so it accepts both IPv4 and IPv6
/// clients through a single endpoint.  Without this, a browser that connects to
/// the server over IPv6 cannot reach the IPv4-only WebTransport port.
fn webtransport_bind_addr(http_bind: &str) -> Option<SocketAddr> {
    match std::env::var("VIBE_RDESK_WT") {
        Ok(value) if matches!(value.trim(), "0" | "false" | "no" | "off") => return None,
        _ => {}
    }
    if let Ok(explicit) = std::env::var("VIBE_RDESK_WT_BIND") {
        return explicit.trim().parse().ok();
    }
    let addrs: Vec<SocketAddr> = http_bind
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();
    // Prefer [::]:port (dual-stack) so the QUIC endpoint is reachable from
    // both IPv4 and IPv6 clients.  Fall back to the first address if no IPv6
    // wildcard is present.
    addrs
        .iter()
        .find(|a| a.is_ipv6() && a.ip().is_unspecified())
        .or_else(|| addrs.first())
        .copied()
}

/// Sets up the WebTransport endpoint, logging and disabling it on failure.
fn setup_webtransport(http_bind: &str) -> Option<webtransport::WtSetup> {
    let addr = webtransport_bind_addr(http_bind)?;
    match webtransport::setup(addr) {
        Ok(setup) => Some(setup),
        Err(err) => {
            warn!("WebTransport disabled: {err}");
            None
        }
    }
}

/// Accepts incoming WebTransport sessions and dispatches each to the shared
/// session handlers.
async fn run_webtransport(endpoint: webtransport::WtEndpoint, state: Arc<AppState>) {
    loop {
        let incoming = endpoint.accept().await;
        let state = state.clone();
        tokio::spawn(async move {
            if let Err(err) = handle_wt_incoming(state, incoming).await {
                warn!(error = %err, "webtransport session ended with an error");
            }
        });
    }
}

async fn handle_wt_incoming(state: Arc<AppState>, incoming: IncomingSession) -> Result<()> {
    let session_request = incoming.await?;
    let path = session_request.path();
    let query_str = path.split_once('?').map(|(_, q)| q).unwrap_or("");
    let query: WsQuery = serde_urlencoded::from_str(query_str).unwrap_or_default();

    if state
        .auth
        .require_passwd(&state.server.passwd, query.passwd.as_deref(), None)
        .await
        .is_err()
    {
        session_request.forbidden().await;
        return Ok(());
    }

    let config = query.stream_config();
    let audio_config = query.audio_config();
    let role = query.role();
    let client_id = query.resolve_client_id();

    let connection = session_request.accept().await?;
    let peer_addr = connection.remote_address();
    let lease = state.clients.register(client_id, peer_addr, role).await;
    let close_rx = lease.close_rx.clone();

    let (send, recv) = connection.accept_bi().await?;
    let (sink, stream) = webtransport::wire_from_bi(connection, send, recv);

    let _lease = lease;
    session::handle_socket(
        sink,
        stream,
        state.server.clone(),
        state.media.clone(),
        config,
        audio_config,
        role,
        close_rx,
        state.clients.clone(),
    )
    .await
}

async fn webclients(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    Query(query): Query<WebClientsQuery>,
) -> Result<Json<WebClientsResponse>, (StatusCode, String)> {
    let session_token = cookie_session_token(&headers);
    state
        .auth
        .require_passwd(
            &state.server.passwd,
            query.passwd.as_deref(),
            session_token.as_deref(),
        )
        .await?;
    let clients = state.clients.list().await;
    Ok(Json(WebClientsResponse {
        current_client_id: query.client_id,
        count: clients.len(),
        clients,
    }))
}

async fn close_webclient(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    AxumPath(client_id): AxumPath<String>,
    Query(query): Query<WebClientsQuery>,
) -> Result<Json<CloseClientsResponse>, (StatusCode, String)> {
    let session_token = cookie_session_token(&headers);
    state
        .auth
        .require_passwd(
            &state.server.passwd,
            query.passwd.as_deref(),
            session_token.as_deref(),
        )
        .await?;
    let closed = usize::from(state.clients.close_client(&client_id).await);
    Ok(Json(CloseClientsResponse { closed }))
}

async fn close_other_webclients(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    Query(query): Query<WebClientsQuery>,
    Json(body): Json<CloseOthersRequest>,
) -> Result<Json<CloseClientsResponse>, (StatusCode, String)> {
    let session_token = cookie_session_token(&headers);
    state
        .auth
        .require_passwd(
            &state.server.passwd,
            query.passwd.as_deref(),
            session_token.as_deref(),
        )
        .await?;
    let closed = state.clients.close_other_clients(&body.client_id).await;
    Ok(Json(CloseClientsResponse { closed }))
}

#[derive(Debug, Serialize)]
struct UploadResponse {
    saved_as: String,
}

#[derive(Debug, Serialize)]
struct CameraChunkResponse {
    device: String,
    queued_chunks: usize,
}

#[derive(Debug, Serialize)]
struct CameraStopResponse {
    stopped: bool,
}

async fn upload(
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<UploadResponse>, (StatusCode, String)> {
    let session_token = cookie_session_token(&headers);
    state
        .auth
        .require_passwd(
            &state.server.passwd,
            auth.passwd.as_deref(),
            session_token.as_deref(),
        )
        .await?;
    let dir = PathBuf::from(&state.server.upload_dir);
    ensure_upload_dir(&dir).await.map_err(internal_error)?;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| internal_error(err.into()))?
    {
        let file_name = field
            .file_name()
            .map(sanitize_file_name)
            .unwrap_or_else(|| default_upload_name("upload.bin"));
        let bytes = field
            .bytes()
            .await
            .map_err(|err| internal_error(err.into()))?;
        let path = unique_upload_path(&dir, &file_name);
        tokio::fs::write(&path, &bytes)
            .await
            .map_err(|err| internal_error(err.into()))?;
        return Ok(Json(UploadResponse {
            saved_as: path.to_string_lossy().into_owned(),
        }));
    }
    Err((StatusCode::BAD_REQUEST, "missing file field".into()))
}

async fn upload_camera_chunk(
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<CameraChunkResponse>, (StatusCode, String)> {
    let session_token = cookie_session_token(&headers);
    state
        .auth
        .require_passwd(
            &state.server.passwd,
            auth.passwd.as_deref(),
            session_token.as_deref(),
        )
        .await?;

    let mut session_id = None;
    let mut seq = None;
    let mut file_bytes = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| internal_error(err.into()))?
    {
        let name = field.name().unwrap_or_default().to_string();
        match name.as_str() {
            "session_id" => {
                let value = field
                    .text()
                    .await
                    .map_err(|err| internal_error(err.into()))?;
                if !value.trim().is_empty() {
                    session_id = Some(value);
                }
            }
            "seq" => {
                let value = field
                    .text()
                    .await
                    .map_err(|err| internal_error(err.into()))?;
                seq = value.trim().parse::<u64>().ok();
            }
            "file" => {
                file_bytes = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|err| internal_error(err.into()))?
                        .to_vec(),
                );
            }
            _ => {}
        }
    }

    let session_id =
        session_id.ok_or_else(|| (StatusCode::BAD_REQUEST, "missing session_id".into()))?;
    let seq = seq.ok_or_else(|| (StatusCode::BAD_REQUEST, "missing seq".into()))?;
    let bytes = file_bytes.ok_or_else(|| (StatusCode::BAD_REQUEST, "missing file".into()))?;

    let status = state
        .camera
        .enqueue_media_chunk(&session_id, seq, bytes)
        .await
        .map_err(internal_error)?;

    Ok(Json(CameraChunkResponse {
        device: status.device,
        queued_chunks: status.queued_chunks,
    }))
}

async fn stop_camera(
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    State(state): State<Arc<AppState>>,
    Json(request): Json<CameraStopRequest>,
) -> Result<Json<CameraStopResponse>, (StatusCode, String)> {
    let session_token = cookie_session_token(&headers);
    state
        .auth
        .require_passwd(
            &state.server.passwd,
            auth.passwd.as_deref(),
            session_token.as_deref(),
        )
        .await?;

    state
        .camera
        .stop_session(&request.session_id)
        .await
        .map_err(internal_error)?;

    Ok(Json(CameraStopResponse { stopped: true }))
}

async fn get_remote_clipboard(
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<ClipboardPayload>, (StatusCode, String)> {
    let session_token = cookie_session_token(&headers);
    state
        .auth
        .require_passwd(
            &state.server.passwd,
            auth.passwd.as_deref(),
            session_token.as_deref(),
        )
        .await?;
    let payload = read_remote_clipboard(&state.server.display)
        .await
        .map_err(internal_error)?;
    Ok(Json(payload))
}

async fn get_clipboard_history(
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ClipboardHistoryEntry>>, (StatusCode, String)> {
    let session_token = cookie_session_token(&headers);
    state
        .auth
        .require_passwd(
            &state.server.passwd,
            auth.passwd.as_deref(),
            session_token.as_deref(),
        )
        .await?;
    let history = read_clipboard_history().await.map_err(internal_error)?;
    Ok(Json(history))
}

async fn set_clipboard_history(
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    State(state): State<Arc<AppState>>,
    Json(history): Json<Vec<ClipboardHistoryEntry>>,
) -> Result<Json<Vec<ClipboardHistoryEntry>>, (StatusCode, String)> {
    let session_token = cookie_session_token(&headers);
    state
        .auth
        .require_passwd(
            &state.server.passwd,
            auth.passwd.as_deref(),
            session_token.as_deref(),
        )
        .await?;
    write_clipboard_history(&history)
        .await
        .map_err(internal_error)?;
    Ok(Json(history))
}

async fn set_remote_clipboard(
    headers: HeaderMap,
    Query(auth): Query<AuthQuery>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ClipboardPayload>,
) -> Result<Json<ClipboardPayload>, (StatusCode, String)> {
    let session_token = cookie_session_token(&headers);
    state
        .auth
        .require_passwd(
            &state.server.passwd,
            auth.passwd.as_deref(),
            session_token.as_deref(),
        )
        .await?;
    write_remote_clipboard(&state.server.display, &payload)
        .await
        .map_err(internal_error)?;
    Ok(Json(payload))
}

fn asset(content_type: &'static str, body: &'static str) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    (StatusCode::OK, headers, body).into_response()
}

fn binary_asset(content_type: &'static str, body: &'static [u8]) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    (StatusCode::OK, headers, body.to_vec()).into_response()
}

async fn cors(req: axum::http::Request<axum::body::Body>, next: Next) -> Response {
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);

    if req.method() == Method::OPTIONS {
        let mut response = StatusCode::NO_CONTENT.into_response();
        apply_cors_headers(response.headers_mut(), origin.as_deref());
        return response;
    }

    let mut response = next.run(req).await;
    apply_cors_headers(response.headers_mut(), origin.as_deref());
    response
}

fn apply_cors_headers(headers: &mut HeaderMap, origin: Option<&str>) {
    let Some(origin) = origin else {
        return;
    };
    let Ok(origin) = HeaderValue::from_str(origin) else {
        return;
    };

    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
        HeaderValue::from_static("true"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("content-type, authorization"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, OPTIONS"),
    );
    headers.insert(header::VARY, HeaderValue::from_static("Origin"));
}

fn cookie_session_token(headers: &HeaderMap) -> Option<String> {
    let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    cookie.split(';').find_map(|item| {
        let (name, value) = item.trim().split_once('=')?;
        (name == "vibe_rdesk_session").then(|| value.to_string())
    })
}

fn internal_error(err: anyhow::Error) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}

fn format_duration(duration: Duration) -> String {
    let total_seconds = duration.as_secs();
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if hours > 0 {
        if minutes > 0 {
            format!("{hours}h {minutes}m")
        } else {
            format!("{hours}h")
        }
    } else if minutes > 0 {
        if seconds > 0 {
            format!("{minutes}m {seconds}s")
        } else {
            format!("{minutes}m")
        }
    } else {
        format!("{seconds}s")
    }
}

fn sanitize_file_name(name: &str) -> String {
    let file_name = Path::new(name)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("upload.bin");
    let cleaned: String = file_name
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '_' | '-' => ch,
            _ => '_',
        })
        .collect();
    if cleaned.is_empty() {
        "upload.bin".into()
    } else {
        cleaned
    }
}

fn unique_upload_path(dir: &Path, file_name: &str) -> PathBuf {
    let path = dir.join(file_name);
    if !path.exists() {
        return path;
    }
    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("upload");
    let ext = Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str());
    for index in 1..10_000 {
        let candidate = match ext {
            Some(ext) if !ext.is_empty() => dir.join(format!("{stem}_{index}.{ext}")),
            _ => dir.join(format!("{stem}_{index}")),
        };
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(default_upload_name(file_name))
}

fn default_upload_name(file_name: &str) -> String {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{stamp}_{file_name}")
}

#[cfg(test)]
mod tests {
    use super::AuthTracker;
    use std::time::Duration;

    #[tokio::test]
    async fn locks_after_twenty_failed_attempts() {
        let tracker = AuthTracker::new_with_lockout(Duration::from_millis(50));

        for _ in 0..19 {
            let err = tracker
                .require_passwd("secret", Some("wrong"), None)
                .await
                .unwrap_err();
            assert_eq!(err.0, axum::http::StatusCode::UNAUTHORIZED);
        }

        let err = tracker
            .require_passwd("secret", Some("wrong"), None)
            .await
            .unwrap_err();
        assert_eq!(err.0, axum::http::StatusCode::TOO_MANY_REQUESTS);
        assert!(err.1.contains("locked for 1 hour"));
    }

    #[tokio::test]
    async fn lockout_expires_and_success_resets_counter() {
        let tracker = AuthTracker::new_with_lockout(Duration::from_millis(25));

        for _ in 0..20 {
            let _ = tracker.require_passwd("secret", Some("wrong"), None).await;
        }

        tokio::time::sleep(Duration::from_millis(30)).await;

        tracker
            .require_passwd("secret", Some("secret"), None)
            .await
            .unwrap();
        let err = tracker
            .require_passwd("secret", Some("wrong"), None)
            .await
            .unwrap_err();
        assert_eq!(err.0, axum::http::StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn successful_auth_issues_session_token() {
        let tracker = AuthTracker::new();
        let token = tracker
            .authenticate("secret", "secret")
            .await
            .expect("session token");
        tracker
            .require_passwd("secret", None, Some(token.as_str()))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn multiple_session_tokens_remain_valid() {
        let tracker = AuthTracker::new();
        let first = tracker
            .authenticate("secret", "secret")
            .await
            .expect("first session token");
        let second = tracker
            .authenticate("secret", "secret")
            .await
            .expect("second session token");
        tracker
            .require_passwd("secret", None, Some(first.as_str()))
            .await
            .unwrap();
        tracker
            .require_passwd("secret", None, Some(second.as_str()))
            .await
            .unwrap();
    }
}
