#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -P "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ARCH="$(uname -m)"
DEFAULT_HEADLESS_LAUNCHER="${SCRIPT_DIR}/vendor/${ARCH}/aurora-wm"

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
HEADLESS_DISPLAY_RAW="${VIBE_RDESK_HEADLESS_DISPLAY:-${DISPLAY:-11}}"
XVFB_SCREEN="${VIBE_RDESK_XVFB_SCREEN:-1280x720x24}"
WINDOW_MANAGER_DELAY_SECONDS=2
XTERM_FONT_FAMILY="${VIBE_RDESK_XTERM_FONT_FAMILY:-Monospace}"
XTERM_FONT_SIZE="${VIBE_RDESK_XTERM_FONT_SIZE:-10}"
HEADLESS_LAUNCHER="${VIBE_RDESK_HEADLESS_LAUNCHER:-${DEFAULT_HEADLESS_LAUNCHER}}"
HEADLESS_INPUT_BACKEND="${VIBE_RDESK_HEADLESS_INPUT_BACKEND:-x11}"
FORCE_HEADLESS_DISPLAY="${VIBE_RDESK_FORCE_HEADLESS_DISPLAY:-no}"
NEW_DISPLAY_NOTICE_SECONDS="${VIBE_RDESK_NEW_DISPLAY_NOTICE_SECONDS:-3}"
STARTUP_HELP_SECONDS="${VIBE_RDESK_HELP_SECONDS:-3}"
VIRTUAL_MIC_SOURCE_NAME="${VIBE_RDESK_VIRTUAL_MIC_SOURCE_NAME:-Viberdeskmic}"
VIRTUAL_MIC_SINK_NAME="${VIBE_RDESK_VIRTUAL_MIC_SINK_NAME:-vibe_rdesk_virtual_mic_sink}"
VIRTUAL_AUDIO_SINK_NAME="${VIBE_RDESK_AUDIO_SINK:-}"
DEFAULT_BIND="${VIBE_RDESK_BIND:-0.0.0.0:8001,[::]:8001}"
export VIBE_RDESK_BIND="${DEFAULT_BIND}"
HTTPS_ENABLED="yes"
SSL_DIR="${VIBE_RDESK_SSL_DIR:-ssl_keys}"
SSL_CERT="${VIBE_RDESK_TLS_CERT:-${SSL_DIR}/server.crt}"
SSL_KEY="${VIBE_RDESK_TLS_KEY:-${SSL_DIR}/server.key}"
DEV_LOG_UID="${SUDO_UID:-$(id -u)}"
DEV_LOG_DIR="${VIBE_RDESK_LOG_DIR:-/tmp/vibe_rdesk-${DEV_LOG_UID}}"
AUDIO_BACKEND=""
SUDO_AUDIO_USER=""
SUDO_AUDIO_UID=""
SUDO_AUDIO_HOME=""
SUDO_AUDIO_RUNTIME_DIR=""
SESSION_RUNTIME_USER=""
SESSION_RUNTIME_UID=""
SESSION_RUNTIME_DIR=""
APP_PID=""
XVFB_PID=""
LAUNCHER_PID=""
PULSE_PID=""
PIPEWIRE_LOOPBACK_PID=""
DBUS_PID=""
HTTPS_PROXY_PID=""
HTTPS_PROXY_PORT=""

APP_ARGS=()

