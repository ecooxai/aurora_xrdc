#!/bin/sh
# Build-host-only isolated musl build. Runtime startup NEVER runs this script.
# Usage from the source checkout: sh tools/portable/rebuild-vendor.sh [fresh-work-directory]
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
WORK=${1:-"$ROOT/.agentwork/vendor-rebuild"}
mkdir -p "$WORK"
WORK=$(CDPATH= cd -- "$WORK" && pwd)
ALPINE="$WORK/alpine"
if [ -e "$ALPINE" ]; then echo 'Use a fresh build directory (prevents patches being applied twice).' >&2; exit 1; fi
python3 "$ROOT/tools/portable/fetch.py" "$WORK/src"
curl -fL --retry 3 https://dl-cdn.alpinelinux.org/alpine/v3.24/releases/x86_64/alpine-minirootfs-3.24.2-x86_64.tar.gz -o "$WORK/alpine.tar.gz"
printf '%s  %s\n' c5ca053cfe1d85c5b96dff8b9bc57045f7f184a30ffb6b65776409ca90388677 "$WORK/alpine.tar.gz" | sha256sum -c -
mkdir -p "$ALPINE"
sudo tar -xzf "$WORK/alpine.tar.gz" -C "$ALPINE"
sudo cp /etc/resolv.conf "$ALPINE/etc/resolv.conf"
sudo mkdir -p "$ALPINE/etc/ssl/certs" "$ALPINE/build" "$ALPINE/proc" "$ALPINE/dev/shm"
sudo cp /etc/ssl/certs/ca-certificates.crt "$ALPINE/etc/ssl/certs/"
for device in null zero random urandom; do sudo cp -a "/dev/$device" "$ALPINE/dev/"; done
sudo cp /proc/cpuinfo /proc/meminfo "$ALPINE/proc/"
sudo chmod 1777 "$ALPINE/tmp" "$ALPINE/dev/shm"
sudo chroot "$ALPINE" apk add --no-cache build-base bash curl git cmake meson ninja python3 pkgconf autoconf automake libtool linux-headers zlib-dev zlib-static expat-dev expat-static libx11-dev libx11-static libxcb-dev libxcb-static libxau-dev libxdmcp-dev libxext-dev libxext-static libxfixes-dev libxrender-dev pixman-dev pixman-static freetype-dev freetype-static xorgproto util-macros libfontenc-dev libxtst-dev libxi-dev dbus-dev libsndfile-dev libsndfile-static json-c-dev libogg-dev libogg-static flac-dev flac-static libvorbis-dev libvorbis-static opus-dev speexdsp-dev openssl-dev openssl-libs-static busybox-static nasm x264-dev libvpx-dev gettext-dev xtrans flex bison
sudo cp "$WORK/src/"*.tar.* "$ALPINE/build/"
sudo cp -a "$WORK/src/tinyx" "$WORK/src/pulseaudio" "$WORK/src/xdotool" "$ALPINE/build/"
sudo cp "$ROOT/tools/portable/"*.sh "$ROOT/tools/portable/"*.py "$ROOT/tools/portable/tinyx-headless.c" "$ALPINE/build/"
for step in build-base build-tinyx build-media; do
  sudo chroot "$ALPINE" sh "/build/$step.sh" > "$WORK/$step.log" 2>&1 || { tail -80 "$WORK/$step.log"; exit 1; }
done
# WM stays in a normal Rust build environment, but targets musl and uses its pinned lockfile.
python3 "$ROOT/tools/portable/patch-wm.py" "$WORK/src/aurora-wm"
rustup target add x86_64-unknown-linux-musl
export CC_x86_64_unknown_linux_musl=${CC_x86_64_unknown_linux_musl:-musl-gcc}
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER="$CC_x86_64_unknown_linux_musl"
RUSTFLAGS='-C target-cpu=x86-64' cargo build --manifest-path "$WORK/src/aurora-wm/Cargo.toml" --locked --release --target x86_64-unknown-linux-musl --target-dir "$WORK/wm-target" --bin aurora-wm
DEST="$ROOT/vendor/x86_64/bin"
mkdir -p "$DEST"
for binary in Xtiny pulseaudio pactl pacat dbus-daemon dbus-send ffmpeg ffprobe xdotool; do cp "$ALPINE/out/bin/$binary" "$DEST/"; done
cp "$ALPINE/bin/busybox.static" "$DEST/busybox"
cp "$WORK/wm-target/x86_64-unknown-linux-musl/release/aurora-wm" "$DEST/"
strip "$DEST/"*
sudo chroot "$ALPINE" apk info -v > "$ROOT/vendor/licenses/alpine-build-packages.txt"
python3 - "$DEST" "$ROOT" <<'PY'
import importlib.util,sys
from pathlib import Path
spec=importlib.util.spec_from_file_location('audit',Path(sys.argv[2])/'tools/portable/package.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
for path in Path(sys.argv[1]).iterdir():
    if path.is_file():m.audit_elf(path);print(path.name,'static ELF verified')
PY
printf 'Vendored helpers rebuilt. Run ./build-dist.sh and the test suite next.\n'
