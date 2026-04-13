#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

log() {
    printf '[reset-audio] %s\n' "$*"
}

have_cmd() {
    command -v "$1" >/dev/null 2>&1
}

run_user_systemctl() {
    if ! have_cmd systemctl; then
        return 1
    fi

    systemctl --user "$@" >/dev/null 2>&1
}

unit_exists() {
    local unit="$1"
    if ! have_cmd systemctl; then
        return 1
    fi

    systemctl --user list-unit-files "${unit}" --no-legend --no-pager 2>/dev/null | awk -v unit="${unit}" '$1 == unit { found = 1 } END { exit(found ? 0 : 1) }'
}

stop_if_present() {
    local unit="$1"
    if unit_exists "${unit}"; then
        log "stopping ${unit}"
        run_user_systemctl stop "${unit}" || true
    fi
}

start_if_present() {
    local unit="$1"
    if unit_exists "${unit}"; then
        log "starting ${unit}"
        run_user_systemctl start "${unit}" || true
    fi
}

kill_user_processes() {
    local pattern="$1"
    local label="$2"
    local pids=""

    if have_cmd pgrep; then
        pids="$(pgrep -u "$(id -u)" -f "${pattern}" || true)"
    elif have_cmd ps; then
        pids="$(ps -u "$(id -u)" -o pid=,args= | awk -v pattern="${pattern}" '$0 ~ pattern { print $1 }' || true)"
    fi

    if [[ -z "${pids}" ]]; then
        return 0
    fi

    while IFS= read -r pid; do
        [[ -n "${pid}" ]] || continue
        log "killing ${label} process ${pid}"
        kill "${pid}" 2>/dev/null || true
    done <<< "${pids}"
}

cleanup_virtual_audio_devices() {
    local cleanup_script="${SCRIPT_DIR}/remove_virtual_audio_devices.sh"

    if [[ -x "${cleanup_script}" ]]; then
        "${cleanup_script}" || true
    fi
}

pick_physical_source() {
    have_cmd pactl || return 1

    pactl list short sources 2>/dev/null | awk '
        $2 !~ /\.monitor$/ &&
        $2 !~ /^Viberdeskmic$/ &&
        $2 !~ /^vibe_rdesk_/ {
            print $2
            exit
        }
    '
}

pick_physical_sink() {
    have_cmd pactl || return 1

    pactl list short sinks 2>/dev/null | awk '
        $2 !~ /^vibe_rdesk_/ &&
        $2 !~ /^vibe_rdesk_virtual_mic_sink$/ {
            print $2
            exit
        }
    '
}

restore_audio_defaults() {
    local source_name=""
    local sink_name=""

    have_cmd pactl || return 0

    source_name="$(pick_physical_source || true)"
    sink_name="$(pick_physical_sink || true)"

    if [[ -n "${sink_name}" ]]; then
        log "restoring default sink ${sink_name}"
        pactl set-default-sink "${sink_name}" >/dev/null 2>&1 || true
        pactl set-sink-mute "${sink_name}" 0 >/dev/null 2>&1 || true
    fi

    if [[ -n "${source_name}" ]]; then
        log "restoring default source ${source_name}"
        pactl set-default-source "${source_name}" >/dev/null 2>&1 || true
        pactl set-source-mute "${source_name}" 0 >/dev/null 2>&1 || true
    fi
}

restore_alsa_capture_controls() {
    if ! have_cmd amixer; then
        return 0
    fi

    log "restoring ALSA capture controls"
    amixer -c 0 sset Capture cap >/dev/null 2>&1 || true
    amixer -c 0 sset Capture 100% >/dev/null 2>&1 || true
    amixer -c 0 sset 'Internal Mic Boost' 0 >/dev/null 2>&1 || true
}

main() {
    log "resetting user audio services"

    cleanup_virtual_audio_devices

    if have_cmd systemctl; then
        run_user_systemctl daemon-reload || true
        run_user_systemctl reset-failed || true
    fi

    stop_if_present pipewire-pulse.service
    stop_if_present pipewire-pulse.socket
    stop_if_present wireplumber.service
    stop_if_present pipewire.service
    stop_if_present pipewire.socket
    stop_if_present pulseaudio.service
    stop_if_present pulseaudio.socket

    kill_user_processes '(^|/)(pw-loopback|wireplumber|pipewire|pipewire-pulse|pulseaudio)( |$)' "audio"
    sleep 1

    start_if_present pipewire.socket
    start_if_present pipewire-pulse.socket
    start_if_present pipewire.service
    start_if_present pipewire-pulse.service
    start_if_present wireplumber.service
    start_if_present pulseaudio.socket
    start_if_present pulseaudio.service

    if have_cmd pulseaudio; then
        log "checking pulseaudio daemon"
        pulseaudio --check >/dev/null 2>&1 || pulseaudio --start >/dev/null 2>&1 || true
    fi

    sleep 1

    restore_alsa_capture_controls
    restore_audio_defaults

    if have_cmd pactl; then
        log "audio server status"
        pactl info 2>/dev/null | sed -n '1,12p' || true
        log "source status"
        pactl list short sources 2>/dev/null | sed -n '1,12p' || true
    fi

    log "done"
}

main "$@"
