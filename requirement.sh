#!/usr/bin/env bash
set -euo pipefail

export DEBIAN_FRONTEND="${DEBIAN_FRONTEND:-noninteractive}"

install_apt() {
  sudo apt-get update
  sudo apt-get install -y \
    acl \
    ca-certificates \
    curl \
    dbus-x11 \
    ffmpeg \
    icewm \
    inotify-tools \
    iproute2 \
    kmod \
    pulseaudio \
    pulseaudio-utils \
    x11-utils \
    xauth \
    xclip \
    xdotool \
    xterm \
    xvfb
}

install_pacman() {
  sudo pacman -Sy --noconfirm \
    acl \
    ca-certificates \
    curl \
    dbus \
    ffmpeg \
    icewm \
    inotify-tools \
    iproute2 \
    kmod \
    libpulse \
    pulseaudio \
    xclip \
    xdotool \
    xorg-server-xvfb \
    xorg-xauth \
    xorg-xdpyinfo \
    xorg-xprop \
    xterm
}

if command -v apt-get >/dev/null 2>&1; then
  install_apt
elif command -v pacman >/dev/null 2>&1; then
  install_pacman
else
  echo "Unsupported package manager. Supported: apt-get, pacman." >&2
  exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable
fi

echo "Dependencies installed for dev.sh."
