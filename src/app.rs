use anyhow::Result;
use axum::{
    Router,
    extract::{
        Query, State,
        ws::{WebSocketUpgrade},
    },
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use serde::Deserialize;
use std::sync::Arc;
use tracing::info;

use crate::{
    session,
    settings::{CodecKind, ServerConfig, StreamConfig},
};

const INDEX_HTML: &str = include_str!("../web/index.html");
const APP_JS: &str = include_str!("../web/app.js");
const APP_CSS: &str = include_str!("../web/app.css");
const SW_JS: &str = include_str!("../web/sw.js");

#[derive(Clone)]
struct AppState {
    server: ServerConfig,
}

#[derive(Debug, Deserialize)]
struct WsQuery {
    codec: Option<CodecKind>,
    bitrate_kbps: Option<u32>,
    fps: Option<u32>,
}

pub async fn run(server: ServerConfig) -> Result<()> {
    let bind = server.bind.clone();
    let app = Router::new()
        .route("/", get(index))
        .route("/app.js", get(js))
        .route("/app.css", get(css))
        .route("/sw.js", get(sw))
        .route("/healthz", get(healthz))
        .route("/ws", get(ws))
        .with_state(Arc::new(AppState { server }));
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

async fn sw() -> Response {
    asset("application/javascript; charset=utf-8", SW_JS)
}

async fn healthz() -> impl IntoResponse {
    "ok"
}

async fn ws(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Query(query): Query<WsQuery>,
) -> impl IntoResponse {
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

fn asset(content_type: &'static str, body: &'static str) -> Response {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    (StatusCode::OK, headers, body).into_response()
}
