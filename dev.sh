#!/bin/sh
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd); DEFAULT_PORT=${AURORA_DEV_PORT:-9990}
case "$DEFAULT_PORT" in ''|*[!0-9]*) echo "dev.sh: invalid AURORA_DEV_PORT: $DEFAULT_PORT" >&2; exit 2;; esac
[ "$DEFAULT_PORT" -ge 1 ] && [ "$DEFAULT_PORT" -le 65535 ] || { echo 'dev.sh: AURORA_DEV_PORT must be 1..65535' >&2; exit 2; }
has_port=0; for arg in "$@"; do case "$arg" in -p|--port|-p=*|--port=*) has_port=1; break;; esac; done
if [ "${AURORA_DEV_SKIP_BUILD:-0}" != 1 ]; then echo '[dev] building portable release...' >&2; "$ROOT/build-dist.sh"; fi
if [ "$has_port" -eq 1 ]; then exec "$ROOT/run.sh" "$@"; else exec "$ROOT/run.sh" --port "$DEFAULT_PORT" "$@"; fi
