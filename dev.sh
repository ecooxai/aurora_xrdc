#!/usr/bin/env bash
set -euo pipefail

# Prefer a rustup-managed toolchain when available in the current workspace.
if [[ -f "${HOME}/.cargo/env" ]]; then
    # shellcheck disable=SC1090
    . "${HOME}/.cargo/env"
fi

WATCH_DIRS=(src web Cargo.toml)
DEBOUNCE_SECONDS=5
REBUILD_RETRY_SECONDS=10
HEADLESS_DISPLAY_RAW="${VIBE_RDESK_HEADLESS_DISPLAY:-11}"
XVFB_SCREEN="${VIBE_RDESK_XVFB_SCREEN:-1280x720x24}"
WINDOW_MANAGER_DELAY_SECONDS=2
XTERM_FONT_FAMILY="${VIBE_RDESK_XTERM_FONT_FAMILY:-Monospace}"
XTERM_FONT_SIZE="${VIBE_RDESK_XTERM_FONT_SIZE:-10}"
VIRTUAL_MIC_SOURCE_NAME="${VIBE_RDESK_VIRTUAL_MIC_SOURCE_NAME:-Viberdeskmic}"
VIRTUAL_MIC_SINK_NAME="${VIBE_RDESK_VIRTUAL_MIC_SINK_NAME:-vibe_rdesk_virtual_mic_sink}"
export VIBE_RDESK_BIND="${VIBE_RDESK_BIND:-0.0.0.0:8001,[::]:8001}"
AUDIO_BACKEND=""
APP_PID=""
XVFB_PID=""
JWM_PID=""
XTERM_PID=""
PULSE_PID=""
PIPEWIRE_LOOPBACK_PID=""

normalize_display() {
    local display="${1:-}"
    if [[ -z "${display}" ]]; then
        return 1
    fi

    if [[ "${display}" == :* ]]; then
        printf '%s\n' "${display}"
    else
        printf ':%s\n' "${display}"
    fi
}

is_display_available() {
    local display="${1:-}"
    if [[ -z "${display}" ]]; then
        return 1
    fi

    if command -v xdpyinfo >/dev/null 2>&1; then
        xdpyinfo -display "${display}" >/dev/null 2>&1
        return $?
    fi

    DISPLAY="${display}" xdotool getmouselocation >/dev/null 2>&1
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

source_exists() {
    local source_name="${1:-}"
    if [[ -z "${source_name}" ]]; then
        return 1
    fi

    pactl list short sources 2>/dev/null | awk '{print $2}' | grep -Fxq "${source_name}"
}

sink_exists() {
    local sink_name="${1:-}"
    if [[ -z "${sink_name}" ]]; then
        return 1
    fi

    pactl list short sinks 2>/dev/null | awk '{print $2}' | grep -Fxq "${sink_name}"
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

pipewire_node_id() {
    local node_name="${1:-}"
    local media_class="${2:-}"
    local node_id=""
    if [[ -z "${node_name}" || -z "${media_class}" ]]; then
        return 1
    fi

    node_id="$(pw-cli ls Node 2>/dev/null | awk -v node_name="${node_name}" -v media_class="${media_class}" '
        function flush_block() {
            if (current_id != "" && current_name == node_name && current_class == media_class) {
                print current_id
                found = 1
                exit
            }
        }
        /^[[:space:]]*id / {
            flush_block()
            current_id = $2
            sub(/,/, "", current_id)
            current_name = ""
            current_class = ""
            next
        }
        /node\.name = "/ {
            split($0, parts, "\"")
            current_name = parts[2]
            next
        }
        /media\.class = "/ {
            split($0, parts, "\"")
            current_class = parts[2]
            next
        }
        END {
            if (!found && current_id != "" && current_name == node_name && current_class == media_class) {
                print current_id
            }
        }
    ')"

    if [[ -z "${node_id}" ]]; then
        return 1
    fi

    printf '%s\n' "${node_id}"
}

pipewire_source_exists() {
    local source_name="${1:-}"
    [[ -n "${source_name}" ]] && pipewire_node_id "${source_name}" "Audio/Source" >/dev/null
}

