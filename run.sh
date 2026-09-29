#!/bin/sh
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
if [ -x "$ROOT/aurora" ]; then exec "$ROOT/aurora" "$@"; fi
if [ -x "$ROOT/.output/dist/current/aurora" ]; then exec "$ROOT/.output/dist/current/aurora" "$@"; fi
printf '%s\n' 'Build once with ./build-dist.sh, or extract the release and run ./aurora --passwd-file /private/password.' >&2
exit 1
