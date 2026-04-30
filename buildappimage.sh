#!/usr/bin/env bash
set -euo pipefail

APP_NAME="vibe_rdesk"
APP_ID="io.github.vibe-rdesk"
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ARCH="$(uname -m)"
BUILD_DIR="${PROJECT_ROOT}/target/appimage"
APPDIR="${BUILD_DIR}/${APP_NAME}.AppDir"
DEB_CACHE="${BUILD_DIR}/deb-cache"
APPIMAGETOOL="${APPIMAGETOOL:-${BUILD_DIR}/appimagetool-${ARCH}.AppImage}"
VERSION="${VERSION:-$(awk -F '"' '/^version = / { print $2; exit }' "${PROJECT_ROOT}/Cargo.toml")}"
OUT_DIR="${OUT_DIR:-${PROJECT_ROOT}/dist}"
PORTABLE_CPU="${PORTABLE_CPU:-1}"
NATIVE_CPU="${NATIVE_CPU:-0}"

RUNTIME_PACKAGES=(
  ca-certificates
  curl
  dbus
  dbus-x11
  ffmpeg
  fontconfig
  jwm
  libasound2
  libgl1
  libpulse0
  libva2
  libvdpau1
  libx11-6
  libx11-xcb1
  libxau6
  libxcb1
  libxcb-shape0
  libxcb-shm0
  libxcb-xfixes0
  libxdmcp6
  libxext6
  libxfixes3
  libxi6
  libxinerama1
  libxkbfile1
  libxmu6
  libxrandr2
  libxrender1
  libxt6
  mesa-va-drivers
  openssl
  pipewire
  pipewire-pulse
  vdpau-driver-all
  wireplumber
  x11-utils
  xauth
  xclip
  xdotool
  xterm
  xvfb
)

ARCH_RUNTIME_PACKAGES=(
  ca-certificates
  curl
  dbus
  ffmpeg
  fontconfig
  jwm
  libglvnd
  libpulse
  libva
  libvdpau
  libx11
  libxau
  libxcb
  libxdmcp
  libxext
  libxfixes
  libxi
  libxinerama
  libxkbfile
  libxmu
  libxrandr
  libxrender
  libxt
  mesa
  mesa-utils
  openssl
  pipewire
  pipewire-pulse
  vdpauinfo
  wireplumber
  xclip
  xdotool
  xorg-server-xvfb
  xorg-xauth
  xorg-xdpyinfo
  xorg-xset
  xterm
)

log() {
  printf '[buildappimage] %s\n' "$*"
}

die() {
  printf '[buildappimage] error: %s\n' "$*" >&2
  exit 1
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "missing required command: $1"
}

host_arch_to_appimage_arch() {
  case "${ARCH}" in
    x86_64|aarch64) printf '%s\n' "${ARCH}" ;;
    *) die "unsupported AppImage host architecture: ${ARCH}" ;;
  esac
}

install_build_prereqs_hint() {
  cat >&2 <<'EOF'
Install build prerequisites on Debian/Ubuntu with:
  sudo apt-get update
  sudo apt-get install -y build-essential curl ca-certificates file patchelf dpkg-dev apt-utils

Install build prerequisites on Arch Linux with:
  sudo pacman -Sy --needed base-devel curl ca-certificates file patchelf libarchive

The runtime payload is resolved with apt, so build this AppImage on the oldest
Debian/Ubuntu version you need to support, or from pacman on Arch Linux.
AppImages cannot safely bundle glibc, kernel modules, GPU drivers, systemd, or
host device permissions.
EOF
}

resolve_deb_dependencies() {
  local packages=("$@")
  apt-cache depends --recurse \
    --no-recommends \
    --no-suggests \
    --no-conflicts \
    --no-breaks \
    --no-replaces \
    --no-enhances \
    "${packages[@]}" |
    awk '
      /^[A-Za-z0-9][A-Za-z0-9+.-]*$/ { print $1; next }
      /^[[:space:]]*Depends:/ {
        pkg = $2
        gsub(/[<>()]/, "", pkg)
        if (pkg != "") print pkg
      }
    ' |
    sort -u |
    while IFS= read -r package_name; do
      if apt-cache show "${package_name}" >/dev/null 2>&1; then
        printf '%s\n' "${package_name}"
      fi
    done
}

