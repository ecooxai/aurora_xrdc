#!/bin/sh
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
TMP=$(mktemp -d); trap 'rm -rf "$TMP"' EXIT INT TERM
cp "$ROOT/run.sh" "$ROOT/dev.sh" "$TMP/"
printf '#!/bin/sh\nprintf "%%s\\n" "$@"\n' > "$TMP/aurora"; chmod +x "$TMP/aurora" "$TMP/run.sh" "$TMP/dev.sh"
out=$("$TMP/run.sh" --port 11220 --headless yes --passwd test 2>"$TMP/err")
printf '%s\n' "$out" | grep -Fx -- ':220' >/dev/null
grep -F '[run] headless port 11220 -> private DISPLAY=:220' "$TMP/err" >/dev/null
out=$("$TMP/run.sh" --port=12005 --headless=yes --passwd=test 2>"$TMP/err"); printf '%s\n' "$out" | grep -Fx -- ':5' >/dev/null
out=$("$TMP/run.sh" --port 11220 --headless yes --display :333 --passwd test 2>"$TMP/err"); [ "$(printf '%s\n' "$out" | grep -cFx -- '--display')" -eq 1 ]; printf '%s\n' "$out" | grep -Fx -- ':333' >/dev/null; ! grep -F 'private DISPLAY=' "$TMP/err" >/dev/null
out=$(AURORA_DEV_SKIP_BUILD=1 AURORA_DEV_PORT=11220 "$TMP/dev.sh" --headless yes --passwd test 2>"$TMP/err"); printf '%s\n' "$out" | grep -Fx -- '11220' >/dev/null; printf '%s\n' "$out" | grep -Fx -- ':220' >/dev/null
if "$TMP/run.sh" --port broken --headless yes --passwd test >"$TMP/out" 2>"$TMP/err"; then echo 'invalid port unexpectedly succeeded' >&2; exit 1; fi
grep -F 'invalid port' "$TMP/err" >/dev/null
echo 'launch script tests: ok'
