#!/bin/sh
# No build or downloads here. Rust is the single source of CLI validation and display selection.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

case "${1:-}" in
    -h|--help)
        cat <<'HELP'
Usage: ./run.sh [Aurora options]

  --port PORT, -p PORT     Server port (default 18443; decimal 1..65535)
  --headless [yes|no|auto] Prefer host display in auto mode (default auto)
  --display :N            Override private display or select an existing host display
  --passwd-file PATH      Read the server password from a private file
  --passwd TEXT           Alternative password argument; prefer a file in production
  --https yes|no          Native HTTPS (default yes)
  --localhost yes|no      Restrict TCP listeners to loopback (default no)
  --audio auto|yes|no     Reuse host audio, force private audio, or skip setup
  --dbus auto|yes|no      Reuse host D-Bus, force private bus, or skip setup
  --state-dir PATH        Private, user-owned runtime parent (mode 700)
  --launcher none|COMMAND Override the private desktop

Private display = port % 1000: port 11220 -> :220. --display overrides this.
A busy display causes an error; no host display or lock file is removed.
Arguments (including --key=value and option-looking passwords) are forwarded unchanged.
AURORA_LAUNCHER_BIN can select an executable. No binaries? Run ./build-dist.sh once.
HELP
        exit 0 ;;
esac

if [ -n "${AURORA_LAUNCHER_BIN:-}" ]; then
    LAUNCHER=$AURORA_LAUNCHER_BIN
elif [ -f "$ROOT/aurora" ] && [ -x "$ROOT/aurora" ]; then
    LAUNCHER=$ROOT/aurora
else
    LAUNCHER=$ROOT/.output/dist/current/aurora
fi
if [ ! -f "$LAUNCHER" ] || [ ! -x "$LAUNCHER" ]; then
    printf '%s\n' "run.sh: launcher is missing or not executable: $LAUNCHER" \
        'Build once with ./build-dist.sh, or run from the extracted release.' >&2
    exit 1
fi
exec "$LAUNCHER" "$@"
