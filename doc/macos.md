# apple_xrdc macOS port

This branch ports Aurora XRDC's browser remote-desktop server from Linux/X11 to macOS while keeping the existing web protocol and UI.

## Current macOS backends

- Screen capture: FFmpeg AVFoundation.
- Video encode: VideoToolbox H.264/H.265 when available, with software codec fallbacks.
- Pointer and keyboard input: CoreGraphics / Quartz CGEvent.
- Scroll input: Quartz scroll-wheel events.
- Clipboard: native `pbcopy` / `pbpaste` text clipboard.
- Display wake: `caffeinate`.

Linux-only uinput, X11, PulseAudio, and v4l2loopback paths are isolated behind platform-specific modules.

## Requirements

Install Rust and FFmpeg:

```sh
brew install ffmpeg
```

macOS must grant the process that runs apple_xrdc:

1. **Screen & System Audio Recording** permission for desktop capture.
2. **Accessibility** permission for remote keyboard and pointer control.

These are macOS TCC permissions. A normal unsigned command-line development binary may prompt on first use.

## Development

The GitHub workflow `.github/workflows/apple-xrdc-macos.yml` builds and tests this branch on `macos-latest`.

For a local Mac development session:

```sh
cargo run -- --passwd 'choose-a-password' --port 18443 --https yes
```

FFmpeg AVFoundation device numbering can vary. Override the input with:

```sh
AURORA_MAC_SCREEN_DEVICE='1:none' cargo run -- --passwd 'choose-a-password'
```

Use the AVFoundation probe to discover devices:

```sh
ffmpeg -hide_banner -f avfoundation -list_devices true -i ''
```

## Port status

Implemented in the first native port:

- HTTP/WebSocket/WebTransport server and existing browser client
- macOS screen-capture command path
- VideoToolbox encoder selection
- absolute/relative pointer movement
- mouse buttons
- wheel scrolling
- keyboard injection
- text clipboard sync
- macOS CI build/test environment

Still to implement or validate on a physical logged-in Mac:

- system-audio capture
- browser microphone injection as a virtual macOS audio device
- browser camera uplink as a CoreMediaIO camera extension
- PNG clipboard read/write via NSPasteboard
- TCC first-run UX and signed/notarized app packaging
- end-to-end interactive capture on a runner/session that exposes a GUI and grants Screen Recording

GitHub-hosted macOS runners are useful for compilation, unit tests, FFmpeg/AVFoundation probing, and platform API validation, but they should not be treated as proof that Screen Recording or Accessibility permissions are granted for a production desktop session.
