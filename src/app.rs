use anyhow::Result;
use axum::{
    Router,
    extract::{
        Multipart, Query, State,
        ws::{WebSocketUpgrade},
    },
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::{get, post},
    Json,
};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tracing::{info, warn};
use tokio::sync::Mutex;

use crate::{
    clipboard::{
        ClipboardHistoryEntry, ClipboardPayload, ensure_upload_dir, read_clipboard_history,
        read_remote_clipboard, write_clipboard_history, write_remote_clipboard,
    },
    ffmpeg,
    session,
    settings::{CodecKind, ServerConfig, StreamConfig},
};

const INDEX_HTML: &str = include_str!("../web/index.html");
const APP_JS: &str = include_str!("../web/app.js");
const APP_CSS: &str = include_str!("../web/app.css");

#[derive(Clone)]
struct AppState {
    server: ServerConfig,
    auth: Arc<AuthTracker>,
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
            Some(actual) if actual == expected => {
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
}

#[derive(Debug, Deserialize)]
struct AuthQuery {
    passwd: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WsQuery {
    codec: Option<CodecKind>,
    bitrate_kbps: Option<u32>,
    fps: Option<u32>,
    passwd: Option<String>,
}

pub async fn run(server: ServerConfig) -> Result<()> {
    let bind = server.bind.clone();
    if let Err(err) = ffmpeg::warm_audio_stack().await {
        warn!("audio bootstrap failed during startup: {err}");
    }
    let app = Router::new()
        .route("/", get(index))
        .route("/app.js", get(js))
        .route("/app.css", get(css))
        .route("/healthz", get(healthz))
        .route("/api/auth", get(auth))
        .route("/ws", get(ws))
        .route("/api/upload", post(upload))
        .route("/api/clipboard/history", get(get_clipboard_history).post(set_clipboard_history))
        .route("/api/clipboard/remote", get(get_remote_clipboard).post(set_remote_clipboard))
        .with_state(Arc::new(AppState {
            server,
            auth: Arc::new(AuthTracker::new()),
        }));
    let listener = tokio::net::TcpListener::bind(&bind).await?;
    info!("listening on http://{bind}");
    axum::serve(listener, app).await?;
    Ok(())
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

async fn healthz() -> impl IntoResponse {
    "ok"
}

async fn auth(
    Query(auth): Query<AuthQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<&'static str, (StatusCode, String)> {
    state
        .auth
        .require_passwd(&state.server.passwd, auth.passwd.as_deref())
        .await?;
    Ok("ok")
}

async fn ws(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Query(query): Query<WsQuery>,
) -> Response {
    if let Err(response) = state
        .auth
        .require_passwd(&state.server.passwd, query.passwd.as_deref())
        .await
    {
        return response.into_response();
    }
    let config = StreamConfig {
        codec: query.codec.unwrap_or(CodecKind::H264),
        bitrate_kbps: query.bitrate_kbps.unwrap_or(4_000),
        fps: query.fps.unwrap_or(16),
    }
    .normalized();
    ws.on_upgrade(move |socket| async move {
        if let Err(err) = session::handle_socket(socket, state.server.clone(), config).await {
            let _ = err;
        }
    })
}

#[derive(Debug, Serialize)]
struct UploadResponse {
    saved_as: String,
}

async fn upload(
    Query(auth): Query<AuthQuery>,
    State(state): State<Arc<AppState>>,
    mut multipart: Multipart,
) -> Result<Json<UploadResponse>, (StatusCode, String)> {
    state
        .auth
        .require_passwd(&state.server.passwd, auth.passwd.as_deref())
        .await?;
    let dir = PathBuf::from(&state.server.upload_dir);
    ensure_upload_dir(&dir)
        .await
        .map_err(internal_error)?;
    while let Some(field) = multipart.next_field().await.map_err(|err| internal_error(err.into()))? {
        let file_name = field.file_name().map(sanitize_file_name).unwrap_or_else(|| default_upload_name("upload.bin"));
        let bytes = field.bytes().await.map_err(|err| internal_error(err.into()))?;
        let path = unique_upload_path(&dir, &file_name);
        tokio::fs::write(&path, &bytes).await.map_err(|err| internal_error(err.into()))?;
        return Ok(Json(UploadResponse {
            saved_as: path.to_string_lossy().into_owned(),
        }));
    }
    Err((StatusCode::BAD_REQUEST, "missing file field".into()))
}

async fn get_remote_clipboard(
    Query(auth): Query<AuthQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<ClipboardPayload>, (StatusCode, String)> {
    state
        .auth
        .require_passwd(&state.server.passwd, auth.passwd.as_deref())
        .await?;
    let payload = read_remote_clipboard(&state.server.display)
        .await
        .map_err(internal_error)?;
    Ok(Json(payload))
}

async fn get_clipboard_history(
    Query(auth): Query<AuthQuery>,
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ClipboardHistoryEntry>>, (StatusCode, String)> {
    state
        .auth
        .require_passwd(&state.server.passwd, auth.passwd.as_deref())
        .await?;
    let history = read_clipboard_history().await.map_err(internal_error)?;
    Ok(Json(history))
}

async fn set_clipboard_history(
    Query(auth): Query<AuthQuery>,
    State(state): State<Arc<AppState>>,
    Json(history): Json<Vec<ClipboardHistoryEntry>>,
) -> Result<Json<Vec<ClipboardHistoryEntry>>, (StatusCode, String)> {
    state
        .auth
        .require_passwd(&state.server.passwd, auth.passwd.as_deref())
        .await?;
    write_clipboard_history(&history)
        .await
        .map_err(internal_error)?;
    Ok(Json(history))
}

async fn set_remote_clipboard(
    Query(auth): Query<AuthQuery>,
    State(state): State<Arc<AppState>>,
    Json(payload): Json<ClipboardPayload>,
) -> Result<Json<ClipboardPayload>, (StatusCode, String)> {
    state
        .auth
        .require_passwd(&state.server.passwd, auth.passwd.as_deref())
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
    let ext = Path::new(file_name).extension().and_then(|value| value.to_str());
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
            let err = tracker.require_passwd("secret", Some("wrong")).await.unwrap_err();
            assert_eq!(err.0, axum::http::StatusCode::UNAUTHORIZED);
        }

        let err = tracker.require_passwd("secret", Some("wrong")).await.unwrap_err();
        assert_eq!(err.0, axum::http::StatusCode::TOO_MANY_REQUESTS);
        assert!(err.1.contains("locked for 1 hour"));
    }

    #[tokio::test]
    async fn lockout_expires_and_success_resets_counter() {
        let tracker = AuthTracker::new_with_lockout(Duration::from_millis(25));

        for _ in 0..20 {
            let _ = tracker.require_passwd("secret", Some("wrong")).await;
        }

        tokio::time::sleep(Duration::from_millis(30)).await;

        tracker.require_passwd("secret", Some("secret")).await.unwrap();
        let err = tracker.require_passwd("secret", Some("wrong")).await.unwrap_err();
        assert_eq!(err.0, axum::http::StatusCode::UNAUTHORIZED);
    }
}
