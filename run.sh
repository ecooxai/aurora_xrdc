#!/usr/bin/env bash
set -euo pipefail

SCRIPT_PATH="${BASH_SOURCE[0]}"
while [[ -L "${SCRIPT_PATH}" ]]; do
    SCRIPT_DIR="$(cd -P "$(dirname "${SCRIPT_PATH}")" && pwd)"
    SCRIPT_PATH="$(readlink "${SCRIPT_PATH}")"
    [[ "${SCRIPT_PATH}" == /* ]] || SCRIPT_PATH="${SCRIPT_DIR}/${SCRIPT_PATH}"
done
SCRIPT_DIR="$(cd -P "$(dirname "${SCRIPT_PATH}")" && pwd)"

APP_BINARY="${VIBE_RDESK_BINARY:-}"
if [[ -z "${APP_BINARY}" && -x "${SCRIPT_DIR}/vibe_rdesk" ]]; then
    APP_BINARY="${SCRIPT_DIR}/vibe_rdesk"
elif [[ -z "${APP_BINARY}" && -n "${APPDIR:-}" && -x "${APPDIR}/usr/bin/vibe_rdesk" ]]; then
    APP_BINARY="${APPDIR}/usr/bin/vibe_rdesk"
elif [[ -z "${APP_BINARY}" ]]; then
    APP_BINARY="target/release/vibe_rdesk"
fi

# Prefer a rustup-managed toolchain when available. Under sudo, use the
# invoking user's rustup path because root often does not have cargo installed.
CARGO_ENV_HOME="${HOME}"
if [[ "$(id -u)" -eq 0 && -n "${SUDO_UID:-}" && "${SUDO_UID}" != "0" ]]; then
    SUDO_PASSWD_ENTRY="$(getent passwd "${SUDO_UID}" 2>/dev/null || true)"
    if [[ -n "${SUDO_PASSWD_ENTRY}" ]]; then
        IFS=: read -r _ _ _ _ _ SUDO_HOME_FOR_CARGO _ <<<"${SUDO_PASSWD_ENTRY}"
        if [[ -n "${SUDO_HOME_FOR_CARGO}" ]]; then
            CARGO_ENV_HOME="${SUDO_HOME_FOR_CARGO}"
        fi
    fi
fi
if [[ -f "${CARGO_ENV_HOME}/.cargo/env" ]]; then
    if [[ -d "${CARGO_ENV_HOME}/.cargo" ]]; then
        export CARGO_HOME="${CARGO_HOME:-${CARGO_ENV_HOME}/.cargo}"
    fi
    if [[ -d "${CARGO_ENV_HOME}/.rustup" ]]; then
        export RUSTUP_HOME="${RUSTUP_HOME:-${CARGO_ENV_HOME}/.rustup}"
    fi
    # shellcheck disable=SC1090
    SAVED_HOME_FOR_CARGO="${HOME}"
    export HOME="${CARGO_ENV_HOME}"
    . "${CARGO_ENV_HOME}/.cargo/env"
    export HOME="${SAVED_HOME_FOR_CARGO}"
fi

WATCH_DIRS=(src web Cargo.toml)
DEBOUNCE_SECONDS=5
REBUILD_RETRY_SECONDS=10
HEADLESS_DISPLAY_RAW="${VIBE_RDESK_HEADLESS_DISPLAY:-11}"
XVFB_SCREEN="${VIBE_RDESK_XVFB_SCREEN:-1280x720x24}"
WINDOW_MANAGER_DELAY_SECONDS=2
XTERM_FONT_FAMILY="${VIBE_RDESK_XTERM_FONT_FAMILY:-Monospace}"
XTERM_FONT_SIZE="${VIBE_RDESK_XTERM_FONT_SIZE:-10}"
HEADLESS_LAUNCHER="${VIBE_RDESK_HEADLESS_LAUNCHER:-${SCRIPT_DIR}/vendor/aurora-wm}"
HEADLESS_INPUT_BACKEND="${VIBE_RDESK_HEADLESS_INPUT_BACKEND:-x11}"
FORCE_HEADLESS_DISPLAY="${VIBE_RDESK_FORCE_HEADLESS_DISPLAY:-no}"
NEW_DISPLAY_NOTICE_SECONDS="${VIBE_RDESK_NEW_DISPLAY_NOTICE_SECONDS:-3}"
STARTUP_HELP_SECONDS="${VIBE_RDESK_HELP_SECONDS:-3}"
VIRTUAL_MIC_SOURCE_NAME="${VIBE_RDESK_VIRTUAL_MIC_SOURCE_NAME:-Viberdeskmic}"
VIRTUAL_MIC_SINK_NAME="${VIBE_RDESK_VIRTUAL_MIC_SINK_NAME:-vibe_rdesk_virtual_mic_sink}"
DEFAULT_HTTPS_PORT=18443
DEFAULT_HTTP_PORT=18080
VIBE_RDESK_BIND_WAS_SET=0
if [[ -n "${VIBE_RDESK_BIND+x}" ]]; then
    VIBE_RDESK_BIND_WAS_SET=1
fi
DEFAULT_BIND="${VIBE_RDESK_BIND:-0.0.0.0:${DEFAULT_HTTPS_PORT},[::]:${DEFAULT_HTTPS_PORT}}"
export VIBE_RDESK_BIND="${DEFAULT_BIND}"
DEV_LOG_UID="${SUDO_UID:-$(id -u)}"
DEV_LOG_DIR="${VIBE_RDESK_LOG_DIR:-/tmp/vibe_rdesk-${DEV_LOG_UID}}"
AUDIO_BACKEND=""
SUDO_AUDIO_USER=""
SUDO_AUDIO_UID=""
SUDO_AUDIO_HOME=""
SUDO_AUDIO_RUNTIME_DIR=""
APP_PID=""
XVFB_PID=""
LAUNCHER_PID=""
PULSE_PID=""
PIPEWIRE_LOOPBACK_PID=""
XTERM_PID=""
DBUS_PID=""
HTTPS_ENABLED="yes"

if [[ -n "${APPDIR:-}" && -z "${VIBE_RDESK_SSL_DIR:-}" && -d "${APPDIR}/ssl_keys" ]]; then
    export VIBE_RDESK_SSL_DIR="${APPDIR}/ssl_keys"
fi
SSL_DIR="${VIBE_RDESK_SSL_DIR:-ssl_keys}"
SSL_CERT="${VIBE_RDESK_TLS_CERT:-${SSL_DIR}/server.crt}"
SSL_KEY="${VIBE_RDESK_TLS_KEY:-${SSL_DIR}/server.key}"

APP_ARGS=()

configure_appimage_desktop_env() {
    [[ -n "${APPDIR:-}" ]] || return 0

    if [[ -d "${APPDIR}/usr/share/icewm" ]]; then
        export ICEWM_PRIVCFG="${ICEWM_PRIVCFG:-${APPDIR}/usr/share/icewm}"
    fi
    if [[ -d "${APPDIR}/usr/share/icons" ]]; then
        export XCURSOR_PATH="${XCURSOR_PATH:-${APPDIR}/usr/share/icons:/usr/share/icons}"
    fi
    if [[ -d "${APPDIR}/usr/lib/x86_64-linux-gnu/gdk-pixbuf-2.0/2.10.0/loaders" ]]; then
        export GDK_PIXBUF_MODULEDIR="${GDK_PIXBUF_MODULEDIR:-${APPDIR}/usr/lib/x86_64-linux-gnu/gdk-pixbuf-2.0/2.10.0/loaders}"
    elif [[ -d "${APPDIR}/usr/lib/aarch64-linux-gnu/gdk-pixbuf-2.0/2.10.0/loaders" ]]; then
        export GDK_PIXBUF_MODULEDIR="${GDK_PIXBUF_MODULEDIR:-${APPDIR}/usr/lib/aarch64-linux-gnu/gdk-pixbuf-2.0/2.10.0/loaders}"
    fi
}

prepare_gdk_pixbuf_cache() {
    [[ -n "${APPDIR:-}" && -n "${GDK_PIXBUF_MODULEDIR:-}" ]] || return 0
    local query_loaders=""

    if [[ -x "${APPDIR}/usr/lib/x86_64-linux-gnu/gdk-pixbuf-2.0/gdk-pixbuf-query-loaders" ]]; then
        query_loaders="${APPDIR}/usr/lib/x86_64-linux-gnu/gdk-pixbuf-2.0/gdk-pixbuf-query-loaders"
    elif [[ -x "${APPDIR}/usr/lib/aarch64-linux-gnu/gdk-pixbuf-2.0/gdk-pixbuf-query-loaders" ]]; then
        query_loaders="${APPDIR}/usr/lib/aarch64-linux-gnu/gdk-pixbuf-2.0/gdk-pixbuf-query-loaders"
    fi
    [[ -n "${query_loaders}" ]] || return 0

    export GDK_PIXBUF_MODULE_FILE="${GDK_PIXBUF_MODULE_FILE:-${DEV_LOG_DIR}/gdk-pixbuf-loaders.cache}"
    if [[ ! -s "${GDK_PIXBUF_MODULE_FILE}" ]]; then
        "${query_loaders}" >"${GDK_PIXBUF_MODULE_FILE}" 2>"${DEV_LOG_DIR}/gdk-pixbuf-query-loaders.log" || true
    fi
}

parse_script_args() {
    local args=("$@")
    local i
    APP_ARGS=()

    for ((i = 0; i < ${#args[@]}; i++)); do
        case "${args[$i]}" in
            --launcher)
                if (( i + 1 >= ${#args[@]} )) || [[ -z "${args[$((i + 1))]}" ]]; then
                    echo "[run] --launcher requires a command. Example: ./run.sh --launcher xterm --passwd <password>" >&2
                    exit 1
                fi
                HEADLESS_LAUNCHER="${args[$((i + 1))]}"
                ((i += 1))
                ;;
            --headless|--force-headless)
                if (( i + 1 < ${#args[@]} )) && [[ "${args[$((i + 1))]}" =~ ^(yes|no)$ ]]; then
                    FORCE_HEADLESS_DISPLAY="${args[$((i + 1))]}"
                    ((i += 1))
                else
                    FORCE_HEADLESS_DISPLAY="yes"
                fi
                ;;
            *)
                APP_ARGS+=("${args[$i]}")
                ;;
        esac
    done
}

require_passwd_arg() {
    local args=("$@")
    local i
    for ((i = 0; i < ${#args[@]}; i++)); do
        case "${args[$i]}" in
            --passwd)
                if (( i + 1 >= ${#args[@]} )) || [[ -z "${args[$((i + 1))]}" ]]; then
                    echo "[run] --passwd requires a non-empty value. Start with: bash run.sh --passwd passwd --port 18443 --https yes --headless no" >&2
                    exit 1
                fi
                return 0
                ;;
        esac
    done

    echo "[run] missing required --passwd. Start with: bash run.sh --passwd passwd --port 18443 --https yes --headless no" >&2
    exit 1
}

first_bind_port() {
    local bind_list="${1:-}"
    local part port
    IFS=',' read -ra parts <<<"${bind_list}"
    for part in "${parts[@]}"; do
        port="${part##*:}"
        if [[ "${port}" =~ ^[0-9]+$ ]]; then
            printf '%s\n' "${port}"
            return 0
        fi
    done
    if [[ "${HTTPS_ENABLED}" == "yes" ]]; then
        printf '%s\n' "${DEFAULT_HTTPS_PORT}"
    else
        printf '%s\n' "${DEFAULT_HTTP_PORT}"
    fi
}

format_access_url() {
    local protocol="$1"
    local host="$2"
    local port="$3"
    local iface="${4:-}"

    if [[ "${host}" == *:* ]]; then
        if [[ "${host}" == fe80:* && -n "${iface}" ]]; then
            host="${host}%25${iface}"
        fi
        printf '%s://[%s]:%s/' "${protocol}" "${host}" "${port}"
    else
        printf '%s://%s:%s/' "${protocol}" "${host}" "${port}"
    fi
}

show_access_urls() {
    local label="$1"
    local protocol="$2"
    local port="$3"
    local ifname family cidr addr key url
    local -A seen=()
    local printed=0

    echo "${label} URLs:"

    if command -v ip >/dev/null 2>&1; then
        while read -r ifname family cidr; do
            [[ -n "${ifname}" && -n "${family}" && -n "${cidr}" ]] || continue
            case "${family}" in
                inet|inet6) ;;
                *) continue ;;
            esac

            ifname="${ifname%%@*}"
            addr="${cidr%%/*}"
            [[ -n "${addr}" ]] || continue

            key="${ifname}|${addr}"
            if [[ -n "${seen[$key]+x}" ]]; then
                continue
            fi
            seen[$key]=1

            url="$(format_access_url "${protocol}" "${addr}" "${port}" "${ifname}")"
            echo "${label}   ${ifname}: ${url}"
            printed=1
        done < <(ip -o addr show 2>/dev/null | awk '$3 == "inet" || $3 == "inet6" {print $2, $3, $4}')
    fi

    if [[ "${printed}" == "0" && "$(command -v hostname || true)" ]]; then
        for addr in $(hostname -I 2>/dev/null || true); do
            addr="${addr%%%*}"
            [[ -n "${addr}" ]] || continue

            key="hostname|${addr}"
            if [[ -n "${seen[$key]+x}" ]]; then
                continue
            fi
            seen[$key]=1

            url="$(format_access_url "${protocol}" "${addr}" "${port}")"
            echo "${label}   ${url}"
            printed=1
        done
    fi

    if [[ "${printed}" == "0" ]]; then
        echo "${label}   unable to detect interface addresses"
    fi
}

configure_bind_args() {
    local args=("$@")
    local port=""
    local port_explicit="no"
    local localhost="no"
    local i value

    for ((i = 0; i < ${#args[@]}; i++)); do
        case "${args[$i]}" in
            --https)
                if (( i + 1 >= ${#args[@]} )); then
                    echo "[run] --https requires yes or no" >&2
                    exit 1
                fi
                value="${args[$((i + 1))]}"
                case "${value}" in
                    yes|no) HTTPS_ENABLED="${value}" ;;
                    *)
                        echo "[run] --https must be yes or no" >&2
                        exit 1
                        ;;
                esac
                ;;
            -p|--port)
                if (( i + 1 >= ${#args[@]} )) || [[ ! "${args[$((i + 1))]}" =~ ^[0-9]+$ ]]; then
                    echo "[run] -p/--port requires a numeric port value" >&2
                    exit 1
                fi
                port="${args[$((i + 1))]}"
                port_explicit="yes"
                ;;
            --localhost)
                if (( i + 1 >= ${#args[@]} )); then
                    echo "[run] --localhost requires yes or no" >&2
                    exit 1
                fi
                value="${args[$((i + 1))]}"
                case "${value}" in
                    yes|no) localhost="${value}" ;;
                    *)
                        echo "[run] --localhost must be yes or no" >&2
                        exit 1
                        ;;
                esac
                ;;
        esac
    done

    if [[ -z "${port}" ]]; then
        if [[ "${VIBE_RDESK_BIND_WAS_SET}" == "1" ]]; then
            port="$(first_bind_port "${VIBE_RDESK_BIND}")"
        elif [[ "${HTTPS_ENABLED}" == "yes" ]]; then
            port="${DEFAULT_HTTPS_PORT}"
        else
            port="${DEFAULT_HTTP_PORT}"
        fi
    fi

    if [[ "${localhost}" == "yes" ]]; then
        export VIBE_RDESK_BIND="127.0.0.1:${port},[::1]:${port}"
    elif [[ "${port_explicit}" == "yes" || "${VIBE_RDESK_BIND_WAS_SET}" == "0" ]]; then
        export VIBE_RDESK_BIND="0.0.0.0:${port},[::]:${port}"
    fi
}

show_startup_help() {
    local protocol="$1"
    local port
    port="$(first_bind_port "${VIBE_RDESK_BIND}")"

    local server_label="HTTPS"
    local reminder="use https://, not http://."
    if [[ "${protocol}" == "http" ]]; then
        server_label="HTTP"
        reminder="using normal http:// because --https no was specified."
    fi

    cat <<EOF
[run] Production:  bash run.sh --passwd passwd --port 18443 --https yes --headless no
[run] Development: bash dev.sh --passwd passwd
[run] starting vibe_rdesk ${server_label} server
[run] bind: ${VIBE_RDESK_BIND}
[run] ${reminder} Default is all IPv4/IPv6 interfaces; use --localhost yes for loopback only.
[run] Use --https no to disable built-in HTTPS and serve HTTP.
[run] If you need HTTPS with --https no, put an HTTPS proxy in front of this server.
[run] starting in ${STARTUP_HELP_SECONDS}s...
EOF
}

show_server_access_urls() {
    local protocol="$1"
    local port
    port="$(first_bind_port "${VIBE_RDESK_BIND}")"

    if ! wait_for_server_port "${port}" && [[ -n "${APP_PID}" ]] && ! kill -0 "${APP_PID}" 2>/dev/null; then
        return 1
    fi
    echo "[run] server started on ${VIBE_RDESK_BIND}"
    show_access_urls "[run]" "${protocol}" "${port}"
}

wait_for_server_port() {
    local port="$1"
    local attempts="${2:-60}"

    while (( attempts > 0 )); do
        if bash -c ":</dev/tcp/127.0.0.1/${port}" >/dev/null 2>&1; then
            return 0
        fi
        if [[ -n "${APP_PID}" ]] && ! kill -0 "${APP_PID}" 2>/dev/null; then
            return 1
        fi
        sleep 0.25
        ((attempts--))
    done

    return 1
}

ensure_tls_keys() {
    if [[ -f "${SSL_CERT}" && -f "${SSL_KEY}" ]]; then
        echo "[run] using TLS certificate ${SSL_CERT}"
        return 0
    fi

    if [[ -f "${SSL_CERT}" || -f "${SSL_KEY}" ]]; then
        echo "[run] TLS certificate/key pair is incomplete; expected both ${SSL_CERT} and ${SSL_KEY}" >&2
        exit 1
    fi

    if ! command -v openssl >/dev/null 2>&1; then
        echo "[run] openssl is required to generate test TLS keys in ${SSL_DIR}" >&2
        exit 1
    fi

    mkdir -p "${SSL_DIR}"
    chmod 700 "${SSL_DIR}" 2>/dev/null || true

    local openssl_config="${SSL_DIR}/server.openssl.cnf"
    cat >"${openssl_config}" <<'EOF'
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
EOF

    echo "[run] generating self-signed test TLS certificate in ${SSL_DIR}"
    openssl req \
        -x509 \
        -newkey rsa:2048 \
        -sha256 \
        -days "${VIBE_RDESK_TEST_CERT_DAYS:-3650}" \
        -nodes \
        -keyout "${SSL_KEY}" \
        -out "${SSL_CERT}" \
        -config "${openssl_config}" \
        >/dev/null 2>&1
    chmod 600 "${SSL_KEY}" 2>/dev/null || true
}

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
        local info
        if ! info="$(xdpyinfo -display "${display}" -ext XTEST 2>&1)"; then
            return 1
        fi
        if printf '%s\n' "${info}" | grep -q "XTEST extension not supported"; then
            return 1
        fi
        return 0
    fi

    DISPLAY="${display}" xdotool getmouselocation >/dev/null 2>&1
}

display_number() {
    local display="${1:-}"
    local normalized number

    normalized="$(normalize_display "${display}")" || return 1
    number="${normalized#:}"
    number="${number%%.*}"

    if [[ ! "${number}" =~ ^[0-9]+$ ]]; then
        return 1
    fi

    printf '%s\n' "${number}"
}

is_display_reserved() {
    local display="${1:-}"
    local number

    number="$(display_number "${display}")" || return 0

    [[ -e "/tmp/.X11-unix/X${number}" || -e "/tmp/.X${number}-lock" ]]
}

find_free_headless_display() {
    local start_display="${1:-11}"
    local start_number candidate attempts=100

    start_number="$(display_number "${start_display}")" || start_number="11"

    while (( attempts > 0 )); do
        candidate=":${start_number}"
        if ! is_display_reserved "${candidate}" && ! is_display_available "${candidate}"; then
            printf '%s\n' "${candidate}"
            return 0
        fi
        ((start_number++))
        ((attempts--))
    done

    return 1
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

ensure_uinput_access() {
    if [[ "${VIBE_RDESK_SKIP_UINPUT_SETUP:-0}" == "1" ]]; then
        echo "[dev] uinput: setup skipped by VIBE_RDESK_SKIP_UINPUT_SETUP=1"
        return 0
    fi

    local target_user=""

    if [[ "$(id -u)" -eq 0 && -n "${SUDO_USER:-}" && "${SUDO_USER}" != "root" ]]; then
        target_user="${SUDO_USER}"
    else
        target_user="$(id -un)"
    fi

    if [[ -e /dev/uinput && -r /dev/uinput && -w /dev/uinput ]]; then
        echo "[dev] uinput: current process can access /dev/uinput"
        return 0
    fi

    # Manual equivalent:
    #   sudo modprobe uinput
    #   sudo setfacl -m "u:$(id -un):rw" /dev/uinput
    if [[ "$(id -u)" -ne 0 ]]; then
        if ! command -v sudo >/dev/null 2>&1; then
            echo "[dev] uinput: sudo is unavailable; pointer motion and smooth wheel will fall back"
            return 0
        fi

        if ! sudo -v; then
            echo "[dev] uinput: sudo auth failed; pointer motion and smooth wheel will fall back"
            return 0
        fi
    fi

    if [[ "$(id -u)" -eq 0 ]]; then
        modprobe uinput 2>/dev/null || true
    else
        sudo modprobe uinput 2>/dev/null || true
    fi

    if [[ ! -e /dev/uinput ]]; then
        echo "[dev] uinput: /dev/uinput is unavailable; pointer motion and smooth wheel will fall back"
        return 0
    fi

    if command -v setfacl >/dev/null 2>&1; then
        if [[ "$(id -u)" -eq 0 ]]; then
            if setfacl -m "u:${target_user}:rw" /dev/uinput 2>/dev/null; then
                echo "[dev] uinput: granted ${target_user} rw access with setfacl"
                return 0
            fi
        elif sudo setfacl -m "u:${target_user}:rw" /dev/uinput 2>/dev/null; then
            echo "[dev] uinput: granted ${target_user} rw access with setfacl"
            return 0
        fi

        echo "[dev] uinput: setfacl failed; trying chown fallback"
    fi

    if [[ "$(id -u)" -eq 0 ]]; then
        if chown "${target_user}" /dev/uinput 2>/dev/null && chmod u+rw /dev/uinput 2>/dev/null; then
            echo "[dev] uinput: granted ${target_user} rw access with chown"
            return 0
        fi
    elif sudo chown "${target_user}" /dev/uinput 2>/dev/null && sudo chmod u+rw /dev/uinput 2>/dev/null; then
        echo "[dev] uinput: granted ${target_user} rw access with chown"
        return 0
    fi

    echo "[dev] uinput: unable to grant access; pointer motion and smooth wheel will fall back"
}

configure_sudo_audio_env() {
    if [[ "$(id -u)" -ne 0 || -z "${SUDO_UID:-}" || "${SUDO_UID}" == "0" ]]; then
        return 0
    fi

    local passwd_entry=""
    local passwd_user=""
    local passwd_home=""

    passwd_entry="$(getent passwd "${SUDO_UID}" 2>/dev/null || true)"
    if [[ -n "${passwd_entry}" ]]; then
        IFS=: read -r passwd_user _ _ _ _ passwd_home _ <<<"${passwd_entry}"
    fi

    SUDO_AUDIO_UID="${SUDO_UID}"
    SUDO_AUDIO_USER="${SUDO_USER:-${passwd_user}}"
    SUDO_AUDIO_HOME="${passwd_home}"
    SUDO_AUDIO_RUNTIME_DIR="/run/user/${SUDO_AUDIO_UID}"

    if [[ -z "${SUDO_AUDIO_USER}" ]]; then
        echo "[dev] sudo audio: unable to resolve user for uid ${SUDO_AUDIO_UID}" >&2
        return 0
    fi

    if [[ ! -d "${SUDO_AUDIO_RUNTIME_DIR}" ]]; then
        echo "[dev] sudo audio: ${SUDO_AUDIO_RUNTIME_DIR} does not exist; the normal user's audio session may not be running" >&2
        return 0
    fi

    export XDG_RUNTIME_DIR="${SUDO_AUDIO_RUNTIME_DIR}"
    export PULSE_SERVER="unix:${SUDO_AUDIO_RUNTIME_DIR}/pulse/native"

    if [[ -z "${PULSE_COOKIE:-}" && -n "${SUDO_AUDIO_HOME}" ]]; then
        if [[ -r "${SUDO_AUDIO_HOME}/.config/pulse/cookie" ]]; then
            export PULSE_COOKIE="${SUDO_AUDIO_HOME}/.config/pulse/cookie"
        elif [[ -r "${SUDO_AUDIO_HOME}/.pulse-cookie" ]]; then
            export PULSE_COOKIE="${SUDO_AUDIO_HOME}/.pulse-cookie"
        fi
    fi

    if [[ -S "${SUDO_AUDIO_RUNTIME_DIR}/bus" ]]; then
        export DBUS_SESSION_BUS_ADDRESS="unix:path=${SUDO_AUDIO_RUNTIME_DIR}/bus"
    fi

    echo "[dev] sudo audio: using ${SUDO_AUDIO_USER}'s audio session at ${SUDO_AUDIO_RUNTIME_DIR}"
}

using_sudo_audio_user() {
    [[ "$(id -u)" -eq 0 && -n "${SUDO_AUDIO_USER}" && -n "${SUDO_AUDIO_UID}" && "${SUDO_AUDIO_UID}" != "0" ]]
}

run_as_audio_user() {
    local -a env_args=()

    if [[ -n "${XDG_RUNTIME_DIR:-}" ]]; then
        env_args+=("XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR}")
    fi
    if [[ -n "${PULSE_SERVER:-}" ]]; then
        env_args+=("PULSE_SERVER=${PULSE_SERVER}")
    fi
    if [[ -n "${PULSE_COOKIE:-}" ]]; then
        env_args+=("PULSE_COOKIE=${PULSE_COOKIE}")
    fi
    if [[ -n "${DBUS_SESSION_BUS_ADDRESS:-}" ]]; then
        env_args+=("DBUS_SESSION_BUS_ADDRESS=${DBUS_SESSION_BUS_ADDRESS}")
    fi
    if [[ -n "${LD_LIBRARY_PATH:-}" ]]; then
        env_args+=("LD_LIBRARY_PATH=${LD_LIBRARY_PATH}")
    fi

    if using_sudo_audio_user && command -v runuser >/dev/null 2>&1; then
        runuser -u "${SUDO_AUDIO_USER}" -- env "${env_args[@]}" "$@"
        return $?
    fi

    env "${env_args[@]}" "$@"
}

session_bus_available() {
    [[ -n "${DBUS_SESSION_BUS_ADDRESS:-}" ]] || return 1
    timeout 1s dbus-send \
        --session \
        --type=method_call \
        --dest=org.freedesktop.DBus \
        / \
        org.freedesktop.DBus.ListNames >/dev/null 2>&1
}

start_private_session_bus() {
    if session_bus_available; then
        return 0
    fi
    command -v dbus-daemon >/dev/null 2>&1 || return 0

    local dbus_dir="${DEV_LOG_DIR}/dbus-$$"
    local dbus_socket="${dbus_dir}/session-bus"
    local dbus_output=""
    local dbus_address=""
    local dbus_pid=""

    mkdir -p "${dbus_dir}"
    chmod 700 "${dbus_dir}" 2>/dev/null || true

    if ! dbus_output="$(dbus-daemon \
        --session \
        --fork \
        --nopidfile \
        --address="unix:path=${dbus_socket}" \
        --print-address=1 \
        --print-pid=1 2>"${DEV_LOG_DIR}/dbus.log")"; then
        echo "[run] private D-Bus session bus did not start; see ${DEV_LOG_DIR}/dbus.log" >&2
        return 0
    fi

    dbus_address="$(printf '%s\n' "${dbus_output}" | sed -n '1p')"
    dbus_pid="$(printf '%s\n' "${dbus_output}" | sed -n '2p')"
    if [[ -n "${dbus_address}" && "${dbus_pid}" =~ ^[0-9]+$ ]]; then
        export DBUS_SESSION_BUS_ADDRESS="${dbus_address}"
        DBUS_PID="${dbus_pid}"
        echo "[run] started private D-Bus session bus"
    fi
}

ensure_dev_log_dir() {
    mkdir -p "${DEV_LOG_DIR}"
    if using_sudo_audio_user; then
        chown "${SUDO_AUDIO_USER}:" "${DEV_LOG_DIR}" 2>/dev/null || true
    fi
}

prepare_dev_log_file() {
    local log_file="$1"

    ensure_dev_log_dir
    : >"${log_file}"
    if using_sudo_audio_user; then
        chown "${SUDO_AUDIO_USER}:" "${log_file}" 2>/dev/null || true
    fi
}

source_exists() {
    local source_name="${1:-}"
    if [[ -z "${source_name}" ]]; then
        return 1
    fi

    run_as_audio_user timeout 1s pactl list short sources 2>/dev/null | awk '{print $2}' | grep -Fxq "${source_name}"
}

sink_exists() {
    local sink_name="${1:-}"
    if [[ -z "${sink_name}" ]]; then
        return 1
    fi

    run_as_audio_user timeout 1s pactl list short sinks 2>/dev/null | awk '{print $2}' | grep -Fxq "${sink_name}"
}

wait_for_pipewire_server() {
    local attempts=20

    while (( attempts > 0 )); do
        if run_as_audio_user pw-cli info 0 >/dev/null 2>&1; then
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

    node_id="$(run_as_audio_user pw-cli ls Node 2>/dev/null | awk -v node_name="${node_name}" -v media_class="${media_class}" '
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
    local attempts="${1:-10}"

    while (( attempts > 0 )); do
        if run_as_audio_user timeout 0.5s pactl info >/dev/null 2>&1; then
            return 0
        fi
        sleep 0.2
        ((attempts--))
    done

    return 1
}

start_private_pulse_server() {
    local pulse_dir="${DEV_LOG_DIR}/pulse-$$"
    local pulse_socket="${pulse_dir}/native"
    local log_file="${DEV_LOG_DIR}/pulseaudio-private.log"
    local pulse_module_dir=""
    local pulse_module_args=()

    prepare_dev_log_file "${log_file}"
    mkdir -p "${pulse_dir}"
    chmod 700 "${pulse_dir}" 2>/dev/null || true
    if using_sudo_audio_user; then
        chown -R "${SUDO_AUDIO_USER}:" "${pulse_dir}" 2>/dev/null || true
    fi

    export PULSE_SERVER="unix:${pulse_socket}"
    if [[ -n "${APPDIR:-}" ]]; then
        for dir in "${APPDIR}"/usr/lib/pulse-*/modules "${APPDIR}"/usr/lib/*/pulse-*/modules; do
            if [[ -d "${dir}" ]]; then
                pulse_module_dir="${dir}"
                break
            fi
        done
    fi
    if [[ -n "${pulse_module_dir}" ]]; then
        pulse_module_args=(--dl-search-path="${pulse_module_dir}")
    fi
    echo "[dev] starting private PulseAudio server at ${pulse_socket}"
    local saved_dbus_session_bus_address="${DBUS_SESSION_BUS_ADDRESS-}"
    local saved_ld_library_path="${LD_LIBRARY_PATH-}"
    export DBUS_SESSION_BUS_ADDRESS="unix:path=${pulse_dir}/dbus-disabled"
    if [[ -n "${pulse_module_dir}" ]]; then
        export LD_LIBRARY_PATH="${pulse_module_dir}:${LD_LIBRARY_PATH:-}"
    fi
    run_as_audio_user pulseaudio \
        "${pulse_module_args[@]}" \
        -n \
        --daemonize=no \
        --exit-idle-time=-1 \
        --disallow-exit=yes \
        --use-pid-file=no \
        --log-target=stderr \
        --load="module-native-protocol-unix socket=${pulse_socket} auth-anonymous=1" \
        --load="module-null-sink sink_name=${VIRTUAL_MIC_SINK_NAME} sink_properties=device.description=VibeRDeskVirtualMicSink" \
        >"${log_file}" 2>&1 &
    PULSE_PID=$!
    if [[ -n "${saved_dbus_session_bus_address}" ]]; then
        export DBUS_SESSION_BUS_ADDRESS="${saved_dbus_session_bus_address}"
    else
        unset DBUS_SESSION_BUS_ADDRESS
    fi
    if [[ -n "${saved_ld_library_path}" ]]; then
        export LD_LIBRARY_PATH="${saved_ld_library_path}"
    else
        unset LD_LIBRARY_PATH
    fi

    if wait_for_pulse_server; then
        return 0
    fi

    echo "[dev] private PulseAudio did not become ready; see ${log_file}" >&2
    return 1
}

start_pulse_server() {
    if run_as_audio_user timeout 1s pactl info >/dev/null 2>&1; then
        echo "[dev] using existing PulseAudio server"
        return 0
    fi

    if [[ -n "${APPDIR:-}" ]]; then
        start_private_pulse_server
        return $?
    fi

    if command -v dbus-launch >/dev/null 2>&1; then
        local log_file="${DEV_LOG_DIR}/pulseaudio.log"
        prepare_dev_log_file "${log_file}"
        echo "[dev] starting PulseAudio with dbus-launch"
        run_as_audio_user dbus-launch pulseaudio >"${log_file}" 2>&1 &
        PULSE_PID=$!
    else
        echo "[dev] starting PulseAudio"
        run_as_audio_user pulseaudio --start >/dev/null 2>&1 || true
    fi

    if ! wait_for_pulse_server; then
        echo "[dev] PulseAudio did not become ready" >&2
        start_private_pulse_server
        return $?
    fi
}

start_pipewire_server() {
    if wait_for_pipewire_server; then
        echo "[dev] using existing PipeWire server"
        return 0
    fi

    if command -v systemctl >/dev/null 2>&1; then
        echo "[dev] starting PipeWire user services"
        run_as_audio_user systemctl --user start pipewire pipewire-pulse wireplumber >/dev/null 2>&1 || true
    fi

    if ! wait_for_pipewire_server; then
        return 1
    fi
}

pipewire_uses_pulse_server() {
    run_as_audio_user timeout 1s pactl info 2>/dev/null | grep -Fq 'Server Name: PulseAudio (on PipeWire'
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
        run_as_audio_user pw-cli create-node adapter \
            "{ factory.name = support.null-audio-sink node.name = \"${VIRTUAL_MIC_SINK_NAME}\" node.description = \"VibeRDeskVirtualMicSink\" media.class = \"Audio/Sink\" object.linger = true audio.position = [ FL FR ] }" \
            >/dev/null
        if ! wait_for_pipewire_sink "${VIRTUAL_MIC_SINK_NAME}"; then
            echo "[dev] PipeWire virtual sink ${VIRTUAL_MIC_SINK_NAME} did not appear" >&2
            return 1
        fi
    fi

    echo "[dev] creating PipeWire virtual mic source ${VIRTUAL_MIC_SOURCE_NAME}"
    local log_file="${DEV_LOG_DIR}/pipewire-loopback.log"
    prepare_dev_log_file "${log_file}"
    run_as_audio_user pw-loopback \
        --name "${VIRTUAL_MIC_SOURCE_NAME}_loopback" \
        --capture-props "{ stream.capture.sink = true target.object = \"${VIRTUAL_MIC_SINK_NAME}\" node.passive = true node.dont-reconnect = true }" \
        --playback-props "{ node.name = \"${VIRTUAL_MIC_SOURCE_NAME}\" node.description = \"${VIRTUAL_MIC_SOURCE_NAME}\" media.class = \"Audio/Source\" audio.position = [ FL FR ] }" \
        >"${log_file}" 2>&1 &
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
        run_as_audio_user pactl load-module \
            module-null-sink \
            "sink_name=${VIRTUAL_MIC_SINK_NAME}" \
            "sink_properties=device.description=VibeRDeskVirtualMicSink" \
            >/dev/null
    fi

    echo "[dev] creating virtual mic source ${VIRTUAL_MIC_SOURCE_NAME}"
    run_as_audio_user pactl load-module \
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
    if ! DISPLAY="$(find_free_headless_display "${HEADLESS_DISPLAY_RAW}")"; then
        echo "[run] could not find a free X11 display starting at $(normalize_display "${HEADLESS_DISPLAY_RAW}")" >&2
        exit 1
    fi
    export VIBE_RDESK_INPUT_BACKEND="${VIBE_RDESK_INPUT_BACKEND:-${HEADLESS_INPUT_BACKEND}}"

    if ! command -v Xvfb >/dev/null 2>&1; then
        echo "[run] Xvfb is required for headless startup. Install it first." >&2
        exit 1
    fi

    local launcher_program="${HEADLESS_LAUNCHER%%[[:space:]]*}"
    if [[ -z "${launcher_program}" ]] || ! command -v "${launcher_program}" >/dev/null 2>&1; then
        echo "[run] launcher '${HEADLESS_LAUNCHER}' is not available. Install it first or use --launcher <command>." >&2
        exit 1
    fi

    local xvfb_log="${DEV_LOG_DIR}/xvfb.log"
    local launcher_log="${DEV_LOG_DIR}/launcher.log"
    local xkb_args=()

    if [[ -n "${XKB_CONFIG_ROOT:-}" && -d "${XKB_CONFIG_ROOT}" ]]; then
        xkb_args=(-xkbdir "${XKB_CONFIG_ROOT}")
    elif [[ -n "${APPDIR:-}" && -d "${APPDIR}/usr/share/X11/xkb" ]]; then
        xkb_args=(-xkbdir "${APPDIR}/usr/share/X11/xkb")
    fi

    echo "[run] starting Xvfb on ${DISPLAY}"
    prepare_dev_log_file "${xvfb_log}"
    {
        echo "[run] PATH=${PATH}"
        echo "[run] XKB_CONFIG_ROOT=${XKB_CONFIG_ROOT:-}"
        echo "[run] bundled keycodes evdev: ${XKB_CONFIG_ROOT:-}/keycodes/evdev"
        echo "[run] xkbcomp: $(command -v xkbcomp 2>/dev/null || true)"
        echo "[run] ICEWM_PRIVCFG=${ICEWM_PRIVCFG:-}"
        echo "[run] XCURSOR_PATH=${XCURSOR_PATH:-}"
        echo "[run] GDK_PIXBUF_MODULEDIR=${GDK_PIXBUF_MODULEDIR:-}"
        echo "[run] GDK_PIXBUF_MODULE_FILE=${GDK_PIXBUF_MODULE_FILE:-}"
        echo "[run] Xvfb args: ${DISPLAY} -screen 0 ${XVFB_SCREEN} -ac -nolisten tcp ${xkb_args[*]}"
    } >>"${xvfb_log}"
    Xvfb "${DISPLAY}" -screen 0 "${XVFB_SCREEN}" -ac -nolisten tcp "${xkb_args[@]}" >>"${xvfb_log}" 2>&1 &
    XVFB_PID=$!

    if ! wait_for_display "${DISPLAY}"; then
        echo "[run] Xvfb on ${DISPLAY} did not become ready; see ${xvfb_log}" >&2
        exit 1
    fi

    echo "[run] created headless X11 display ${DISPLAY}"
    sleep "${NEW_DISPLAY_NOTICE_SECONDS}"

    sleep "${WINDOW_MANAGER_DELAY_SECONDS}"

    echo "[run] starting launcher '${HEADLESS_LAUNCHER}' on ${DISPLAY}"
    prepare_dev_log_file "${launcher_log}"
    export VIBE_RDESK_HEADLESS_DISPLAY_ACTIVE=1
    if [[ "${HEADLESS_LAUNCHER}" == "xterm" ]]; then
        DISPLAY="${DISPLAY}" xterm \
            -display "${DISPLAY}" \
            -title "vibe_rdesk" \
            -fa "${XTERM_FONT_FAMILY}" \
            -fs "${XTERM_FONT_SIZE}" \
            -geometry 120x30+40+40 \
            >"${launcher_log}" 2>&1 &
    else
        DISPLAY="${DISPLAY}" bash -lc "exec ${HEADLESS_LAUNCHER}" >"${launcher_log}" 2>&1 &
    fi
    LAUNCHER_PID=$!

    if ! wait_for_process "${LAUNCHER_PID}" 4; then
        echo "[run] launcher '${HEADLESS_LAUNCHER}' exited during startup; see ${launcher_log}" >&2
        if [[ "${HEADLESS_LAUNCHER}" == "xterm" ]] || ! command -v xterm >/dev/null 2>&1; then
            exit 1
        fi
        echo "[run] falling back to xterm as the headless launcher"
        DISPLAY="${DISPLAY}" xterm \
            -display "${DISPLAY}" \
            -title "vibe_rdesk" \
            -fa "${XTERM_FONT_FAMILY}" \
            -fs "${XTERM_FONT_SIZE}" \
            -geometry 120x30+40+40 \
            >>"${launcher_log}" 2>&1 &
        LAUNCHER_PID=$!
        wait_for_process "${LAUNCHER_PID}" 4 || {
            echo "[run] fallback xterm exited during startup; see ${launcher_log}" >&2
            exit 1
        }
    fi

    if [[ "${HEADLESS_LAUNCHER}" != "xterm" && "${VIBE_RDESK_START_XTERM:-1}" != "0" ]] && command -v xterm >/dev/null 2>&1; then
        DISPLAY="${DISPLAY}" xterm \
            -display "${DISPLAY}" \
            -title "vibe_rdesk" \
            -fa "${XTERM_FONT_FAMILY}" \
            -fs "${XTERM_FONT_SIZE}" \
            -geometry 120x30+40+40 \
            >>"${launcher_log}" 2>&1 &
        XTERM_PID=$!
        wait_for_process "${XTERM_PID}" 4 || true
    fi
}

ensure_display() {
    local start_display
    start_display="$(normalize_display "${HEADLESS_DISPLAY_RAW}")"

    if [[ "${FORCE_HEADLESS_DISPLAY}" == "yes" ]]; then
        echo "[run] force-starting headless X11 at first free display from ${start_display}"
        start_headless_display
        return 0
    fi

    if [[ -n "${DISPLAY:-}" ]]; then
        if is_display_available "${DISPLAY}"; then
            echo "[run] using existing X server on ${DISPLAY}"
            return 0
        fi

        echo "[run] DISPLAY=${DISPLAY} is set but unavailable or missing XTEST; starting headless X11 at first free display from ${start_display}"
    else
        echo "[run] DISPLAY is not set; starting headless X11 at first free display from ${start_display}"
    fi

    start_headless_display
}

cleanup() {
    if [[ -n "${APP_PID}" ]] && kill -0 "${APP_PID}" 2>/dev/null; then
        kill "${APP_PID}" 2>/dev/null || true
        wait "${APP_PID}" 2>/dev/null || true
    fi

    if [[ -n "${LAUNCHER_PID}" ]] && kill -0 "${LAUNCHER_PID}" 2>/dev/null; then
        kill "${LAUNCHER_PID}" 2>/dev/null || true
        wait "${LAUNCHER_PID}" 2>/dev/null || true
    fi

    if [[ -n "${XTERM_PID}" ]] && kill -0 "${XTERM_PID}" 2>/dev/null; then
        kill "${XTERM_PID}" 2>/dev/null || true
        wait "${XTERM_PID}" 2>/dev/null || true
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

    if [[ -n "${DBUS_PID}" ]] && kill -0 "${DBUS_PID}" 2>/dev/null; then
        kill "${DBUS_PID}" 2>/dev/null || true
        wait "${DBUS_PID}" 2>/dev/null || true
    fi
}

run_server() {
    if [[ ! -x "${APP_BINARY}" ]]; then
        echo "[run] ${APP_BINARY} is missing or not executable. Build it first with ./build.sh." >&2
        exit 1
    fi

    echo "[run] starting app..."
    "${APP_BINARY}" "$@" &
    APP_PID=$!
    show_server_access_urls "${SERVER_PROTOCOL}"
    wait "${APP_PID}"
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

parse_script_args "$@"
require_passwd_arg "${APP_ARGS[@]}"
configure_bind_args "${APP_ARGS[@]}"
if [[ "${HTTPS_ENABLED}" == "yes" ]]; then
    SERVER_PROTOCOL="https"
else
    SERVER_PROTOCOL="http"
    unset VIBE_RDESK_TLS_CERT
    unset VIBE_RDESK_TLS_KEY
fi
show_startup_help "${SERVER_PROTOCOL}"
sleep "${STARTUP_HELP_SECONDS}"
if [[ "${HTTPS_ENABLED}" == "yes" ]]; then
    ensure_tls_keys
    export VIBE_RDESK_TLS_CERT="${SSL_CERT}"
    export VIBE_RDESK_TLS_KEY="${SSL_KEY}"
fi
configure_sudo_audio_env
ensure_dev_log_dir
configure_appimage_desktop_env
prepare_gdk_pixbuf_cache
start_private_session_bus
ensure_uinput_access
ensure_display
start_audio_server
ensure_virtual_mic

run_server "${APP_ARGS[@]}"
