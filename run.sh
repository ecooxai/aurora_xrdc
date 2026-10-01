#!/bin/sh
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
find_launcher() {
    if [ -n "${AURORA_LAUNCHER_BIN:-}" ]; then [ -x "$AURORA_LAUNCHER_BIN" ] || { echo "run.sh: AURORA_LAUNCHER_BIN is not executable: $AURORA_LAUNCHER_BIN" >&2; return 1; }; echo "$AURORA_LAUNCHER_BIN";
    elif [ -x "$ROOT/aurora" ]; then echo "$ROOT/aurora";
    elif [ -x "$ROOT/.output/dist/current/aurora" ]; then echo "$ROOT/.output/dist/current/aurora";
    else echo 'run.sh: no Aurora launcher found. Run ./build-dist.sh once, or use the extracted release.' >&2; return 1; fi
}
scan_port() { port=18443; while [ "$#" -gt 0 ]; do case "$1" in -p|--port) [ "$#" -ge 2 ] || { echo "run.sh: $1 requires a value" >&2; return 2; }; port=$2; shift 2;; -p=*|--port=*) port=${1#*=}; shift;; *) shift;; esac; done; case "$port" in ''|*[!0-9]*) echo "run.sh: invalid port: $port" >&2; return 2;; esac; [ "$port" -ge 1 ] && [ "$port" -le 65535 ] || { echo "run.sh: port must be 1..65535: $port" >&2; return 2; }; echo "$port"; }
scan_headless() { mode=auto; while [ "$#" -gt 0 ]; do case "$1" in --headless=*) mode=${1#*=}; shift;; --headless) if [ "$#" -ge 2 ]; then case "$2" in yes|no|auto) mode=$2; shift 2; continue;; esac; fi; mode=yes; shift;; *) shift;; esac; done; case "$mode" in auto|yes|no);; *) echo "run.sh: invalid --headless mode: $mode" >&2; return 2;; esac; echo "$mode"; }
has_display_arg() { while [ "$#" -gt 0 ]; do case "$1" in --display|--display=*) return 0;; esac; shift; done; return 1; }
LAUNCHER=$(find_launcher); PORT=$(scan_port "$@"); HEADLESS=$(scan_headless "$@")
if [ "$HEADLESS" = yes ] && ! has_display_arg "$@"; then DISPLAY_NUMBER=$(( PORT % 1000 )); echo "[run] headless port $PORT -> private DISPLAY=:$DISPLAY_NUMBER" >&2; set -- "$@" --display ":$DISPLAY_NUMBER"; fi
exec "$LAUNCHER" "$@"
