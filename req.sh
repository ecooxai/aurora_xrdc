#!/usr/bin/env bash
set -euo pipefail

install_apt() {
  export DEBIAN_FRONTEND=noninteractive
  sudo apt-get update
  sudo apt-get install -y \
    ffmpeg \
    xdotool \
    xclip \
    xvfb \
    jwm \
    xterm \
    x11-utils \
    curl \
    ca-certificates \
    dbus-x11 \
    xauth \
    pulseaudio \
    wireplumber \
    pulseaudio-utils
}

install_pacman() {
  sudo pacman -Sy --noconfirm \
    ffmpeg \
    xdotool \
    xclip \
    xorg-server-xvfb \
    jwm \
    xterm \
    xorg-xdpyinfo \
    xorg-xprop \
    curl \
    ca-certificates \
    dbus \
    xorg-xauth \
     dbus-launch \
    pulseaudio \
    pipewire-pulse \
    wireplumber \
    libpulse
}

if command -v apt-get >/dev/null 2>&1; then
  install_apt
elif command -v pacman >/dev/null 2>&1; then
  install_pacman
else
  echo "Unsupported package manager. Supported: apt-get, pacman." >&2
  exit 1
fi

systemctl --user daemon-reload || true
systemctl --user enable --now pipewire pipewire-pulse wireplumber || true

cat <<'EOF'
Dependencies installed.

Audio notes:
- `pactl` is provided by `pulseaudio-utils`.
- `pipewire-pulse` provides a Pulse-compatible server for FFmpeg and `pactl`.
- If no real audio sink exists, vibe_rdesk will create a null sink automatically.

Display notes:
- `Xvfb`, `jwm`, and `xterm` are installed so `./dev.sh` can create a visible headless desktop on `:11`.
- `xdpyinfo` is installed so `./dev.sh` can validate an existing `DISPLAY` before using it.

If this is a headless VM and the user service is not running yet, log in with the target desktop user and run:
  systemctl --user start pipewire pipewire-pulse wireplumber

To verify audio tooling:
  pactl info
  pactl get-default-sink
EOF