parse_script_args() {
    local args=("$@")
    local i
    APP_ARGS=()

    for ((i = 0; i < ${#args[@]}; i++)); do
        case "${args[$i]}" in
            --launcher)
                if (( i + 1 >= ${#args[@]} )) || [[ -z "${args[$((i + 1))]}" ]]; then
                    echo "[dev] --launcher requires a command. Example: ./dev.sh --launcher xterm --passwd <password>" >&2
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
            --https)
                if (( i + 1 < ${#args[@]} )) && [[ "${args[$((i + 1))]}" != --* ]]; then
                    if [[ "${args[$((i + 1))]}" == "no" ]]; then
                        HTTPS_ENABLED="no"
                    else
                        HTTPS_ENABLED="yes"
                    fi
                    ((i += 1))
                else
                    HTTPS_ENABLED="yes"
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
                    echo "[dev] --passwd requires a non-empty value. Start with: bash dev.sh --passwd passwd" >&2
                    exit 1
                fi
                return 0
                ;;
        esac
    done

    echo "[dev] missing required --passwd. Start with: bash dev.sh --passwd passwd" >&2
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
    printf '8001\n'
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
    local address_family="${4:-}"
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
            if [[ -n "${address_family}" && "${family}" != "${address_family}" ]]; then
                continue
            fi

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
    local localhost="no"
    local i value

    for ((i = 0; i < ${#args[@]}; i++)); do
        case "${args[$i]}" in
            -p|--port)
                if (( i + 1 >= ${#args[@]} )) || [[ ! "${args[$((i + 1))]}" =~ ^[0-9]+$ ]]; then
                    echo "[dev] -p/--port requires a numeric port value" >&2
                    exit 1
                fi
                port="${args[$((i + 1))]}"
                ;;
            --localhost)
                if (( i + 1 >= ${#args[@]} )); then
                    echo "[dev] --localhost requires yes or no" >&2
                    exit 1
                fi
                value="${args[$((i + 1))]}"
                case "${value}" in
                    yes|no) localhost="${value}" ;;
                    *)
                        echo "[dev] --localhost must be yes or no" >&2
                        exit 1
                        ;;
                esac
                ;;
        esac
    done

    if [[ -z "${port}" ]]; then
        port="$(first_bind_port "${VIBE_RDESK_BIND}")"
    fi

    if [[ "${localhost}" == "yes" ]]; then
        export VIBE_RDESK_BIND="127.0.0.1:${port},[::1]:${port}"
    elif [[ -n "${port}" ]]; then
        export VIBE_RDESK_BIND="0.0.0.0:${port},[::]:${port}"
    fi
}

show_startup_help() {
    local http_port
    http_port="$(first_bind_port "${VIBE_RDESK_BIND}")"

    local https_line="HTTPS proxy: enabled; first available port at or after $((http_port + 1))"
    local reminder="use https:// for TLS, or http:// for direct local HTTP."
    if [[ "${HTTPS_ENABLED}" == "no" ]]; then
        https_line="HTTPS proxy: disabled because --https no was specified"
        reminder="using normal http:// because --https no was specified."
    fi

    cat <<EOF
[dev] Development: bash dev.sh --passwd passwd --port ${http_port} --https yes
[dev] Starts an HTTP development server with optional self-signed HTTPS proxy.
[dev] Production:  bash run.sh --passwd passwd --port ${http_port} --https yes --headless no
[dev] starting vibe_rdesk HTTP development server
[dev] HTTP bind: ${VIBE_RDESK_BIND}
[dev] ${https_line}
[dev] ${reminder} Default is all IPv4/IPv6 interfaces; use --localhost yes for loopback only.
[dev] starting in ${STARTUP_HELP_SECONDS}s...
EOF
}

show_server_access_urls() {
    local http_port
    http_port="$(first_bind_port "${VIBE_RDESK_BIND}")"

    if ! wait_for_server_port "${http_port}" && [[ -n "${APP_PID}" ]] && ! kill -0 "${APP_PID}" 2>/dev/null; then
        return 1
    fi
    echo "[dev] HTTP server started on ${VIBE_RDESK_BIND}"
    show_access_urls "[dev]" "http" "${http_port}" "inet"

    if [[ "${HTTPS_ENABLED}" != "no" ]]; then
        start_https_proxy "${http_port}"
        show_access_urls "[dev]" "https" "${HTTPS_PROXY_PORT}" "inet"
    fi
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
        echo "[dev] using TLS certificate ${SSL_CERT}"
        return 0
    fi

    if [[ -f "${SSL_CERT}" || -f "${SSL_KEY}" ]]; then
        echo "[dev] TLS certificate/key pair is incomplete; expected both ${SSL_CERT} and ${SSL_KEY}" >&2
        exit 1
    fi

    if ! command -v openssl >/dev/null 2>&1; then
        echo "[dev] openssl is required to generate test TLS keys in ${SSL_DIR}" >&2
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

    echo "[dev] generating self-signed test TLS certificate in ${SSL_DIR}"
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

start_https_proxy() {
    local http_port="$1"
    local start_port="$((http_port + 1))"
    local bind_host="0.0.0.0"
    local first_bind

    first_bind="${VIBE_RDESK_BIND%%,*}"
    if [[ "${first_bind}" == 127.0.0.1:* || "${first_bind}" == "[::1]:"* ]]; then
        bind_host="127.0.0.1"
    fi

    if [[ -n "${HTTPS_PROXY_PID}" ]] && kill -0 "${HTTPS_PROXY_PID}" 2>/dev/null; then
        kill "${HTTPS_PROXY_PID}" 2>/dev/null || true
        wait "${HTTPS_PROXY_PID}" 2>/dev/null || true
    fi

    local proxy_ready="${DEV_LOG_DIR}/https-proxy.port"
    local proxy_log="${DEV_LOG_DIR}/https-proxy.log"
    rm -f "${proxy_ready}"

    python3 - "${bind_host}" "${start_port}" "127.0.0.1" "${http_port}" "${SSL_CERT}" "${SSL_KEY}" "${proxy_ready}" >"${proxy_log}" 2>&1 <<'PY' &
import os
import select
import socket
import ssl
import sys
import threading

bind_host, start_port, upstream_host, upstream_port, cert_path, key_path, ready_path = sys.argv[1:]
start_port = int(start_port)
upstream_port = int(upstream_port)

# Listen on IPv6 with a dual-stack socket so the proxy is reachable over both
# IPv4 and IPv6. For the wildcard bind, "::" with IPV6_V6ONLY=0 also accepts
# IPv4-mapped clients; for an explicit IPv4 host keep an IPv4-only socket.
if bind_host in ("0.0.0.0", "::", ""):
    family, listen_host, dual_stack = socket.AF_INET6, "::", True
else:
    family, listen_host, dual_stack = socket.AF_INET, bind_host, False

listener = None
port = start_port
while port <= 65535:
    sock = socket.socket(family, socket.SOCK_STREAM)
    sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    if dual_stack:
        try:
            sock.setsockopt(socket.IPPROTO_IPV6, socket.IPV6_V6ONLY, 0)
        except OSError:
            pass
    try:
        sock.bind((listen_host, port))
        sock.listen(128)
        listener = sock
        break
    except OSError:
        sock.close()
        port += 1

if listener is None:
    raise SystemExit(f"no free HTTPS proxy port found at or above {start_port}")

context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
context.load_cert_chain(cert_path, key_path)

with open(ready_path, "w", encoding="utf-8") as ready:
    ready.write(f"{port}\n")

def relay(client):
    upstream = None
    try:
        tls_client = context.wrap_socket(client, server_side=True)
        upstream = socket.create_connection((upstream_host, upstream_port))
        sockets = [tls_client, upstream]
        while True:
            readable, _, _ = select.select(sockets, [], [])
            for source in readable:
                data = source.recv(65536)
                if not data:
                    return
                target = upstream if source is tls_client else tls_client
                target.sendall(data)
    except Exception:
        return
    finally:
        for sock in (client, upstream):
            if sock is not None:
                try:
                    sock.close()
                except OSError:
                    pass

while True:
    client, _ = listener.accept()
    thread = threading.Thread(target=relay, args=(client,), daemon=True)
    thread.start()
PY
    HTTPS_PROXY_PID=$!

    local attempts=120
    while (( attempts > 0 )); do
        if [[ -s "${proxy_ready}" ]]; then
            HTTPS_PROXY_PORT="$(<"${proxy_ready}")"
            echo "[dev] HTTPS proxy started on ${bind_host}:${HTTPS_PROXY_PORT} -> 127.0.0.1:${http_port}"
            return 0
        fi
        if ! kill -0 "${HTTPS_PROXY_PID}" 2>/dev/null; then
            echo "[dev] HTTPS proxy failed to start; see ${proxy_log}" >&2
            exit 1
        fi
        sleep 0.25
        ((attempts--))
    done

    echo "[dev] timed out waiting for HTTPS proxy startup; see ${proxy_log}" >&2
    exit 1
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

display_has_window_manager() {
    local display="${1:-}"
    local wm_check=""
    if [[ -z "${display}" ]] || ! command -v xprop >/dev/null 2>&1; then
        return 1
    fi

    wm_check="$(DISPLAY="${display}" xprop -root _NET_SUPPORTING_WM_CHECK 2>/dev/null || true)"
    [[ "${wm_check}" == *"window id #"* && "${wm_check}" != *"not found"* ]]
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

configure_session_runtime_env() {
    local runtime_uid=""
    local runtime_user=""
    local runtime_home=""
    local passwd_entry=""

    if [[ "$(id -u)" -eq 0 && -n "${SUDO_UID:-}" && "${SUDO_UID}" != "0" ]]; then
        runtime_uid="${SUDO_UID}"
        passwd_entry="$(getent passwd "${runtime_uid}" 2>/dev/null || true)"
        if [[ -n "${passwd_entry}" ]]; then
            IFS=: read -r runtime_user _ _ _ _ runtime_home _ <<<"${passwd_entry}"
        fi
        runtime_user="${SUDO_USER:-${runtime_user}}"

        SUDO_AUDIO_UID="${runtime_uid}"
        SUDO_AUDIO_USER="${runtime_user}"
        SUDO_AUDIO_HOME="${runtime_home}"
        SUDO_AUDIO_RUNTIME_DIR="/run/user/${runtime_uid}"
    else
        runtime_uid="$(id -u)"
        runtime_user="$(id -un)"
        runtime_home="${HOME}"
    fi

    if [[ -z "${runtime_uid}" || -z "${runtime_user}" ]]; then
        echo "[dev] runtime: unable to resolve session user" >&2
        return 0
    fi

    SESSION_RUNTIME_UID="${runtime_uid}"
    SESSION_RUNTIME_USER="${runtime_user}"
    SESSION_RUNTIME_DIR="/run/user/${SESSION_RUNTIME_UID}"

    if [[ ! -d "${SESSION_RUNTIME_DIR}" ]]; then
        echo "[dev] runtime: creating ${SESSION_RUNTIME_DIR}"
        if [[ "$(id -u)" -eq 0 ]]; then
            mkdir -p "${SESSION_RUNTIME_DIR}" || true
        elif command -v sudo >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
            sudo mkdir -p "${SESSION_RUNTIME_DIR}" || true
        else
            mkdir -p "${SESSION_RUNTIME_DIR}" 2>/dev/null || true
        fi
    fi

    if [[ ! -d "${SESSION_RUNTIME_DIR}" ]]; then
        echo "[dev] runtime: ${SESSION_RUNTIME_DIR} is unavailable" >&2
        return 0
    fi

    if [[ "$(id -u)" -eq 0 ]]; then
        chown "${SESSION_RUNTIME_USER}:" "${SESSION_RUNTIME_DIR}" 2>/dev/null || true
        chmod 700 "${SESSION_RUNTIME_DIR}" 2>/dev/null || true
    elif command -v sudo >/dev/null 2>&1 && sudo -n true 2>/dev/null; then
        sudo chown "${SESSION_RUNTIME_USER}:" "${SESSION_RUNTIME_DIR}" 2>/dev/null || true
        sudo chmod 700 "${SESSION_RUNTIME_DIR}" 2>/dev/null || true
    else
        chmod 700 "${SESSION_RUNTIME_DIR}" 2>/dev/null || true
    fi

    export XDG_RUNTIME_DIR="${SESSION_RUNTIME_DIR}"
    unset PULSE_SERVER
    unset PULSE_COOKIE

    if [[ -S "${SESSION_RUNTIME_DIR}/bus" ]]; then
        export DBUS_SESSION_BUS_ADDRESS="unix:path=${SESSION_RUNTIME_DIR}/bus"
    fi

    echo "[dev] runtime: XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR}"
}

using_sudo_audio_user() {
    [[ "$(id -u)" -eq 0 && -n "${SUDO_AUDIO_USER}" && -n "${SUDO_AUDIO_UID}" && "${SUDO_AUDIO_UID}" != "0" ]]
}

command_needs_appimage_library_path() {
    [[ -n "${APPDIR:-}" ]] || return 1

    local arg base resolved
    for arg in "$@"; do
        [[ -n "${arg}" ]] || continue
        base="${arg##*/}"
        case "${base}" in
            Xvfb|dbus-daemon|dbus-launch|dbus-send|ip|modprobe|openssl|pactl|pipewire|pipewire-pulse|pulseaudio|pw-cli|pw-loopback|timeout|udevadm|wireplumber|wpctl|xdotool|xdpyinfo|xset|xterm)
                if [[ "${arg}" == */* ]]; then
                    resolved="${arg}"
                else
                    resolved="$(command -v "${arg}" 2>/dev/null || true)"
                fi
                if [[ -n "${resolved}" && "${resolved}" == "${APPDIR}/"* ]]; then
                    return 0
                fi
                ;;
        esac
    done

    return 1
}

run_as_audio_user() {
    local -a env_args=()
    local -a unset_env_args=("-u" "PULSE_SERVER" "-u" "PULSE_COOKIE")

    if [[ -n "${XDG_RUNTIME_DIR:-}" ]]; then
        env_args+=("XDG_RUNTIME_DIR=${XDG_RUNTIME_DIR}")
    fi
    if [[ -n "${DBUS_SESSION_BUS_ADDRESS:-}" ]]; then
        env_args+=("DBUS_SESSION_BUS_ADDRESS=${DBUS_SESSION_BUS_ADDRESS}")
    fi
    if [[ -n "${APPDIR:-}" ]] && ! command_needs_appimage_library_path "$@"; then
        if [[ -n "${VIBE_RDESK_HOST_LD_LIBRARY_PATH+x}" ]]; then
            env_args+=("LD_LIBRARY_PATH=${VIBE_RDESK_HOST_LD_LIBRARY_PATH}")
        else
            unset_env_args+=("-u" "LD_LIBRARY_PATH")
        fi
    elif [[ -n "${LD_LIBRARY_PATH:-}" ]]; then
        env_args+=("LD_LIBRARY_PATH=${LD_LIBRARY_PATH}")
    fi

    if using_sudo_audio_user && command -v runuser >/dev/null 2>&1; then
        runuser -u "${SUDO_AUDIO_USER}" -- env "${unset_env_args[@]}" "${env_args[@]}" "$@"
        return $?
    fi

    env "${unset_env_args[@]}" "${env_args[@]}" "$@"
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

    local dbus_dir="${XDG_RUNTIME_DIR:-${DEV_LOG_DIR}}/vibe-rdesk-dbus-$$"
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
        echo "[dev] private D-Bus session bus did not start; see ${DEV_LOG_DIR}/dbus.log" >&2
        return 0
    fi

    dbus_address="$(printf '%s\n' "${dbus_output}" | sed -n '1p')"
    dbus_pid="$(printf '%s\n' "${dbus_output}" | sed -n '2p')"
    if [[ -n "${dbus_address}" && "${dbus_pid}" =~ ^[0-9]+$ ]]; then
        export DBUS_SESSION_BUS_ADDRESS="${dbus_address}"
        DBUS_PID="${dbus_pid}"
        echo "[dev] started private D-Bus session bus"
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

log_to_file_and_terminal() {
    local log_file="$1"

    tee -a "${log_file}"
}

print_log_tail_to_console() {
    local prefix="$1"
    local log_file="$2"
    local lines="${3:-${VIBE_RDESK_XVFB_LOG_TAIL_LINES:-120}}"

    if [[ -s "${log_file}" ]]; then
        echo "${prefix} last ${lines} lines from ${log_file}:" >&2
        tail -n "${lines}" "${log_file}" >&2 || true
    else
        echo "${prefix} ${log_file} is empty; Xvfb did not write stdout/stderr" >&2
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

pulse_device_by_description() {
    local kind="$1"
    local description="$2"
    local command_kind name=""

    case "${kind}" in
        sinks|sources) command_kind="${kind}" ;;
        *) return 1 ;;
    esac

    name="$(run_as_audio_user timeout 1s pactl list "${command_kind}" 2>/dev/null | awk -v description="${description}" '
        /^[[:space:]]*Name:/ {
            current_name = $2
            next
        }
        /^[[:space:]]*device\.description = "/ {
            line = $0
            sub(/^[^\"]*"/, "", line)
            sub(/"$/, "", line)
            if (line == description && current_name != "") {
                print current_name
                exit
            }
        }
    ')"

    [[ -n "${name}" ]] || return 1
    printf '%s\n' "${name}"
}

project_virtual_audio_sink_name() {
    local sink_name="${VIBE_RDESK_AUDIO_SINK:-}"
    local display_name normalized
    if [[ -n "${sink_name//[[:space:]]/}" ]]; then
        printf '%s\n' "${sink_name}"
        return 0
    fi

    display_name="${DISPLAY:-$(normalize_display "${HEADLESS_DISPLAY_RAW}")}"
    normalized="${display_name//[:.]/_}"
    printf 'vibe_rdesk_%s\n' "${normalized}"
}

configure_virtual_audio_sink_env() {
    VIRTUAL_AUDIO_SINK_NAME="$(project_virtual_audio_sink_name)"
    export VIBE_RDESK_AUDIO_SINK="${VIRTUAL_AUDIO_SINK_NAME}"
}

wait_for_sink() {
    local sink_name="$1"
    local attempts=10

    while (( attempts > 0 )); do
        if [[ "${AUDIO_BACKEND}" == "pipewire" ]]; then
            if pipewire_sink_exists "${sink_name}"; then
                return 0
            fi
        else
            if sink_exists "${sink_name}"; then
                return 0
            fi
        fi
        sleep 0.25
        ((attempts--))
    done

    return 1
}

set_default_audio_sink() {
    local sink_name="$1"
    local node_id=""
    if command -v pactl >/dev/null 2>&1 && run_as_audio_user pactl set-default-sink "${sink_name}" >/dev/null 2>&1; then
        local sink_inputs input_id
        sink_inputs="$(run_as_audio_user pactl list short sink-inputs 2>/dev/null || true)"
        while read -r input_id _; do
            [[ -n "${input_id}" ]] || continue
            run_as_audio_user pactl move-sink-input "${input_id}" "${sink_name}" >/dev/null 2>&1 || true
        done <<<"${sink_inputs}"
        return 0
    fi

    if command -v wpctl >/dev/null 2>&1; then
        node_id="$(pipewire_node_id "${sink_name}" "Audio/Sink" 2>/dev/null || true)"
        if [[ -n "${node_id}" ]]; then
            run_as_audio_user wpctl set-default "${node_id}" >/dev/null 2>&1
            return $?
        fi
    fi

    return 1
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

pipewire_node_name_by_description() {
    local description="${1:-}"
    local media_class="${2:-}"
    local node_name=""
    if [[ -z "${description}" || -z "${media_class}" ]]; then
        return 1
    fi

    node_name="$(run_as_audio_user pw-cli ls Node 2>/dev/null | awk -v description="${description}" -v media_class="${media_class}" '
        function flush_block() {
            if (current_name != "" && current_desc == description && current_class == media_class) {
                print current_name
                found = 1
                exit
            }
        }
        /^[[:space:]]*id / {
            flush_block()
            current_name = ""
            current_desc = ""
            current_class = ""
            next
        }
        /node\.name = "/ {
            split($0, parts, "\"")
            current_name = parts[2]
            next
        }
        /node\.description = "/ {
            split($0, parts, "\"")
            current_desc = parts[2]
            next
        }
        /media\.class = "/ {
            split($0, parts, "\"")
            current_class = parts[2]
            next
        }
        END {
            if (!found && current_name != "" && current_desc == description && current_class == media_class) {
                print current_name
            }
        }
    ')"

    [[ -n "${node_name}" ]] || return 1
    printf '%s\n' "${node_name}"
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
    local pulse_dir="${XDG_RUNTIME_DIR:-${DEV_LOG_DIR}}/pulse"
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
    echo "[dev] starting private PulseAudio server with XDG runtime ${XDG_RUNTIME_DIR:-unset}"
    local saved_ld_library_path="${LD_LIBRARY_PATH-}"
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
        --load="module-null-sink sink_name=${VIRTUAL_AUDIO_SINK_NAME} sink_properties=device.description=VibeRDesk" \
        --load="module-null-sink sink_name=${VIRTUAL_MIC_SINK_NAME} sink_properties=device.description=VibeRDeskVirtualMicSink" \
        > >(log_to_file_and_terminal "${log_file}") 2>&1 &
    PULSE_PID=$!
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

    local log_file="${DEV_LOG_DIR}/pulseaudio.log"
    prepare_dev_log_file "${log_file}"
    echo "[dev] starting PulseAudio"
    run_as_audio_user pulseaudio --start --daemonize=yes --exit-idle-time=-1 > >(log_to_file_and_terminal "${log_file}") 2>&1 || true

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
    local existing_source existing_sink

    if pipewire_source_exists "${VIRTUAL_MIC_SOURCE_NAME}"; then
        echo "[dev] using existing PipeWire virtual mic source ${VIRTUAL_MIC_SOURCE_NAME}"
        return 0
    fi

    existing_source="$(pipewire_node_name_by_description "${VIRTUAL_MIC_SOURCE_NAME}" "Audio/Source" 2>/dev/null || true)"
    if [[ -n "${existing_source}" ]]; then
        VIRTUAL_MIC_SOURCE_NAME="${existing_source}"
        export VIBE_RDESK_VIRTUAL_MIC_SOURCE_NAME="${VIRTUAL_MIC_SOURCE_NAME}"
        echo "[dev] using existing PipeWire virtual mic source ${VIRTUAL_MIC_SOURCE_NAME}"
        return 0
    fi

    if ! pipewire_sink_exists "${VIRTUAL_MIC_SINK_NAME}"; then
        existing_sink="$(pipewire_node_name_by_description "VibeRDeskVirtualMicSink" "Audio/Sink" 2>/dev/null || true)"
        if [[ -n "${existing_sink}" ]]; then
            VIRTUAL_MIC_SINK_NAME="${existing_sink}"
            export VIBE_RDESK_VIRTUAL_MIC_SINK_NAME="${VIRTUAL_MIC_SINK_NAME}"
            echo "[dev] using existing PipeWire virtual mic sink ${VIRTUAL_MIC_SINK_NAME}"
        else
        echo "[dev] creating PipeWire virtual sink ${VIRTUAL_MIC_SINK_NAME}"
        run_as_audio_user pw-cli create-node adapter \
            "{ factory.name = support.null-audio-sink node.name = \"${VIRTUAL_MIC_SINK_NAME}\" node.description = \"VibeRDeskVirtualMicSink\" media.class = \"Audio/Sink\" object.linger = true audio.position = [ FL FR ] }" \
            >/dev/null
        if ! wait_for_pipewire_sink "${VIRTUAL_MIC_SINK_NAME}"; then
            echo "[dev] PipeWire virtual sink ${VIRTUAL_MIC_SINK_NAME} did not appear" >&2
            return 1
        fi
        fi
    fi

    echo "[dev] creating PipeWire virtual mic source ${VIRTUAL_MIC_SOURCE_NAME}"
    local log_file="${DEV_LOG_DIR}/pipewire-loopback.log"
    prepare_dev_log_file "${log_file}"
    run_as_audio_user pw-loopback \
        --name "${VIRTUAL_MIC_SOURCE_NAME}_loopback" \
        --capture-props "{ stream.capture.sink = true target.object = \"${VIRTUAL_MIC_SINK_NAME}\" node.passive = true node.dont-reconnect = true }" \
        --playback-props "{ node.name = \"${VIRTUAL_MIC_SOURCE_NAME}\" node.description = \"${VIRTUAL_MIC_SOURCE_NAME}\" media.class = \"Audio/Source\" audio.position = [ FL FR ] }" \
        > >(log_to_file_and_terminal "${log_file}") 2>&1 &
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

ensure_pipewire_virtual_audio_sink() {
    local existing_sink

    if ! pipewire_sink_exists "${VIRTUAL_AUDIO_SINK_NAME}"; then
        existing_sink="$(pipewire_node_name_by_description "VibeRDesk" "Audio/Sink" 2>/dev/null || true)"
        if [[ -n "${existing_sink}" ]]; then
            VIRTUAL_AUDIO_SINK_NAME="${existing_sink}"
            export VIBE_RDESK_AUDIO_SINK="${VIRTUAL_AUDIO_SINK_NAME}"
            echo "[dev] using existing PipeWire virtual audio output ${VIRTUAL_AUDIO_SINK_NAME}"
        else
        echo "[dev] creating PipeWire virtual audio output ${VIRTUAL_AUDIO_SINK_NAME}"
        run_as_audio_user pw-cli create-node adapter \
            "{ factory.name = support.null-audio-sink node.name = \"${VIRTUAL_AUDIO_SINK_NAME}\" node.description = \"VibeRDesk\" media.class = \"Audio/Sink\" object.linger = true audio.position = [ FL FR ] }" \
            >/dev/null
        if ! wait_for_pipewire_sink "${VIRTUAL_AUDIO_SINK_NAME}"; then
            echo "[dev] PipeWire virtual audio output ${VIRTUAL_AUDIO_SINK_NAME} did not appear" >&2
            return 1
        fi
        fi
    else
        echo "[dev] using existing PipeWire virtual audio output ${VIRTUAL_AUDIO_SINK_NAME}"
    fi

    if set_default_audio_sink "${VIRTUAL_AUDIO_SINK_NAME}"; then
        echo "[dev] default audio output: ${VIRTUAL_AUDIO_SINK_NAME}"
        return 0
    fi
    echo "[dev] failed to set default audio output to ${VIRTUAL_AUDIO_SINK_NAME}" >&2
    return 1
}

ensure_pulse_virtual_audio_sink() {
    local existing_sink

    if ! command -v pactl >/dev/null 2>&1; then
        echo "[dev] pactl is required to provision the virtual audio output" >&2
        return 1
    fi

    if ! sink_exists "${VIRTUAL_AUDIO_SINK_NAME}"; then
        existing_sink="$(pulse_device_by_description sinks "VibeRDesk" 2>/dev/null || true)"
        if [[ -n "${existing_sink}" ]]; then
            VIRTUAL_AUDIO_SINK_NAME="${existing_sink}"
            export VIBE_RDESK_AUDIO_SINK="${VIRTUAL_AUDIO_SINK_NAME}"
            echo "[dev] using existing virtual audio output ${VIRTUAL_AUDIO_SINK_NAME}"
        else
        echo "[dev] creating virtual audio output ${VIRTUAL_AUDIO_SINK_NAME}"
        run_as_audio_user pactl load-module \
            module-null-sink \
            "sink_name=${VIRTUAL_AUDIO_SINK_NAME}" \
            "sink_properties=device.description=VibeRDesk" \
            >/dev/null
        if ! wait_for_sink "${VIRTUAL_AUDIO_SINK_NAME}"; then
            echo "[dev] virtual audio output ${VIRTUAL_AUDIO_SINK_NAME} did not appear" >&2
            return 1
        fi
        fi
    else
        echo "[dev] using existing virtual audio output ${VIRTUAL_AUDIO_SINK_NAME}"
    fi

    if set_default_audio_sink "${VIRTUAL_AUDIO_SINK_NAME}"; then
        echo "[dev] default audio output: ${VIRTUAL_AUDIO_SINK_NAME}"
        return 0
    fi
    echo "[dev] failed to set default audio output to ${VIRTUAL_AUDIO_SINK_NAME}" >&2
    return 1
}

ensure_virtual_audio_sink() {
    if [[ -z "${VIRTUAL_AUDIO_SINK_NAME}" ]]; then
        configure_virtual_audio_sink_env
    fi
    if [[ "${AUDIO_BACKEND}" == "pipewire" ]]; then
        ensure_pipewire_virtual_audio_sink
        return $?
    fi

    ensure_pulse_virtual_audio_sink
}

ensure_pulse_virtual_mic() {
    local existing_source existing_sink

    if ! command -v pactl >/dev/null 2>&1; then
        echo "[dev] pactl is required to provision the virtual microphone" >&2
        return 1
    fi

    if source_exists "${VIRTUAL_MIC_SOURCE_NAME}"; then
        echo "[dev] using existing virtual mic source ${VIRTUAL_MIC_SOURCE_NAME}"
        return 0
    fi

    existing_source="$(pulse_device_by_description sources "${VIRTUAL_MIC_SOURCE_NAME}" 2>/dev/null || true)"
    if [[ -n "${existing_source}" ]]; then
        VIRTUAL_MIC_SOURCE_NAME="${existing_source}"
        export VIBE_RDESK_VIRTUAL_MIC_SOURCE_NAME="${VIRTUAL_MIC_SOURCE_NAME}"
        echo "[dev] using existing virtual mic source ${VIRTUAL_MIC_SOURCE_NAME}"
        return 0
    fi

    if ! sink_exists "${VIRTUAL_MIC_SINK_NAME}"; then
        existing_sink="$(pulse_device_by_description sinks "VibeRDeskVirtualMicSink" 2>/dev/null || true)"
        if [[ -n "${existing_sink}" ]]; then
            VIRTUAL_MIC_SINK_NAME="${existing_sink}"
            export VIBE_RDESK_VIRTUAL_MIC_SINK_NAME="${VIRTUAL_MIC_SINK_NAME}"
            echo "[dev] using existing virtual mic sink ${VIRTUAL_MIC_SINK_NAME}"
        else
        echo "[dev] creating virtual mic sink ${VIRTUAL_MIC_SINK_NAME}"
        run_as_audio_user pactl load-module \
            module-null-sink \
            "sink_name=${VIRTUAL_MIC_SINK_NAME}" \
            "sink_properties=device.description=VibeRDeskVirtualMicSink" \
            >/dev/null
        fi
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

start_launcher() {
    local launcher_program="${HEADLESS_LAUNCHER%%[[:space:]]*}"
    local launcher_name="${launcher_program##*/}"
    if [[ -z "${launcher_program}" ]] || ! command -v "${launcher_program}" >/dev/null 2>&1; then
        echo "[dev] launcher '${HEADLESS_LAUNCHER}' is not available. Install it first or use --launcher <command>." >&2
        exit 1
    fi

    local launcher_log="${DEV_LOG_DIR}/launcher.log"

    echo "[dev] start launcher ${launcher_name}"
    prepare_dev_log_file "${launcher_log}"
    sleep 1
    if [[ "${HEADLESS_LAUNCHER}" == "xterm" ]]; then
        DISPLAY="${DISPLAY}" XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-}" DBUS_SESSION_BUS_ADDRESS="${DBUS_SESSION_BUS_ADDRESS:-}" xterm \
            -display "${DISPLAY}" \
            -title "vibe_rdesk" \
            -fa "${XTERM_FONT_FAMILY}" \
            -fs "${XTERM_FONT_SIZE}" \
            -geometry 120x30+40+40 \
            > >(log_to_file_and_terminal "${launcher_log}") 2>&1 &
    else
        DISPLAY="${DISPLAY}" XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-}" DBUS_SESSION_BUS_ADDRESS="${DBUS_SESSION_BUS_ADDRESS:-}" bash -lc "exec ${HEADLESS_LAUNCHER}" > >(log_to_file_and_terminal "${launcher_log}") 2>&1 &
    fi
    LAUNCHER_PID=$!

    if ! wait_for_process "${LAUNCHER_PID}" 4; then
        echo "[dev] launcher '${HEADLESS_LAUNCHER}' exited during startup; see ${launcher_log}" >&2
        exit 1
    fi
    sleep 2
    echo "[dev] launcher started"
}

start_headless_display() {
    export DISPLAY
    if ! DISPLAY="$(find_free_headless_display "${HEADLESS_DISPLAY_RAW}")"; then
        echo "[dev] could not find a free X11 display starting at $(normalize_display "${HEADLESS_DISPLAY_RAW}")" >&2
        exit 1
    fi
    export VIBE_RDESK_INPUT_BACKEND="${VIBE_RDESK_INPUT_BACKEND:-${HEADLESS_INPUT_BACKEND}}"

    if ! command -v Xvfb >/dev/null 2>&1; then
        echo "[dev] Xvfb is required for headless startup. Install it first." >&2
        exit 1
    fi

    local xvfb_log="${DEV_LOG_DIR}/xvfb.log"

    echo "[dev] starting Xvfb on ${DISPLAY}"
    prepare_dev_log_file "${xvfb_log}"
    echo "[dev] Xvfb ${DISPLAY} -screen 0 ${XVFB_SCREEN} -ac -nolisten tcp" > >(log_to_file_and_terminal "${xvfb_log}") 2>&1
    Xvfb "${DISPLAY}" -screen 0 "${XVFB_SCREEN}" -ac -nolisten tcp > >(log_to_file_and_terminal "${xvfb_log}") 2>&1 &
    XVFB_PID=$!

    sleep 0.5
    if ! kill -0 "${XVFB_PID}" 2>/dev/null; then
        echo "[dev] Xvfb exited during startup; see ${xvfb_log}" >&2
        print_log_tail_to_console "[dev]" "${xvfb_log}"
        exit 1
    fi

    if ! wait_for_display "${DISPLAY}"; then
        echo "[dev] Xvfb on ${DISPLAY} did not become ready; see ${xvfb_log}" >&2
        print_log_tail_to_console "[dev]" "${xvfb_log}"
        exit 1
    fi

    echo "[dev] created headless X11 display ${DISPLAY}"
    sleep "${NEW_DISPLAY_NOTICE_SECONDS}"

    sleep "${WINDOW_MANAGER_DELAY_SECONDS}"
    start_launcher
}

ensure_display() {
    local start_display
    start_display="$(normalize_display "${HEADLESS_DISPLAY_RAW}")"

    if [[ "${FORCE_HEADLESS_DISPLAY}" == "yes" ]]; then
        echo "[dev] force-starting headless X11 at first free display from ${start_display}"
        start_headless_display
        return 0
    fi

    if [[ -n "${DISPLAY:-}" ]]; then
        if is_display_available "${DISPLAY}"; then
            echo "[dev] using existing X server on ${DISPLAY}"
            if display_has_window_manager "${DISPLAY}"; then
                echo "[dev] window manager already running on ${DISPLAY}"
            else
                echo "[dev] no window manager detected on ${DISPLAY}"
                start_launcher
            fi
            return 0
        fi

        echo "[dev] DISPLAY=${DISPLAY} is set but unavailable or missing XTEST; starting headless X11 at first free display from ${start_display}"
    else
        echo "[dev] DISPLAY is not set; starting headless X11 at first free display from ${start_display}"
    fi

    start_headless_display
}

cleanup() {
    if [[ -n "${HTTPS_PROXY_PID}" ]] && kill -0 "${HTTPS_PROXY_PID}" 2>/dev/null; then
        kill "${HTTPS_PROXY_PID}" 2>/dev/null || true
        wait "${HTTPS_PROXY_PID}" 2>/dev/null || true
    fi

    if [[ -n "${APP_PID}" ]] && kill -0 "${APP_PID}" 2>/dev/null; then
        kill "${APP_PID}" 2>/dev/null || true
        wait "${APP_PID}" 2>/dev/null || true
    fi

    if [[ -n "${LAUNCHER_PID}" ]] && kill -0 "${LAUNCHER_PID}" 2>/dev/null; then
        kill "${LAUNCHER_PID}" 2>/dev/null || true
        wait "${LAUNCHER_PID}" 2>/dev/null || true
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
    if [[ -n "${HTTPS_PROXY_PID}" ]] && kill -0 "${HTTPS_PROXY_PID}" 2>/dev/null; then
        echo "[dev] stopping HTTPS proxy..."
        kill "${HTTPS_PROXY_PID}" 2>/dev/null || true
        wait "${HTTPS_PROXY_PID}" 2>/dev/null || true
    fi

    echo "[dev] starting app..."
    target/debug/vibe_rdesk "$@" &
    APP_PID=$!
    show_server_access_urls
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
show_startup_help
sleep "${STARTUP_HELP_SECONDS}"
unset VIBE_RDESK_TLS_CERT
unset VIBE_RDESK_TLS_KEY
if [[ "${HTTPS_ENABLED}" != "no" ]]; then
    ensure_tls_keys
fi
configure_session_runtime_env
ensure_dev_log_dir
start_private_session_bus
ensure_uinput_access
ensure_display
configure_virtual_audio_sink_env
start_audio_server
ensure_virtual_audio_sink
ensure_virtual_mic

build_and_run "${APP_ARGS[@]}"

while true; do
    if command -v inotifywait >/dev/null 2>&1; then
        inotifywait -qq -r -e modify -e create -e delete -e move "${WATCH_DIRS[@]}"
    else
        wait_for_change_polling
    fi

    echo "[dev] change detected, waiting ${DEBOUNCE_SECONDS}s before rebuild..."
    sleep "${DEBOUNCE_SECONDS}"
    build_and_run "${APP_ARGS[@]}"
done
