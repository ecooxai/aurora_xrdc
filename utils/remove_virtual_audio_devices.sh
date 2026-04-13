#!/usr/bin/env bash
set -euo pipefail

VIRTUAL_MIC_SOURCE_NAME="${VIBE_RDESK_VIRTUAL_MIC_SOURCE_NAME:-Viberdeskmic}"
VIRTUAL_MIC_SINK_NAME="${VIBE_RDESK_VIRTUAL_MIC_SINK_NAME:-vibe_rdesk_virtual_mic_sink}"
AUDIO_SINK_PREFIX="${VIBE_RDESK_AUDIO_SINK_PREFIX:-vibe_rdesk_}"
PIPEWIRE_LOOPBACK_NAME="${VIRTUAL_MIC_SOURCE_NAME}_loopback"

log() {
    printf '[cleanup-audio] %s\n' "$*"
}

have_cmd() {
    command -v "$1" >/dev/null 2>&1
}

kill_matching_loopbacks() {
    local pids=""

    if have_cmd pgrep; then
        pids="$(pgrep -f "pw-loopback .*(${PIPEWIRE_LOOPBACK_NAME}|${VIRTUAL_MIC_SOURCE_NAME}|${VIRTUAL_MIC_SINK_NAME}|${AUDIO_SINK_PREFIX})" || true)"
    elif have_cmd ps; then
        pids="$(ps -eo pid=,args= | awk -v loopback_name="${PIPEWIRE_LOOPBACK_NAME}" -v source_name="${VIRTUAL_MIC_SOURCE_NAME}" -v sink_name="${VIRTUAL_MIC_SINK_NAME}" -v sink_prefix="${AUDIO_SINK_PREFIX}" '
            /pw-loopback/ && (
                index($0, loopback_name) ||
                index($0, source_name) ||
                index($0, sink_name) ||
                index($0, sink_prefix)
            ) {
                print $1
            }
        ' || true)"
    fi

    if [[ -z "${pids}" ]]; then
        return 0
    fi

    while IFS= read -r pid; do
        [[ -n "${pid}" ]] || continue
        log "killing pw-loopback process ${pid}"
        kill "${pid}" 2>/dev/null || true
    done <<< "${pids}"
}

unload_pulse_modules() {
    have_cmd pactl || return 0

    pactl list short modules 2>/dev/null | while IFS=$'\t' read -r module_id module_name module_args _; do
        [[ -n "${module_id}" && -n "${module_name}" ]] || continue

        case "${module_name}" in
            module-remap-source)
                if [[ "${module_args}" == *"source_name=${VIRTUAL_MIC_SOURCE_NAME}"* ]]; then
                    log "unloading PulseAudio module ${module_id} (${module_name})"
                    pactl unload-module "${module_id}" >/dev/null 2>&1 || true
                fi
                ;;
            module-null-sink)
                if [[ "${module_args}" == *"sink_name=${VIRTUAL_MIC_SINK_NAME}"* || "${module_args}" == *"sink_name=${AUDIO_SINK_PREFIX}"* ]]; then
                    log "unloading PulseAudio module ${module_id} (${module_name})"
                    pactl unload-module "${module_id}" >/dev/null 2>&1 || true
                fi
                ;;
        esac
    done
}

destroy_pipewire_nodes() {
    have_cmd pw-cli || return 0

    pw-cli ls Node 2>/dev/null | awk '
        /^[[:space:]]*id / {
            if (id != "" && name != "" && class != "") {
                print id "\t" name "\t" class
            }
            id = $2
            sub(/,/, "", id)
            name = ""
            class = ""
            next
        }
        /node\.name = "/ {
            split($0, parts, "\"")
            name = parts[2]
            next
        }
        /media\.class = "/ {
            split($0, parts, "\"")
            class = parts[2]
            next
        }
        END {
            if (id != "" && name != "" && class != "") {
                print id "\t" name "\t" class
            }
        }
    ' | while IFS=$'\t' read -r node_id node_name media_class; do
        [[ -n "${node_id}" && -n "${node_name}" && -n "${media_class}" ]] || continue

        case "${media_class}:${node_name}" in
            "Audio/Source:${VIRTUAL_MIC_SOURCE_NAME}"|"Audio/Sink:${VIRTUAL_MIC_SINK_NAME}")
                log "destroying PipeWire node ${node_name} (${node_id})"
                pw-cli destroy "${node_id}" >/dev/null 2>&1 || true
                ;;
            Audio/Sink:*)
                if [[ "${node_name}" == "${AUDIO_SINK_PREFIX}"* ]]; then
                    log "destroying PipeWire node ${node_name} (${node_id})"
                    pw-cli destroy "${node_id}" >/dev/null 2>&1 || true
                fi
                ;;
        esac
    done
}

main() {
    log "removing vibe_rdesk virtual audio devices"
    kill_matching_loopbacks
    unload_pulse_modules
    destroy_pipewire_nodes
    log "done"
}

main "$@"