download_and_extract_runtime_debs() {
  need_cmd apt-cache
  need_cmd dpkg-deb

  mkdir -p "${DEB_CACHE}" "${APPDIR}"
  log "resolving runtime package dependencies"
  mapfile -t packages < <(resolve_deb_dependencies "${RUNTIME_PACKAGES[@]}")
  ((${#packages[@]} > 0)) || die "apt dependency resolution returned no packages"

  log "downloading ${#packages[@]} runtime packages"
  (
    cd "${DEB_CACHE}"
    apt-get download "${packages[@]}"
  )

  log "extracting runtime packages into AppDir"
  find "${DEB_CACHE}" -maxdepth 1 -name '*.deb' -print0 |
    while IFS= read -r -d '' deb; do
      dpkg-deb -x "${deb}" "${APPDIR}"
    done
}

download_and_extract_runtime_pacman() {
  need_cmd pacman
  need_cmd bsdtar

  local pacman_cache="${BUILD_DIR}/pacman-cache"
  mkdir -p "${pacman_cache}" "${APPDIR}"

  log "downloading Arch runtime packages"
  sudo pacman -Sw \
    --noconfirm \
    --cachedir "${pacman_cache}" \
    "${ARCH_RUNTIME_PACKAGES[@]}"

  log "extracting Arch runtime packages into AppDir"
  find "${pacman_cache}" -maxdepth 1 \( -name '*.pkg.tar.zst' -o -name '*.pkg.tar.xz' -o -name '*.pkg.tar' \) -print0 |
    while IFS= read -r -d '' package_file; do
      bsdtar -xf "${package_file}" -C "${APPDIR}"
    done
}

download_and_extract_runtime_packages() {
  if command -v apt-get >/dev/null 2>&1; then
    download_and_extract_runtime_debs
  elif command -v pacman >/dev/null 2>&1; then
    download_and_extract_runtime_pacman
  else
    die "runtime package bundling requires apt-get/dpkg-deb or pacman/bsdtar"
  fi
}

copy_binary_closure() {
  local bin="$1"
  local libdir="${APPDIR}/usr/lib"
  mkdir -p "${libdir}"

  if ! command -v ldd >/dev/null 2>&1; then
    return 0
  fi

  ldd "${bin}" 2>/dev/null |
    awk '
      /=> \// { print $3 }
      /^[[:space:]]*\// { print $1 }
    ' |
    while IFS= read -r lib; do
      case "${lib}" in
        /lib*/ld-linux*.so.*|/lib*/libc.so.*|/lib*/libm.so.*|/lib*/libpthread.so.*|/lib*/librt.so.*|/lib*/libdl.so.*|/lib*/libresolv.so.*|/lib*/libnsl.so.*|/lib*/libnss_*.so.*)
          ;;
        *)
          cp -L -n "${lib}" "${libdir}/" 2>/dev/null || true
          ;;
      esac
    done
}

copy_important_host_binaries() {
  local names=(
    ffmpeg
    xdotool
    xclip
    Xvfb
    jwm
    xterm
    xdpyinfo
    xset
    dbus-launch
    dbus-send
    pactl
    pipewire
    pipewire-pulse
    pw-cli
    pw-loopback
    wireplumber
    openssl
  )
  local name path

  mkdir -p "${APPDIR}/usr/bin"
  for name in "${names[@]}"; do
    if path="$(command -v "${name}" 2>/dev/null)"; then
      cp -L -n "${path}" "${APPDIR}/usr/bin/${name}" 2>/dev/null || true
      copy_binary_closure "${path}"
    fi
  done
}

patch_elf_rpaths() {
  command -v patchelf >/dev/null 2>&1 || {
    log "patchelf not found; skipping RPATH patching"
    return 0
  }

  log "patching ELF RPATHs"
  find "${APPDIR}/usr/bin" "${APPDIR}/usr/lib" "${APPDIR}/usr/libexec" \
    -type f -print0 2>/dev/null |
    while IFS= read -r -d '' file_path; do
      if file "${file_path}" 2>/dev/null | grep -Eq 'ELF .* (executable|shared object|pie executable)'; then
        patchelf --set-rpath '$ORIGIN/../lib:$ORIGIN/../lib/x86_64-linux-gnu:$ORIGIN/../lib/aarch64-linux-gnu:$ORIGIN:$ORIGIN/../../usr/lib:$ORIGIN/../../usr/lib/x86_64-linux-gnu:$ORIGIN/../../usr/lib/aarch64-linux-gnu' "${file_path}" 2>/dev/null || true
      fi
    done
}

remove_nonportable_core_libs() {
  log "removing host-specific core libraries from AppDir"
  find "${APPDIR}" -type f \( \
    -name 'ld-linux*.so*' -o \
    -name 'libanl.so*' -o \
    -name 'libBrokenLocale.so*' -o \
    -name 'libc.so*' -o \
    -name 'libcidn.so*' -o \
    -name 'libdl.so*' -o \
    -name 'libm.so*' -o \
    -name 'libmvec.so*' -o \
    -name 'libnsl.so*' -o \
    -name 'libnss_*.so*' -o \
    -name 'libpthread.so*' -o \
    -name 'libresolv.so*' -o \
    -name 'librt.so*' -o \
    -name 'libthread_db.so*' -o \
    -name 'libutil.so*' \
  \) -delete 2>/dev/null || true
}

write_desktop_metadata() {
  mkdir -p "${APPDIR}/usr/share/applications" "${APPDIR}/usr/share/icons/hicolor/256x256/apps"

  cat >"${APPDIR}/${APP_ID}.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Vibe RDesk
Comment=Portable Rust remote desktop server
Exec=${APP_NAME}
Icon=${APP_ID}
Terminal=true
Categories=Network;RemoteAccess;
EOF
  cp "${APPDIR}/${APP_ID}.desktop" "${APPDIR}/usr/share/applications/${APP_ID}.desktop"

  cat >"${APPDIR}/${APP_ID}.svg" <<'EOF'
<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256" viewBox="0 0 256 256">
  <rect width="256" height="256" rx="36" fill="#101820"/>
  <rect x="42" y="58" width="172" height="112" rx="12" fill="#1f6feb"/>
  <rect x="58" y="74" width="140" height="80" rx="6" fill="#0b1020"/>
  <path d="M78 101h54v14H78zm0 28h100v14H78z" fill="#7ee787"/>
  <path d="M112 190h32v20h42v18H70v-18h42z" fill="#d0d7de"/>
</svg>
EOF
  cp "${APPDIR}/${APP_ID}.svg" "${APPDIR}/usr/share/icons/hicolor/256x256/apps/${APP_ID}.svg"
}

write_apprun() {
  cat >"${APPDIR}/AppRun" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

SELF_PATH="${BASH_SOURCE[0]}"
while [[ -L "${SELF_PATH}" ]]; do
  SELF_DIR="$(cd -P "$(dirname "${SELF_PATH}")" && pwd)"
  SELF_PATH="$(readlink "${SELF_PATH}")"
  [[ "${SELF_PATH}" == /* ]] || SELF_PATH="${SELF_DIR}/${SELF_PATH}"
done
APPDIR="$(cd -P "$(dirname "${SELF_PATH}")" && pwd)"

export PATH="${APPDIR}/usr/bin:${APPDIR}/usr/sbin:${PATH}"
export LD_LIBRARY_PATH="${APPDIR}/usr/lib:${APPDIR}/usr/lib/x86_64-linux-gnu:${APPDIR}/usr/lib/aarch64-linux-gnu:${LD_LIBRARY_PATH:-}"
export XDG_DATA_DIRS="${APPDIR}/usr/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
export FONTCONFIG_PATH="${APPDIR}/etc/fonts:${FONTCONFIG_PATH:-/etc/fonts}"
export SSL_CERT_FILE="${SSL_CERT_FILE:-${APPDIR}/etc/ssl/certs/ca-certificates.crt}"
export VIBE_RDESK_SSL_DIR="${VIBE_RDESK_SSL_DIR:-${HOME}/.local/share/vibe_rdesk/ssl_keys}"
export VIBE_RDESK_LOG_DIR="${VIBE_RDESK_LOG_DIR:-${XDG_RUNTIME_DIR:-/tmp}/vibe_rdesk-$(id -u)}"
export VIBE_RDESK_BIND="${VIBE_RDESK_BIND:-0.0.0.0:8001,[::]:8001}"

HEADLESS_DISPLAY_RAW="${VIBE_RDESK_HEADLESS_DISPLAY:-11}"
XVFB_SCREEN="${VIBE_RDESK_XVFB_SCREEN:-1280x720x24}"
WINDOW_MANAGER_DELAY_SECONDS="${VIBE_RDESK_WINDOW_MANAGER_DELAY_SECONDS:-2}"
XTERM_FONT_FAMILY="${VIBE_RDESK_XTERM_FONT_FAMILY:-Monospace}"
XTERM_FONT_SIZE="${VIBE_RDESK_XTERM_FONT_SIZE:-10}"
VIRTUAL_MIC_SOURCE_NAME="${VIBE_RDESK_VIRTUAL_MIC_SOURCE_NAME:-Viberdeskmic}"
VIRTUAL_MIC_SINK_NAME="${VIBE_RDESK_VIRTUAL_MIC_SINK_NAME:-vibe_rdesk_virtual_mic_sink}"
SSL_DIR="${VIBE_RDESK_SSL_DIR}"
SSL_CERT="${VIBE_RDESK_TLS_CERT:-${SSL_DIR}/server.crt}"
SSL_KEY="${VIBE_RDESK_TLS_KEY:-${SSL_DIR}/server.key}"
DEV_LOG_DIR="${VIBE_RDESK_LOG_DIR}"
AUDIO_BACKEND=""
APP_PID=""
XVFB_PID=""
JWM_PID=""
XTERM_PID=""
PULSE_PID=""
PIPEWIRE_LOOPBACK_PID=""

require_passwd_arg() {
  local args=("$@")
  local i
  for ((i = 0; i < ${#args[@]}; i++)); do
    if [[ "${args[$i]}" == "--passwd" ]]; then
      if (( i + 1 >= ${#args[@]} )) || [[ -z "${args[$((i + 1))]}" ]]; then
        echo "[vibe_rdesk] --passwd requires a non-empty value" >&2
        exit 1
      fi
      return 0
    fi
  done
  echo "[vibe_rdesk] missing required --passwd. Start with: ${0} --passwd <password>" >&2
  exit 1
}

normalize_display() {
  local display="${1:-}"
  [[ -n "${display}" ]] || return 1
  if [[ "${display}" == :* ]]; then
    printf '%s\n' "${display}"
  else
    printf ':%s\n' "${display}"
  fi
}

is_display_available() {
  local display="${1:-}"
  [[ -n "${display}" ]] || return 1
  if command -v xdpyinfo >/dev/null 2>&1; then
    xdpyinfo -display "${display}" >/dev/null 2>&1
  else
    DISPLAY="${display}" xdotool getmouselocation >/dev/null 2>&1
  fi
}

wait_for_display() {
  local display="$1"
  local attempts=20
  while (( attempts > 0 )); do
    if is_display_available "${display}"; then
      return 0
    fi
    sleep 0.5
    ((attempts--))
  done
  return 1
}

wait_for_process() {
  local pid="$1"
  local attempts="${2:-8}"
  while (( attempts > 0 )); do
    if kill -0 "${pid}" 2>/dev/null; then
      return 0
    fi
    sleep 0.5
    ((attempts--))
  done
  return 1
}

ensure_tls_keys() {
  if [[ -f "${SSL_CERT}" && -f "${SSL_KEY}" ]]; then
    return 0
  fi
  if [[ -f "${SSL_CERT}" || -f "${SSL_KEY}" ]]; then
    echo "[vibe_rdesk] TLS certificate/key pair is incomplete in ${SSL_DIR}" >&2
    exit 1
  fi
  command -v openssl >/dev/null 2>&1 || {
    echo "[vibe_rdesk] openssl is required to generate TLS keys" >&2
    exit 1
  }
  mkdir -p "${SSL_DIR}"
  chmod 700 "${SSL_DIR}" 2>/dev/null || true
  local openssl_config="${SSL_DIR}/server.openssl.cnf"
  cat >"${openssl_config}" <<'CNF'
[req]
default_bits = 2048
distinguished_name = dn
x509_extensions = v3_req
prompt = no
[dn]
CN = vibe-rdesk-local
[v3_req]
subjectAltName = @alt_names
[alt_names]
DNS.1 = localhost
DNS.2 = vibe-rdesk-local
IP.1 = 127.0.0.1
IP.2 = ::1
CNF
  openssl req -x509 -newkey rsa:2048 -sha256 -days "${VIBE_RDESK_TEST_CERT_DAYS:-3650}" -nodes \
    -keyout "${SSL_KEY}" -out "${SSL_CERT}" -config "${openssl_config}" >/dev/null 2>&1
  chmod 600 "${SSL_KEY}" 2>/dev/null || true
}

ensure_log_dir() {
  mkdir -p "${DEV_LOG_DIR}"
}

source_exists() {
  local source_name="${1:-}"
  [[ -n "${source_name}" ]] && pactl list short sources 2>/dev/null | awk '{print $2}' | grep -Fxq "${source_name}"
}

sink_exists() {
  local sink_name="${1:-}"
  [[ -n "${sink_name}" ]] && pactl list short sinks 2>/dev/null | awk '{print $2}' | grep -Fxq "${sink_name}"
}

wait_for_pulse_server() {
  local attempts=20
  while (( attempts > 0 )); do
    if pactl info >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.25
    ((attempts--))
  done
  return 1
}

wait_for_pipewire_server() {
  local attempts=20
  while (( attempts > 0 )); do
    if pw-cli info 0 >/dev/null 2>&1; then
      return 0
    fi
    sleep 0.25
    ((attempts--))
  done
  return 1
}

start_audio_server() {
  if command -v pw-cli >/dev/null 2>&1 && command -v pw-loopback >/dev/null 2>&1; then
    if wait_for_pipewire_server; then
      AUDIO_BACKEND="pipewire"
      echo "[vibe_rdesk] using existing PipeWire server"
      return 0
    fi
    if command -v systemctl >/dev/null 2>&1; then
      systemctl --user start pipewire pipewire-pulse wireplumber >/dev/null 2>&1 || true
    fi
    if wait_for_pipewire_server; then
      AUDIO_BACKEND="pipewire"
      echo "[vibe_rdesk] audio backend: PipeWire"
      return 0
    fi
  fi

  if pactl info >/dev/null 2>&1; then
    AUDIO_BACKEND="pipewire-pulse"
    echo "[vibe_rdesk] using existing Pulse-compatible audio server"
    return 0
  fi

  command -v pipewire >/dev/null 2>&1 && pipewire >/dev/null 2>&1 &
  command -v pipewire-pulse >/dev/null 2>&1 && pipewire-pulse >/dev/null 2>&1 &
  command -v wireplumber >/dev/null 2>&1 && wireplumber >/dev/null 2>&1 &

  if wait_for_pulse_server; then
    AUDIO_BACKEND="pipewire-pulse"
    echo "[vibe_rdesk] audio backend: PipeWire Pulse compatibility"
    return 0
  fi
  echo "[vibe_rdesk] audio server did not start; video/control may still work, audio capture may not" >&2
  return 0
}

ensure_virtual_mic() {
  command -v pactl >/dev/null 2>&1 || return 0
  pactl info >/dev/null 2>&1 || return 0
  if source_exists "${VIRTUAL_MIC_SOURCE_NAME}"; then
    return 0
  fi
  if ! sink_exists "${VIRTUAL_MIC_SINK_NAME}"; then
    pactl load-module module-null-sink "sink_name=${VIRTUAL_MIC_SINK_NAME}" \
      "sink_properties=device.description=VibeRDeskVirtualMicSink" >/dev/null 2>&1 || return 0
  fi
  pactl load-module module-remap-source "source_name=${VIRTUAL_MIC_SOURCE_NAME}" \
    "master=${VIRTUAL_MIC_SINK_NAME}.monitor" \
    "source_properties=device.description=${VIRTUAL_MIC_SOURCE_NAME}" >/dev/null 2>&1 || true
}

start_headless_display() {
  export DISPLAY
  DISPLAY="$(normalize_display "${HEADLESS_DISPLAY_RAW}")"
  if is_display_available "${DISPLAY}"; then
    echo "[vibe_rdesk] using existing X server on ${DISPLAY}"
    return 0
  fi
  command -v Xvfb >/dev/null 2>&1 || {
    echo "[vibe_rdesk] Xvfb is unavailable" >&2
    exit 1
  }
  local xvfb_log="${DEV_LOG_DIR}/xvfb.log"
  local jwm_log="${DEV_LOG_DIR}/jwm.log"
  local xterm_log="${DEV_LOG_DIR}/xterm.log"
  echo "[vibe_rdesk] starting Xvfb on ${DISPLAY}"
  Xvfb "${DISPLAY}" -screen 0 "${XVFB_SCREEN}" -ac -nolisten tcp >"${xvfb_log}" 2>&1 &
  XVFB_PID=$!
  wait_for_display "${DISPLAY}" || {
    echo "[vibe_rdesk] Xvfb on ${DISPLAY} did not become ready; see ${xvfb_log}" >&2
    exit 1
  }
  sleep "${WINDOW_MANAGER_DELAY_SECONDS}"
  if command -v jwm >/dev/null 2>&1; then
    DISPLAY="${DISPLAY}" jwm >"${jwm_log}" 2>&1 &
    JWM_PID=$!
    wait_for_process "${JWM_PID}" 4 || true
  fi
  if command -v xterm >/dev/null 2>&1; then
    DISPLAY="${DISPLAY}" xterm -display "${DISPLAY}" -title "vibe_rdesk" \
      -fa "${XTERM_FONT_FAMILY}" -fs "${XTERM_FONT_SIZE}" -geometry 120x30+40+40 \
      >"${xterm_log}" 2>&1 &
    XTERM_PID=$!
    wait_for_process "${XTERM_PID}" 4 || true
  fi
}

ensure_display() {
  local fallback_display
  fallback_display="$(normalize_display "${HEADLESS_DISPLAY_RAW}")"
  if [[ -n "${DISPLAY:-}" ]] && is_display_available "${DISPLAY}"; then
    echo "[vibe_rdesk] using existing X server on ${DISPLAY}"
    return 0
  fi
  [[ -n "${DISPLAY:-}" ]] && echo "[vibe_rdesk] DISPLAY=${DISPLAY} is unavailable; falling back to ${fallback_display}"
  start_headless_display
}

cleanup() {
  for pid in "${APP_PID}" "${XTERM_PID}" "${JWM_PID}" "${XVFB_PID}" "${PULSE_PID}" "${PIPEWIRE_LOOPBACK_PID}"; do
    if [[ -n "${pid}" ]] && kill -0 "${pid}" 2>/dev/null; then
      kill "${pid}" 2>/dev/null || true
      wait "${pid}" 2>/dev/null || true
    fi
  done
}

trap cleanup EXIT INT TERM

if [[ "${1:-}" == "--appimage-version" ]]; then
  echo "vibe_rdesk AppImage"
  exit 0
fi

require_passwd_arg "$@"
ensure_tls_keys
export VIBE_RDESK_TLS_CERT="${SSL_CERT}"
export VIBE_RDESK_TLS_KEY="${SSL_KEY}"
ensure_log_dir

if [[ "${VIBE_RDESK_SKIP_RUNTIME_SETUP:-0}" != "1" ]]; then
  ensure_display
  start_audio_server
  ensure_virtual_mic
fi

echo "[vibe_rdesk] starting HTTPS app on ${VIBE_RDESK_BIND}"
"${APPDIR}/usr/bin/vibe_rdesk" "$@" &
APP_PID=$!
wait "${APP_PID}"
EOF
  chmod +x "${APPDIR}/AppRun"
  ln -sf AppRun "${APPDIR}/${APP_NAME}"
}

download_appimagetool() {
  if command -v appimagetool >/dev/null 2>&1; then
    APPIMAGETOOL="$(command -v appimagetool)"
    return 0
  fi
  if [[ -x "${APPIMAGETOOL}" ]]; then
    return 0
  fi

  local appimage_arch
  appimage_arch="$(host_arch_to_appimage_arch)"
  local url="https://github.com/AppImage/AppImageKit/releases/download/continuous/appimagetool-${appimage_arch}.AppImage"
  mkdir -p "$(dirname "${APPIMAGETOOL}")"
  log "downloading appimagetool from ${url}"
  curl -fsSL "${url}" -o "${APPIMAGETOOL}"
  chmod +x "${APPIMAGETOOL}"
}

build_release_binary() {
  need_cmd cargo

  log "building optimized release binary"
  export CARGO_PROFILE_RELEASE_CODEGEN_UNITS="${CARGO_PROFILE_RELEASE_CODEGEN_UNITS:-1}"
  export CARGO_PROFILE_RELEASE_INCREMENTAL="${CARGO_PROFILE_RELEASE_INCREMENTAL:-false}"
  export CARGO_PROFILE_RELEASE_LTO="${CARGO_PROFILE_RELEASE_LTO:-thin}"
  export CARGO_PROFILE_RELEASE_PANIC="${CARGO_PROFILE_RELEASE_PANIC:-abort}"

  if [[ "${NATIVE_CPU}" == "1" ]]; then
    export RUSTFLAGS="${RUSTFLAGS:-} -C target-cpu=native"
    log "using target-cpu=native; this may not run on older CPUs"
  elif [[ "${PORTABLE_CPU}" == "1" ]]; then
    export RUSTFLAGS="${RUSTFLAGS:-} -C target-cpu=x86-64"
  fi

  cargo build --release
}

assemble_appdir() {
  rm -rf "${APPDIR}"
  mkdir -p "${APPDIR}/usr/bin" "${OUT_DIR}"

  download_and_extract_runtime_packages

  cp "${PROJECT_ROOT}/target/release/${APP_NAME}" "${APPDIR}/usr/bin/${APP_NAME}"
  chmod +x "${APPDIR}/usr/bin/${APP_NAME}"
  copy_binary_closure "${APPDIR}/usr/bin/${APP_NAME}"
  copy_important_host_binaries

  write_desktop_metadata
  write_apprun
  remove_nonportable_core_libs
  patch_elf_rpaths
}

make_appimage() {
  download_appimagetool

  local out_file="${OUT_DIR}/${APP_NAME}-${VERSION}-${ARCH}.AppImage"
  rm -f "${out_file}"
  log "creating ${out_file}"
  ARCH="${ARCH}" VERSION="${VERSION}" "${APPIMAGETOOL}" "${APPDIR}" "${out_file}"
  chmod +x "${out_file}"
  log "created ${out_file}"
}

main() {
  if ! command -v patchelf >/dev/null 2>&1 || { ! command -v apt-get >/dev/null 2>&1 && ! command -v pacman >/dev/null 2>&1; }; then
    install_build_prereqs_hint
  fi

  build_release_binary
  assemble_appdir
  make_appimage

  cat <<EOF

AppImage built successfully.

Output:
  ${OUT_DIR}/${APP_NAME}-${VERSION}-${ARCH}.AppImage

Run:
  ${OUT_DIR}/${APP_NAME}-${VERSION}-${ARCH}.AppImage --passwd <password>

Notes:
  - Build on an old enough Debian/Ubuntu base for maximum glibc compatibility.
    Arch Linux builds work, but they target systems with glibc at least as new
    as the Arch build host.
  - Host kernel features still cannot be bundled: /dev/uinput permissions,
    v4l2loopback, GPU drivers, and systemd user sessions depend on the target host.
  - Use NATIVE_CPU=1 ./buildappimage.sh only for machines with compatible CPUs.
EOF
}

main "$@"
