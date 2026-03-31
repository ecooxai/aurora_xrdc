#!/usr/bin/env bash
set -euo pipefail

# Prefer a rustup-managed toolchain when available in the current workspace.
if [[ -f "${HOME}/.cargo/env" ]]; then
    # shellcheck disable=SC1090
    . "${HOME}/.cargo/env"
fi

# Start PulseAudio when available, but avoid noisy DBus-related failures in
# environments where it is already running or no system bus exists.
if command -v pulseaudio >/dev/null 2>&1 && ! pulseaudio --check 2>/dev/null; then
    pulseaudio --start >/dev/null 2>&1 || true
fi

WATCH_DIRS=(src web Cargo.toml)
DEBOUNCE_SECONDS=5
REBUILD_RETRY_SECONDS=10
APP_PID=""
XVFB_PID=""
XTERM_PID=""
FALLBACK_DISPLAY=":11"

cleanup() {
    if [[ -n "${APP_PID}" ]] && kill -0 "${APP_PID}" 2>/dev/null; then
        kill "${APP_PID}" 2>/dev/null || true
        wait "${APP_PID}" 2>/dev/null || true
    fi
    if [[ -n "${XTERM_PID}" ]] && kill -0 "${XTERM_PID}" 2>/dev/null; then
        kill "${XTERM_PID}" 2>/dev/null || true
        wait "${XTERM_PID}" 2>/dev/null || true
    fi
    if [[ -n "${XVFB_PID}" ]] && kill -0 "${XVFB_PID}" 2>/dev/null; then
        kill "${XVFB_PID}" 2>/dev/null || true
        wait "${XVFB_PID}" 2>/dev/null || true
    fi
}

display_ready() {
    local display="${1:-}"
    [[ -n "${display}" ]] || return 1
    command -v xdpyinfo >/dev/null 2>&1 || return 1
    DISPLAY="${display}" xdpyinfo >/dev/null 2>&1
}

start_fallback_xserver() {
    if ! command -v Xvfb >/dev/null 2>&1; then
        echo "[dev] DISPLAY is unavailable and Xvfb is not installed." >&2
        return 1
    fi
    if ! command -v xterm >/dev/null 2>&1; then
        echo "[dev] DISPLAY is unavailable and xterm is not installed." >&2
        return 1
    fi

    export DISPLAY="${FALLBACK_DISPLAY}"

    if display_ready "${DISPLAY}"; then
        echo "[dev] reusing existing X server on ${DISPLAY}"
    else
        echo "[dev] DISPLAY is unavailable, starting Xvfb on ${DISPLAY}"
        rm -f "/tmp/.X11-unix/X${DISPLAY#:}" "/tmp/.X${DISPLAY#*:}-lock"
        Xvfb "${DISPLAY}" -screen 0 1280x720x24 >/tmp/xvfb11.log 2>&1 &
        XVFB_PID=$!

        for _ in $(seq 1 20); do
            if display_ready "${DISPLAY}"; then
                break
            fi
            sleep 0.5
        done

        if ! display_ready "${DISPLAY}"; then
            echo "[dev] failed to start Xvfb on ${DISPLAY}" >&2
            return 1
        fi
    fi

    if ! pgrep -f "xterm -geometry 100x30+80+60 -title vibe_rdesk-dev-xterm" >/dev/null 2>&1; then
        DISPLAY="${DISPLAY}" xterm -geometry 100x30+80+60 -title vibe_rdesk-dev-xterm >/tmp/xterm11.log 2>&1 &
        XTERM_PID=$!
    fi

    echo "[dev] using DISPLAY=${DISPLAY}"
}

ensure_display() {
    if display_ready "${DISPLAY:-}"; then
        echo "[dev] using existing DISPLAY=${DISPLAY}"
        return 0
    fi

    start_fallback_xserver
}

build_and_run() {
    while true; do
        echo "[dev] building..."
        if ./test.sh; then
            break
        fi

        echo "[dev] build failed, retrying in ${REBUILD_RETRY_SECONDS}s..."
        sleep "${REBUILD_RETRY_SECONDS}"
    done

    if [[ -n "${APP_PID}" ]] && kill -0 "${APP_PID}" 2>/dev/null; then
        echo "[dev] stopping running app..."
        kill "${APP_PID}" 2>/dev/null || true
        wait "${APP_PID}" 2>/dev/null || true
    fi

    echo "[dev] starting app..."
    cargo run -- "$@" &
    APP_PID=$!
}

wait_for_change_polling() {
    local previous_state=""
    local current_state=""

    previous_state="$(find "${WATCH_DIRS[@]}" -type f -print0 2>/dev/null | sort -z | xargs -0 stat -c '%n %Y %s' 2>/dev/null || true)"

    while true; do
        sleep 1
        current_state="$(find "${WATCH_DIRS[@]}" -type f -print0 2>/dev/null | sort -z | xargs -0 stat -c '%n %Y %s' 2>/dev/null || true)"
        if [[ "${current_state}" != "${previous_state}" ]]; then
            return 0
        fi
    done
}

trap cleanup EXIT INT TERM

ensure_display
build_and_run "$@"

while true; do
    if command -v inotifywait >/dev/null 2>&1; then
        inotifywait -qq -r -e modify -e create -e delete -e move "${WATCH_DIRS[@]}"
    else
        wait_for_change_polling
    fi

    echo "[dev] change detected, waiting ${DEBOUNCE_SECONDS}s before rebuild..."
    sleep "${DEBOUNCE_SECONDS}"
    build_and_run "$@"
done
