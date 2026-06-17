# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

**Development (auto-rebuild on file change):**
```bash
bash dev.sh --passwd passwd
```
Runs `test.sh` first, then starts the debug binary and watches `src/`, `web/`, and `Cargo.toml` for changes. Starts Xvfb + a window manager if no `DISPLAY` is available. Generates a self-signed TLS cert in `ssl_keys/` if absent.

**Starting the dev server (standard invocation for Claude):**

Use port `9990` and display `:0`. Kill any process already holding port 9990, then ensure display `:0` is running before launching:

```bash
# 1. Free port 9990 if occupied
fuser -k 9990/tcp 2>/dev/null || true
sleep 0.5

# 2. Start Xvfb on :0 if the display is not already available
if ! DISPLAY=:0 xdpyinfo >/dev/null 2>&1; then
    Xvfb :0 -screen 0 1280x720x24 -ac -nolisten tcp &
    sleep 1
fi

# 3. Launch dev server
DISPLAY=:0 bash dev.sh --passwd passwd --port 9990
```

- Always kill the existing process on port 9990 before starting a new one.
- If `DISPLAY=:0` is unavailable, start Xvfb on `:0` (not a different display number).
- Pass `--port 9990` explicitly; do not rely on the default port.

**Production:**
```bash
bash run.sh --passwd passwd --port 18443 --https yes --headless no
```

**Build (optimized release):**
```bash
bash build.sh
```
Sets `target-cpu=native` by default. Release binary ends up in `target/release/vibe_rdesk`; the intended verification path is `/var/tmp/vibe_rdesk-build/release/vibe_rdesk`.

**Test + debug build (what `dev.sh` calls):**
```bash
bash test.sh        # cargo test && cargo build
```

**Run a single test:**
```bash
cargo test <test_name>
```

**Logging:**
```bash
RUST_LOG=debug bash dev.sh --passwd passwd
```

## Architecture

`vibe_rdesk` is a Rust/Tokio server that captures an X11 desktop with FFmpeg and streams H.264/H.265/VP8 video (plus Opus audio) to a browser over WebSocket or WebTransport. All browser interaction goes through the same `session::handle_socket` function regardless of transport.

### Transport abstraction (`src/transport.rs`, `src/webtransport.rs`)

Both the WebSocket (`/ws`) and WebTransport (QUIC/UDP) paths produce a `(WireSink, WireStream)` pair of boxed trait objects carrying `axum::extract::ws::Message` values. Session logic never knows which transport it's on. WebTransport video additionally uses a dedicated unidirectional QUIC stream with a keyframe-drop queue to avoid head-of-line blocking.

### Media pipeline (`src/media.rs`, `src/ffmpeg.rs`, `src/streamer.rs`, `src/audio.rs`, `src/audio_streamer.rs`)

`MediaHub` owns one shared FFmpeg video capture process and one audio capture process for all connected clients. Clients subscribe to a `broadcast` channel of `StreamFrame`s. When the first subscriber joins the hub starts FFmpeg; when the last one leaves it stops. Stream setting changes (codec, bitrate, FPS) restart the shared FFmpeg process and apply to all clients. Video frames pass through `src/annexb.rs` for Annex-B / IVF framing before forwarding.

### Session (`src/session.rs`)

Per-connection state machine. Handles the JSON hello/stats/pong/clipboard protocol on the control channel and dispatches pointer, keyboard, and wheel events to the input backends. `SessionRole` allows a client to connect as `all`, `control`, `video`, `audio`, `mic`, or `input`, which splits traffic across separate WebTransport sessions in the browser.

### Input backends (`src/uinput.rs`, `src/x11_input.rs`, `src/ffmpeg.rs`)

Input prefers uinput (`/dev/uinput`) for relative pointer motion and smooth wheel events. Falls back to X11/xtest (`x11rb`) for pointer and clicks, then to `xdotool` subprocess for keyboard. `dev.sh` tries to grant uinput access with `setfacl` or `chown` at startup.

### HTTP layer (`src/app.rs`)

Axum router. `AppState` holds `ServerConfig`, `AuthTracker`, `CameraRelay`, `MediaHub`, and `ClientManager`. Auth uses cookie session tokens (UUID) with a 20-attempt / 1-hour lockout. All web assets (`web/`) are compiled into the binary with `include_str!`/`include_bytes!`. The WebTransport endpoint binds on the same UDP port as the first TCP bind address and advertises its cert hash via `/api/wt-info`.

### Camera uplink (`src/camera.rs`)

Browser records 1-second MP4/WebM chunks and POSTs them to `/api/camera/chunk`. The server creates a `v4l2loopback` virtual camera (`VibeRDesk Camera`) on demand and keeps a single FFmpeg relay open per session to feed chunks into the virtual device.

### Client manager (`src/client_manager.rs`)

Tracks connected browser sessions by client ID. Supports close-other-clients for single-seat enforcement.

### Web client (`web/`)

Vanilla JS. `app.js` opens a WebSocket (or WebTransport when available), decodes video with the WebCodecs API, and renders frames via a OffscreenCanvas worker (`video_renderer_worker.js`). `sw.js` is a service worker for caching. The `?debug=1` query parameter enables the stats overlay.

### Settings (`src/settings.rs`)

`ServerConfig` is parsed from CLI args. `StreamConfig` holds codec, bitrate, FPS, and encoder tuning. `AudioStreamConfig` holds audio bitrate.

## Key constraints

- Do not edit `doc/vibetask.md` unless the user explicitly permits a specific section.
- Keep modules small and focused.
- Do not use browser `alert()` or `prompt()`; use in-app UI feedback.
- Keep documentation in `doc/`.
- Rust edition 2024 (`Cargo.toml`); use `cargo test` before claiming a task complete.
