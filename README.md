# vibe_rdesk

`vibe_rdesk` is a Rust-based remote desktop MVP for X11 systems. It serves a browser UI, captures the desktop with FFmpeg, streams video to the browser over WebSocket/WebCodecs, and relays pointer and keyboard input back to the X server with `xdotool`.

## Features

- H.264 streaming by default.
- Best-effort H.265 and VP8 streaming.
- Browser-side pointer, click, wheel, and keyboard input.
- Remote and local clipboard sync.
- Browser debug overlay with latency, FPS, codec, encoder mode, CPU, memory, and network stats.
- Service worker caching for the web client.

## Requirements

- Rust toolchain.
- FFmpeg installed and available on `PATH`.
- `xdotool` installed and available on `PATH`.
- An X11 session on the host machine.
- A server password passed at startup.

## Run

```bash
cargo run -- --passwd <password>
```

The server listens on `0.0.0.0:8001` by default.

## Configuration

### Command-line flags

- `--passwd <password>`: required server password.
- `-p, --port <port>`: override the listening port.

### Environment variables

- `VIBE_RDESK_BIND`: bind address, defaults to `0.0.0.0:8001`.
- `DISPLAY`: X11 display, defaults to `:0.0`.
- `VIBE_RDESK_UPLOAD_DIR`: upload directory, defaults to `~/Desktop`. A leading `~/` is expanded against the server user's home directory.

## Browser UI

Open the server URL in a browser after starting the app. The client supports:

- Selecting codec, bitrate, and FPS.
- Capturing pointer and keyboard input when the canvas is active.
- Clipboard push/pull through the clipboard cards.
- `?debug=1` for the debug overlay.

## Development

```bash
./test.sh
./dev.sh
```

## Project layout

- `src/`: Rust server, session, streaming, clipboard, and input code.
- `web/`: browser client, service worker, styling, and HTML shell.
- `doc/`: task notes and project overview.
