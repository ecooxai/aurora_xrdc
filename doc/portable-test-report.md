# Aurora XRDC v0.2.2 verification report

## Acceptance result

The requested port-to-display mapping and launcher cleanup are implemented and tested.
The official Debian 11 slim (bullseye) root filesystem ran the extracted Linux x86_64
release at port 11220 / display :220 as UID 1000, without installing or downloading
additional packages inside that root filesystem. The build was performed outside it.

## What was corrected

The shell scripts no longer contain their own competing option parser. The static Rust
launcher parses arguments once, treats leading-zero ports as decimal, preserves a password
such as `--port` as a value, honors a final explicit port over the dev default, and maps a
private display to port modulo 1000. `--display :N` or `:N.0` overrides it. Invalid screen
suffixes and occupied/dangling-symlink X paths are rejected; no host lock or display is removed.
Help/version do not build, build failures propagate, and paths/arguments with spaces are preserved.

Expanded tests found that the old bundled xdotool dereferenced a missing XKB keyboard map
on TinyX. Its static rebuild now uses core X11 keyboard mapping when XKB is absent and does
not read uninitialized XKB group state. Native server input, video and audio remain unchanged.
The helper's separate Alpine build environment is not the Debian runtime test environment.

## Automated checks

70 Rust tests passed (13 launcher, 57 server/input); 17 wrapper regression tests,
5 JavaScript wheel-queue tests and 4 ELF-audit rejection tests passed. All 12 bundled
executables are checked for absent PT_INTERP and DT_NEEDED entries. The manifest lists
no required runtime packages and records each binary's SHA-256.

The real-service matrix passed: run.sh 11220 -> :220; dev.sh 11221 -> :221;
011222 -> :222; explicit 12220 + --display :321.0 -> :321; automatic headless fallback;
option-looking password authentication; same-suffix collision rejected without affecting
the first instance; existing X/audio/D-Bus reused; all owned children reaped on shutdown.

Real 1280x720 X11 capture encoded and decoded with H.264, H.265, VP8 and VP9. Real monitor
audio encoded and decoded with AAC and Opus, with nonzero RMS. The mixed-input test sent
2,000 wheel messages interleaved with 50 key pairs, 50 click pairs and 50 pings. All key
pairs and pings arrived. Local ping median: 0.89 ms; p95: 2.04 ms;
maximum: 2.09 ms. These are local measurements, not WAN guarantees.

Headless Chromium rendered nonblank 1280x720 desktops using WebSocket, WebRTC data-channel
and WebRTC media modes. ICE/peer states were connected where applicable; media mode received
a video track. No JavaScript errors or mobile horizontal overflow were observed at 390px width.
These are functionality checks, not a claim of pixel-perfect window-manager rendering; a
pre-existing intermittent dark strip was visible in the WM settings sidebar screenshot.

## Debian 11 test provenance

Official image: debian:11-slim, linux/amd64.
Manifest: sha256:70509c95d1857a3704c0a5d92ee2e0adac95f612a9386889d70760bfd7c1ebba
Rootfs layer: sha256:4705738e5e0492efae5d2523aa791e06c852e2e1acb5e70a365cc08f9da0c556

The rootfs layer was downloaded and digest-verified by the host-side test driver. DNS and
apt repositories were disabled inside the test root. The dpkg status database, apt lists
and apt archive cache were unchanged before/after execution. Inside that root, TinyX,
PulseAudio, D-Bus machine-id queries, xdotool geometry/mouse/keyboard commands, FFmpeg capture
and HTTP authentication passed. No apt install, apt update or package download was run there.

A chroot shares its host kernel. This validates Debian 11 userspace on the current Linux
6.6.122+ host; it does not validate a separate Debian 11 kernel or ARM64, GPU drivers, physical
camera devices, or every Xorg extension. TinyX still lacks XKB/XInput2/GLX. WebTransport-over-QUIC
and all possible desktop applications are not claimed as end-to-end tested by this run.

## Reproduction

Run `./test.sh`, then `./build-dist.sh`. With test ports free, run:

```sh
python3 tests/launcher_runtime_test.py --package .output/dist/current --output .output/runtime-qa
python3 tests/browser_launcher_test.py --package .output/dist/current --output .output/browser-qa
sudo python3 tests/debian11_chroot_qa.py \
  --archive .output/dist/aurora-xrdc-0.2.2-linux-x86_64.tar.gz \
  --work "$PWD/.agentwork/debian11-fresh-qa" --port 11220
```

Python/Chromium/Rust/build tools are needed only on the build/test host. The deployed
Debian server does not need them. Current build packaging requires Python 3.11 or newer.
Runtime test password 2208 is fixture data, not an installed default in the release.