pipewire_sink_exists() {
    local sink_name="${1:-}"
    [[ -n "${sink_name}" ]] && pipewire_node_id "${sink_name}" "Audio/Sink" >/dev/null
}

wait_for_pipewire_source() {
    local source_name="$1"
    local attempts=10

    while (( attempts > 0 )); do
        if pipewire_source_exists "${source_name}"; then
            return 0
        fi
        sleep 0.25
        ((attempts--))
    done

    return 1
}

wait_for_pipewire_sink() {
    local sink_name="$1"
    local attempts=10

    while (( attempts > 0 )); do
        if pipewire_sink_exists "${sink_name}"; then
            return 0
        fi
        sleep 0.25
        ((attempts--))
    done

    return 1
}

wait_for_source() {
    local source_name="$1"
    local attempts=10

    while (( attempts > 0 )); do
        if [[ "${AUDIO_BACKEND}" == "pipewire" ]]; then
            if pipewire_source_exists "${source_name}"; then
                return 0
            fi
        else
            if source_exists "${source_name}"; then
                return 0
            fi
        fi
        sleep 0.25
        ((attempts--))
    done

    return 1
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

start_pulse_server() {
    if pactl info >/dev/null 2>&1; then
        echo "[dev] using existing PulseAudio server"
        return 0
    fi

    if command -v dbus-launch >/dev/null 2>&1; then
        echo "[dev] starting PulseAudio with dbus-launch"
        dbus-launch pulseaudio >/tmp/vibe_rdesk-pulseaudio.log 2>&1 &
        PULSE_PID=$!
    else
        echo "[dev] starting PulseAudio"
        pulseaudio --start >/dev/null 2>&1 || true
    fi

    if ! wait_for_pulse_server; then
        echo "[dev] PulseAudio did not become ready" >&2
        return 1
    fi
}

start_pipewire_server() {
    if wait_for_pipewire_server; then
        echo "[dev] using existing PipeWire server"
        return 0
    fi

    if command -v systemctl >/dev/null 2>&1; then
        echo "[dev] starting PipeWire user services"
        systemctl --user start pipewire pipewire-pulse wireplumber >/dev/null 2>&1 || true
    fi

    if ! wait_for_pipewire_server; then
        return 1
    fi
}

pipewire_uses_pulse_server() {
    pactl info 2>/dev/null | grep -Fq 'Server Name: PulseAudio (on PipeWire'
}

start_audio_server() {
    if command -v pw-cli >/dev/null 2>&1 && command -v pw-loopback >/dev/null 2>&1; then
        if start_pipewire_server || wait_for_pipewire_server; then
            AUDIO_BACKEND="pipewire"
            echo "[dev] audio backend: PipeWire"
            return 0
        fi
    fi

    if start_pulse_server; then
        AUDIO_BACKEND="pulseaudio"
        if pipewire_uses_pulse_server; then
            echo "[dev] audio backend: PipeWire (PulseAudio compatibility)"
        else
            echo "[dev] audio backend: PulseAudio"
        fi
        return 0
    fi

    echo "[dev] failed to start an audio server" >&2
    return 1
}

ensure_pipewire_virtual_mic() {
    if pipewire_source_exists "${VIRTUAL_MIC_SOURCE_NAME}"; then
        echo "[dev] using existing PipeWire virtual mic source ${VIRTUAL_MIC_SOURCE_NAME}"
        return 0
    fi

    if ! pipewire_sink_exists "${VIRTUAL_MIC_SINK_NAME}"; then
        echo "[dev] creating PipeWire virtual sink ${VIRTUAL_MIC_SINK_NAME}"
        pw-cli create-node adapter \
            "{ factory.name = support.null-audio-sink node.name = \"${VIRTUAL_MIC_SINK_NAME}\" node.description = \"VibeRDeskVirtualMicSink\" media.class = \"Audio/Sink\" object.linger = true audio.position = [ FL FR ] }" \
            >/dev/null
        if ! wait_for_pipewire_sink "${VIRTUAL_MIC_SINK_NAME}"; then
            echo "[dev] PipeWire virtual sink ${VIRTUAL_MIC_SINK_NAME} did not appear" >&2
            return 1
        fi
    fi

    echo "[dev] creating PipeWire virtual mic source ${VIRTUAL_MIC_SOURCE_NAME}"
    pw-loopback \
        --name "${VIRTUAL_MIC_SOURCE_NAME}_loopback" \
        --capture-props "{ stream.capture.sink = true target.object = \"${VIRTUAL_MIC_SINK_NAME}\" node.passive = true node.dont-reconnect = true }" \
        --playback-props "{ node.name = \"${VIRTUAL_MIC_SOURCE_NAME}\" node.description = \"${VIRTUAL_MIC_SOURCE_NAME}\" media.class = \"Audio/Source\" audio.position = [ FL FR ] }" \
        >/tmp/vibe_rdesk-pipewire-loopback.log 2>&1 &
    PIPEWIRE_LOOPBACK_PID=$!

    if ! wait_for_pipewire_source "${VIRTUAL_MIC_SOURCE_NAME}"; then
        echo "[dev] PipeWire virtual mic source ${VIRTUAL_MIC_SOURCE_NAME} did not appear" >&2
        if [[ -n "${PIPEWIRE_LOOPBACK_PID}" ]] && kill -0 "${PIPEWIRE_LOOPBACK_PID}" 2>/dev/null; then
            kill "${PIPEWIRE_LOOPBACK_PID}" 2>/dev/null || true
            wait "${PIPEWIRE_LOOPBACK_PID}" 2>/dev/null || true
        fi
        PIPEWIRE_LOOPBACK_PID=""
        return 1
    fi
}

ensure_pulse_virtual_mic() {
    if ! command -v pactl >/dev/null 2>&1; then
        echo "[dev] pactl is required to provision the virtual microphone" >&2
        return 1
    fi

    if source_exists "${VIRTUAL_MIC_SOURCE_NAME}"; then
        echo "[dev] using existing virtual mic source ${VIRTUAL_MIC_SOURCE_NAME}"
        return 0
    fi

    if ! sink_exists "${VIRTUAL_MIC_SINK_NAME}"; then
        echo "[dev] creating virtual mic sink ${VIRTUAL_MIC_SINK_NAME}"
        pactl load-module \
            module-null-sink \
            "sink_name=${VIRTUAL_MIC_SINK_NAME}" \
            "sink_properties=device.description=VibeRDeskVirtualMicSink" \
            >/dev/null
    fi

    echo "[dev] creating virtual mic source ${VIRTUAL_MIC_SOURCE_NAME}"
    pactl load-module \
        module-remap-source \
        "source_name=${VIRTUAL_MIC_SOURCE_NAME}" \
        "master=${VIRTUAL_MIC_SINK_NAME}.monitor" \
        "source_properties=device.description=${VIRTUAL_MIC_SOURCE_NAME}" \
        >/dev/null

    if ! wait_for_source "${VIRTUAL_MIC_SOURCE_NAME}"; then
        echo "[dev] virtual mic source ${VIRTUAL_MIC_SOURCE_NAME} did not appear" >&2
        return 1
    fi
}

ensure_virtual_mic() {
    if [[ "${AUDIO_BACKEND}" == "pipewire" ]]; then
        ensure_pipewire_virtual_mic
        return $?
    fi

    ensure_pulse_virtual_mic
}

start_headless_display() {
    export DISPLAY
    DISPLAY="$(normalize_display "${HEADLESS_DISPLAY_RAW}")"

    if is_display_available "${DISPLAY}"; then
        echo "[dev] using existing X server on ${DISPLAY}"
        return 0
    fi

    if ! command -v Xvfb >/dev/null 2>&1; then
        echo "[dev] Xvfb is required for headless startup. Install it first." >&2
        exit 1
    fi

    if ! command -v jwm >/dev/null 2>&1; then
        echo "[dev] jwm is required for headless startup. Install it first." >&2
        exit 1
    fi

    if ! command -v xterm >/dev/null 2>&1; then
        echo "[dev] xterm is required for headless startup. Install it first." >&2
        exit 1
    fi

    echo "[dev] starting Xvfb on ${DISPLAY}"
    Xvfb "${DISPLAY}" -screen 0 "${XVFB_SCREEN}" -ac -nolisten tcp >/tmp/vibe_rdesk-xvfb.log 2>&1 &
    XVFB_PID=$!

    if ! wait_for_display "${DISPLAY}"; then
        echo "[dev] Xvfb on ${DISPLAY} did not become ready; see /tmp/vibe_rdesk-xvfb.log" >&2
        exit 1
    fi

    sleep "${WINDOW_MANAGER_DELAY_SECONDS}"

    echo "[dev] starting jwm on ${DISPLAY}"
    DISPLAY="${DISPLAY}" jwm >/tmp/vibe_rdesk-jwm.log 2>&1 &
    JWM_PID=$!

    if ! wait_for_process "${JWM_PID}" 4; then
        echo "[dev] jwm exited during startup; see /tmp/vibe_rdesk-jwm.log" >&2
        exit 1
    fi

    echo "[dev] starting xterm on ${DISPLAY}"
    DISPLAY="${DISPLAY}" xterm \
        -display "${DISPLAY}" \
        -title "vibe_rdesk" \
        -fa "${XTERM_FONT_FAMILY}" \
        -fs "${XTERM_FONT_SIZE}" \
        -geometry 120x30+40+40 \
        >/tmp/vibe_rdesk-xterm.log 2>&1 &
    XTERM_PID=$!

    if ! wait_for_process "${XTERM_PID}" 4; then
        echo "[dev] xterm exited during startup; see /tmp/vibe_rdesk-xterm.log" >&2
        exit 1
    fi
}

ensure_display() {
    local fallback_display
    fallback_display="$(normalize_display "${HEADLESS_DISPLAY_RAW}")"

    if [[ -n "${DISPLAY:-}" ]]; then
        if is_display_available "${DISPLAY}"; then
            echo "[dev] using existing X server on ${DISPLAY}"
            return 0
        fi

        echo "[dev] DISPLAY=${DISPLAY} is set but unavailable; falling back to ${fallback_display}"
    else
        echo "[dev] DISPLAY is not set; starting headless X11 on ${fallback_display}"
    fi

    start_headless_display
}

cleanup() {
    if [[ -n "${APP_PID}" ]] && kill -0 "${APP_PID}" 2>/dev/null; then
        kill "${APP_PID}" 2>/dev/null || true
        wait "${APP_PID}" 2>/dev/null || true
    fi

    if [[ -n "${XTERM_PID}" ]] && kill -0 "${XTERM_PID}" 2>/dev/null; then
        kill "${XTERM_PID}" 2>/dev/null || true
        wait "${XTERM_PID}" 2>/dev/null || true
    fi

    if [[ -n "${JWM_PID}" ]] && kill -0 "${JWM_PID}" 2>/dev/null; then
        kill "${JWM_PID}" 2>/dev/null || true
        wait "${JWM_PID}" 2>/dev/null || true
    fi

    if [[ -n "${XVFB_PID}" ]] && kill -0 "${XVFB_PID}" 2>/dev/null; then
        kill "${XVFB_PID}" 2>/dev/null || true
        wait "${XVFB_PID}" 2>/dev/null || true
    fi

    if [[ -n "${PULSE_PID}" ]] && kill -0 "${PULSE_PID}" 2>/dev/null; then
        kill "${PULSE_PID}" 2>/dev/null || true
        wait "${PULSE_PID}" 2>/dev/null || true
    fi

    if [[ -n "${PIPEWIRE_LOOPBACK_PID}" ]] && kill -0 "${PIPEWIRE_LOOPBACK_PID}" 2>/dev/null; then
        kill "${PIPEWIRE_LOOPBACK_PID}" 2>/dev/null || true
        wait "${PIPEWIRE_LOOPBACK_PID}" 2>/dev/null || true
    fi
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
start_audio_server
ensure_virtual_mic

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
