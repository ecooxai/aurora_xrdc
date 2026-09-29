# Aurora XRDC — portable Linux release

Extract the archive and run `aurora` from any directory. The package contains the server,
web client, TinyX, PulseAudio, D-Bus, FFmpeg, XTEST fallback, Aurora WM, and a small shell.
There is no install step, package-manager call, service-manager call, or shared-library search.
Run as an ordinary Linux user, not root. Keep the extracted directory together.

```sh
tar -xzf aurora-xrdc-0.2.0-linux-x86_64.tar.gz
cd aurora-xrdc-0.2.0-linux-x86_64
# Use an existing private password file; restrict it to mode 600.
./aurora --passwd-file "$HOME/.config/aurora/password" --port 18443
```

Native HTTPS is on by default. An ephemeral self-signed localhost certificate is generated
without OpenSSL. For a trusted hostname, set both `VIBE_RDESK_TLS_CERT` and `VIBE_RDESK_TLS_KEY`
to your certificate/key files. Behind an HTTPS reverse proxy, use `--https no`; the proxy
must support WebSocket upgrade. Use `--localhost yes` when the proxy runs on the same host.
Raw unencrypted HTTP across an untrusted network is not suitable for a remote desktop.
Passwords can also be supplied with `--passwd TEXT`, but that initial command is visible
in process arguments; password files are preferable. No default password is installed.

## Service selection

`--headless auto` first probes `DISPLAY`, or `:0` when DISPLAY is unset. A working X11/XTEST
connection is required; an existing socket alone is not treated as success. The host's
Xauthority is preserved. If access fails, a private TinyX memory framebuffer is started
on a free display. Use `--headless no` to require host X11 instead of falling back, or
`--headless yes` to explicitly request a private desktop. Existing X sockets and lock
files are never removed. The private server disables TCP and requires a random Xauthority
cookie; it does not use `-ac`, a physical input device, a framebuffer device, or a VT.

Audio defaults to `--audio auto`: reuse a reachable PulseAudio/PipeWire Pulse socket;
otherwise start the static PulseAudio fallback with an authenticated private UNIX socket
and a 48 kHz stereo null sink. Host default sinks and active audio streams are not moved
at startup. Microphone devices are created only when microphone input is enabled.
`--audio yes` forces an isolated fallback; `--audio no` disables fallback setup.
The fallback deliberately includes only native UNIX protocol, null source/sink,
remap source/sink, loopback, sine source and always-sink modules. It cannot load `.so` files
or provide ALSA/Bluetooth hardware drivers. A normal host audio service remains the preferred
way to use physical audio hardware.

D-Bus similarly reuses `DBUS_SESSION_BUS_ADDRESS` or a reachable `/run/user/UID/bus`.
A private session bus is used only when needed. Its machine-id is copied from a valid host
ID or generated in the private runtime directory, never written to `/etc` or `/var/lib`; `--dbus yes` forces it and `--dbus no`
disables fallback setup. No host system bus, audio daemon, display server or systemd unit
is started, stopped, restarted, or reconfigured by this supervisor.

Aurora WM starts only on a private desktop. `--launcher none` leaves it bare, and
`--launcher 'COMMAND'` explicitly requests a custom desktop command (which requires the
host shell). Logs and `session.json` are written to a fresh mode-700 directory beneath
`/tmp/aurora-UID` by default. `--state-dir PATH` selects another private parent.
The session file records DISPLAY, Xauthority, audio and bus paths for launching additional
applications into the same desktop. Ctrl-C or SIGTERM terminates only owned process groups.

## Input and latency

Pointer and keyboard edges keep their ordered connection. Wheel injection runs on an
independent worker/connection and never awaits X11/subprocess I/O in the input receiver.
The worker has a 16-event queue, discards scroll older than 100 ms, and cancels pending
work on reset/disconnect. Subprocess fallback has zero artificial click delay and a 500 ms
kill/reap timeout. Slow scroll cannot create an unbounded per-event task backlog.

