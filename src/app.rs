use anyhow::Result;
use axum::{
    Json, Router,
    extract::{Multipart, Query, State, ws::WebSocketUpgrade},
    http::{HeaderMap, HeaderValue, Method, StatusCode, header},
    middleware::{self, Next},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
};
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
    clipboard::{
        ClipboardHistoryEntry, ClipboardPayload, ensure_upload_dir, read_clipboard_history,
        read_remote_clipboard, write_clipboard_history, write_remote_clipboard,
    },
    ffmpeg,
    media::MediaHub,
    session,
    settings::{AudioStreamConfig, CodecKind, EncodePreference, ServerConfig, StreamConfig},
};

const INDEX_HTML: &str = include_str!("../web/index.html");
const APP_JS: &str = include_str!("../web/app.js");
const APP_CSS: &str = include_str!("../web/app.css");
const VIDEO_RENDERER_WORKER_JS: &str = include_str!("../web/video_renderer_worker.js");

#[derive(Clone)]
struct AppState {
    server: ServerConfig,
    auth: Arc<AuthTracker>,
    camera: CameraRelay,
    media: MediaHub,
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
struct WsQuery {
    codec: Option<CodecKind>,
    bitrate_kbps: Option<u32>,
    audio_bitrate_kbps: Option<u32>,
    fps: Option<u32>,
    encode_preference: Option<EncodePreference>,
    passwd: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CameraStopRequest {
    session_id: String,
}

pub async fn run(server: ServerConfig) -> Result<()> {
    let bind = server.bind.clone();
    let camera = CameraRelay::new(PathBuf::from(&server.upload_dir));
    let media = MediaHub::new(server.clone());
    if let Err(err) = ffmpeg::warm_audio_stack().await {
        warn!("audio bootstrap failed during startup: {err}");
    }
    let state = Arc::new(AppState {
        server,
        auth: Arc::new(AuthTracker::new()),
        camera,
        media: media.clone(),
    });
    let app = Router::new()
        .route("/", get(index))
        .route("/app.js", get(js))
        .route("/app.css", get(css))
        .route("/video_renderer_worker.js", get(video_renderer_worker_js))
        .route("/healthz", get(healthz))
        .route("/api/auth", get(auth_check).post(auth_login))
        .route("/api/encoders", get(encoders))
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
        .map(|listener| listener.local_addr().map(|addr| format!("http://{addr}")))
        .collect::<Result<Vec<_>, _>>()?;
    info!("listening on {}", listen_urls.join(", "));
    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    tokio::spawn(async move {
        shutdown_signal().await;
        let _ = shutdown_tx.send(true);
    });
    let result = try_join_all(listeners.into_iter().map(|listener| {
        let app = app.clone();
        let mut shutdown_rx = shutdown_rx.clone();
        async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = shutdown_rx.changed().await;
                })
                .into_future()
                .await
        }
    }))
    .await;
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
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    Ok(Json(EncodersResponse { options }))
}

async fn ws(
    headers: HeaderMap,
    ws: WebSocketUpgrade,
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
    let config = StreamConfig {
        codec: query.codec.unwrap_or(CodecKind::H264),
        bitrate_kbps: query.bitrate_kbps.unwrap_or(4_000),
        fps: query.fps.unwrap_or(16),
        encode_preference: query.encode_preference.unwrap_or_default(),
    }
    .normalized();
    let audio_config = AudioStreamConfig {
        bitrate_kbps: query.audio_bitrate_kbps.unwrap_or(128),
    }
    .normalized();
    ws.on_upgrade(move |socket| async move {
        if let Err(err) = session::handle_socket(
            socket,
            state.server.clone(),
            state.media.clone(),
            config,
            audio_config,
        )
        .await
        {
            warn!(error = %err, "websocket session ended with an error");
        }
    })
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
        .enqueue_mp4_chunk(&session_id, seq, bytes)
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
