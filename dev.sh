#!/bin/sh
# Build once, then exec the same launcher as production. No duplicate argument parser.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
case "${1:-}" in
    -h|--help)
        printf '%s\n' 'Usage: ./dev.sh [Aurora options]' \
            'Builds a portable release, then runs it. Default port: 9990.' \
            'AURORA_DEV_PORT overrides the default; an explicit --port wins.' \
            'AURORA_DEV_SKIP_BUILD=1 reuses an existing release.' ''
        exec "$ROOT/run.sh" --help ;;
    --version) exec "$ROOT/run.sh" --version ;;
esac
DEFAULT_PORT=${AURORA_DEV_PORT:-9990}
case "$DEFAULT_PORT" in
    ''|*[!0-9]*) printf '%s\n' 'dev.sh: AURORA_DEV_PORT must be decimal 1..65535' >&2; exit 2 ;;
esac
if ! [ "$DEFAULT_PORT" -ge 1 ] 2>/dev/null || ! [ "$DEFAULT_PORT" -le 65535 ] 2>/dev/null; then
    printf '%s\n' 'dev.sh: AURORA_DEV_PORT must be decimal 1..65535' >&2
    exit 2
fi
case "${AURORA_DEV_SKIP_BUILD:-0}" in
    0) printf '%s\n' '[dev] building portable release...' >&2; "$ROOT/build-dist.sh" ;;
    1) ;;
    *) printf '%s\n' 'dev.sh: AURORA_DEV_SKIP_BUILD must be 0 or 1' >&2; exit 2 ;;
esac
# Last CLI value wins in Rust, so user arguments override this default without shell re-parsing.
exec "$ROOT/run.sh" --port "$DEFAULT_PORT" "$@"
