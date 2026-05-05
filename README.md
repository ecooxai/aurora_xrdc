# vibe_rdesk

`vibe_rdesk` is a Rust-based remote desktop MVP for X11 systems. It serves a browser UI, captures the desktop with FFmpeg, streams video to the browser over WebSocket/WebCodecs, and relays input back to the host with uinput, X11 injection, and `xdotool` fallbacks.

## Features

- H.264 streaming by default.
- Best-effort H.265 and VP8 streaming.
- One shared FFmpeg video capture pipeline for all connected browsers, with shared codec/FPS/bitrate changes.
- Browser-side pointer, click, wheel, and keyboard input.
- Remote and local clipboard sync.
- Browser camera uplink as MP4 or WebM chunks, streamed into a server-side virtual camera named `VibeRDesk Camera`.
- Browser debug overlay with latency, FPS, codec, encoder mode, CPU, memory, and network stats.
- Service worker caching for the web client.

## Requirements

- Rust toolchain.
- FFmpeg installed and available on `PATH`.
- `xdotool` installed and available on `PATH` for keyboard input and input fallback paths.
- `/dev/uinput` access is optional, but enables low-latency relative pointer movement and smooth wheel events.
- An X11 session on the host machine.
- A server password passed at startup.
- `v4l2loopback` installed on the server host if you want camera uplink.

## Run

```bash
cargo run -- --passwd <password>
```

The server listens on `0.0.0.0:8001` and `[::]:8001` by default.

## Configuration

### Command-line flags

- `--passwd <password>`: required server password.
- `-p, --port <port>`: override the listening port.

### Environment variables

- `VIBE_RDESK_BIND`: bind address list, defaults to `0.0.0.0:8001,[::]:8001`.
- `DISPLAY`: X11 display, defaults to `:0.0`.
- `VIBE_RDESK_UPLOAD_DIR`: upload directory, defaults to `~/Desktop`. A leading `~/` is expanded against the server user's home directory.

## Camera Uplink

The camera toggle records browser camera video into 1-second MP4 or WebM chunks and uploads them to the server. The server checks for an existing virtual camera named `VibeRDesk Camera`, creates it with `modprobe v4l2loopback ...` when needed, then keeps one FFmpeg relay open for the browser session and streams the uploaded media into that device.

The server keeps a black placeholder feed open while no browser camera is active so Chromium and other apps can detect the virtual camera before the browser starts sending camera frames. After the placeholder starts, the server also refreshes the udev camera capability tag and restarts WirePlumber best-effort so PipeWire/portal camera lists see the virtual camera as a source.

If the host cannot create `/dev/video*` automatically, install `v4l2loopback` and ensure the server process has permission to run `modprobe`.

Browsers only expose local camera devices to secure origins. Use `https://...` or `http://localhost:...` when enabling camera uplink.

## Browser UI

Open the server URL in a browser after starting the app. The client supports:

- Selecting codec, bitrate, and FPS.
- Applying stream setting changes globally across connected clients without spawning another video FFmpeg.
- Capturing pointer and keyboard input when the canvas is active.
- Clipboard push/pull through the clipboard cards.
- `?debug=1` for the debug overlay.

## Development

```bash
./test.sh
./dev.sh
```

`run.sh` and `dev.sh` start a headless X11 display when `DISPLAY` is unavailable. They default to `jwm`, and you can choose another desktop/session or a terminal with `--launcher`:

```bash
./dev.sh --launcher xfce4-session --passwd <password>
./run.sh --launcher xterm --passwd <password>
./run.sh --launcher "openbox-session" --passwd <password>
```

On a machine with a real X11 display, use `--headless` to force the app onto the Xvfb display instead of reusing the host `DISPLAY`. Headless script launches default to X11-targeted input so wheel events stay on the virtual display; set `VIBE_RDESK_INPUT_BACKEND=uinput` only when you intentionally want host-seat uinput injection.

```bash
./run.sh --headless --launcher xfce4-session --passwd <password>
```

## Project layout

- `src/`: Rust server, session, streaming, clipboard, and input code.
- `web/`: browser client, service worker, styling, and HTML shell.
- `doc/`: task notes and project overview.
