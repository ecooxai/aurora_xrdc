# Portable Aurora validation report

Baseline: ecooxai/aurora_xrdc `1ca4ee3`. Tested on Linux x86_64, kernel 6.6.122+,
8 virtual CPUs in the current Colab high-RAM development instance.

- Baseline Rust tests: 54 passed.
- Updated Rust tests: 66 passed (57 server/input and 9 supervisor).
- JavaScript wheel queue: 5 passed; app syntax check passed.
- Deterministic headless mapping tests passed for CLI and scripts: port `11220` maps to
  TinyX `DISPLAY=:220`; explicit `--display` overrides the mapping and collisions fail closed.
- Debian 11 (`debian:11-slim`, bullseye) chroot QA passed at port 11220 / display :220.
  The release started TinyX, private D-Bus, static PulseAudio and the server as an unprivileged
  UID, captured a real 1280x720 X11 frame with the bundled FFmpeg, and authenticated password
  `2208`. DNS and apt sources were disabled before startup; dpkg status, apt lists, and apt
  archives were byte-for-byte unchanged after the run: zero package downloads/installs occurred.
- ELF audit tests: 4 passed, including rejection of runtime loaders/shared libraries/wrong architecture.
- Helper integration: TinyX XTEST accepted valid auth and rejected missing auth;
  static PulseAudio native protocol worked, built-in modules loaded, external modules rejected.
- Authenticated HTTP health/API/asset tests passed; protected API rejected unauthenticated access.
- Real 1280x720 TinyX capture encoded and decoded with H.264, H.265, VP8 and VP9.
- Real monitor audio playback/capture encoded and decoded with AAC and Opus; RMS checks were nonzero.
- 2,000 wheel messages interleaved with 50 key pairs and 50 click pairs: all key edges,
  button releases and 50 pongs observed. Latest localhost p50 1.74 ms, p95 3.93 ms,
  maximum 5.71 ms during concurrent build activity. These are local test measurements,
  not WAN performance guarantees or a controlled before/after benchmark.
- Abrupt transport abort released the held key and button within 21.4 ms. The earlier
  implementation failed this test; the final input cleanup runs on the error path too.
- Chromium desktop/mobile smoke passed with rendered nonblank 1280x720 video, zero
  JavaScript errors, zero HTTP 5xx, and no horizontal overflow at 390 px viewport width.
  WebSocket, WebRTC data-channel and WebRTC media modes were exercised. WebRTC connection
  and ICE states were explicitly checked as connected; media mode had a received video track.
- Login became visible in approximately 150–214 ms in local browser runs.
- Native HTTPS health passed with internally generated TLS keys/certificates.
- Host-reuse tests started no fallback processes, preserved the host's default audio sink,
  and left the test host X11, audio and session D-Bus services alive after shutdown.
- Empty-root test passed with no host shell, shared-library directories, installed runtime
  packages, or /proc. TinyX, PulseAudio, D-Bus, Aurora WM and the server started as UID 1001.
  The private D-Bus machine-id query is also checked without `/etc/machine-id`. Owned processes shut down successfully. /dev/null, /dev/urandom and writable runtime
  directories were provided as kernel/runtime facilities, not installed packages.

Packaging performs a fresh PT_INTERP/DT_NEEDED check on every included ELF and records
SHA-256 hashes. The minimal archive omits ffprobe (only needed for tests/diagnostics).
The optional diagnostic build includes that additional static executable.

Not validated on ARM64, other kernels, or every distribution/browser. GPU/AV1 encoding
requires an explicitly selected suitable host FFmpeg and drivers; no GPU driver, physical
audio stack, NetworkManager, Bluetooth service, or v4l2loopback module is installed by this
release. TinyX does not implement modern XKB/XInput2/GLX functionality. WebTransport-over-QUIC
and physical camera/microphone devices are not claimed as end-to-end tested by this report.

Detailed execution logs, JSON results, sample streams and screenshots are retained in
`.output/` in the source checkout. Runtime credentials are deliberately excluded from
both the release archive and the report.
