#!/bin/sh
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
"$ROOT/build-dist.sh"
exec "$ROOT/run.sh" --port 9990 "$@"