The browser batches wheel deltas every 8 ms, bounds accumulated deltas, and pauses wheel
writes when the transport buffer exceeds 32 KiB. Keyboard and pointer events do not await
this queue. WebTransport reports queued bytes instead of hiding an unbounded write chain.
Display wake uses native X11/DPMS requests; it no longer launches `xset`/`dbus-send` or
moves the user's pointer as a wake-up trick. Private displays default to XTEST rather
than the machine-wide `/dev/uinput` seat.

## Scope and requirements

This archive is **Linux x86_64**, built for the baseline x86-64 CPU with runtime SIMD
selection in codecs. It is not an ARM64 or universal Unix release. A Linux kernel with
UNIX sockets, shared memory, `/dev/null`, and `/dev/urandom`, and writable private runtime
storage is needed. No target system libc or dynamic loader is needed. `/proc` improves
system statistics but is not part of the shipped application.

TinyX is a small software X11 fallback, not a replacement for a modern accelerated Xorg
session: it lacks XKB, XInput2, GLX and modern desktop integration. Applications requiring
those extensions need a host X server. The archive does not include a browser or an entire
Linux desktop distribution. Host NetworkManager, Bluetooth, power controls, `xdg-open`,
PDF viewers and GPU drivers are optional desktop integrations, not bundled services.

The optional 36 MiB `ffprobe` diagnostic is retained in the source checkout but omitted
from the normal archive because neither runtime uses it. Build with `--with-diagnostics`
to include it.

The bundled FFmpeg provides CPU H.264/H.265/VP8/VP9, AAC/Opus and desktop/audio capture.
Hardware encoders and AV1 encoders depend on an appropriate host FFmpeg/driver stack.
Set `AURORA_FFMPEG=/absolute/path/to/ffmpeg` to opt into that host binary; otherwise the bundled static encoder is used.
Virtual camera loopback requires an existing host v4l2loopback device; portable mode will
not install or load kernel modules. Standard remote desktop, clipboard, audio and input
do not require that optional camera feature.

## Build, audit and troubleshooting

`manifest.json` records every executable's SHA-256 and ELF linkage audit. `SHA256SUMS`
covers the package files. Packaging rejects any PT_INTERP or DT_NEEDED entry. These checks
are stricter than copying a loader and shared libraries next to a supposedly static binary.

In the source checkout, `./build-dist.sh` rebuilds the Rust release and creates the archive.
Rust, a musl C compiler and Python 3 are **build-host** tools only. The vendored helper
rebuild scripts, source pins and licenses document their separate musl build. Runtime
startup never downloads anything. Original launchers are preserved in `tools/legacy/`
for comparison; they are not used by the portable release.

Check the printed log directory after an error. `./aurora --probe-display :0` checks X11
access without starting services. A busy or inaccessible host display is never taken over;
fix DISPLAY/XAUTHORITY or explicitly choose a private fallback.

## Validation commands

`./test.sh` runs Rust unit tests, browser wheel-queue tests, syntax validation and the
ELF audit rejection tests. `python3 tests/helpers_smoke.py` tests the static X/audio helpers.
The live test scripts operate only on an explicitly owned QA instance: set `.output/qa-release`
to its extracted package and `.output/qa-session` to its printed session directory, place
the QA password in `.output/qa-password` (mode 600), and use port 19990. Run
`tests/live_qa.py --session .output/qa-session --password-file .output/qa-password`,
`tests/input_disconnect_qa.py`, `tests/browser_qa.py`, and `tests/lifecycle_qa.py` with Python.
Browser tests need Playwright/Chromium on the test host. Lifecycle tests need build-host
sudo solely to construct and enter an empty root; the desktop inside runs as UID 1001.
These test tools are never dependencies on the deployed server.

To rebuild vendored helpers from pinned source archives/commits, run
`sh tools/portable/rebuild-vendor.sh /absolute/fresh/build-directory` in the source checkout.
The Alpine build-only package inventory is included with the notices. The source downloads
are hash/revision checked; the distribution repository for build-only packages can evolve,
so this is not a promise of bit-identical builds across future toolchain updates.
