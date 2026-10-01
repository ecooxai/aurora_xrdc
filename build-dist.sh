#!/bin/sh
# Build-time tools: Rust, a musl C toolchain, and Python 3. Target runtime packages: none.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cd "$ROOT"
case "$(uname -m)" in
    x86_64) TARGET=x86_64-unknown-linux-musl ;;
    *) echo 'This release supports Linux x86_64. Cross-architecture binaries must be built/tested separately.' >&2; exit 1 ;;
esac
rustup target add "$TARGET"

compiler_works() {
    [ -n "$1" ] || return 1
    command -v "$1" >/dev/null 2>&1 || [ -x "$1" ] || return 1
    "$1" -print-file-name=libc.a >/dev/null 2>&1
}

CC=${CC_x86_64_unknown_linux_musl:-}
if [ -z "$CC" ]; then
    if compiler_works musl-gcc; then
        CC=musl-gcc
    else
        # Colab persistent tool installs keep musl under ~/.local/sysroot while /usr is ephemeral.
        SYSROOT=${AURORA_MUSL_SYSROOT:-"$HOME/.local/sysroot"}
        SPECS="$SYSROOT/usr/lib/x86_64-linux-musl/musl-gcc.specs"
        if [ -f "$SPECS" ] && command -v x86_64-linux-gnu-gcc >/dev/null 2>&1; then
            TOOL_DIR="$ROOT/.agentwork/musl-toolchain"
            mkdir -p "$TOOL_DIR"
            python3 - "$SPECS" "$TOOL_DIR/musl-gcc.specs" "$SYSROOT" <<'PY'
from pathlib import Path
import sys
src,dst,root=Path(sys.argv[1]),Path(sys.argv[2]),Path(sys.argv[3]).resolve()
s=src.read_text().replace('/usr/include/x86_64-linux-musl',str(root/'usr/include/x86_64-linux-musl')).replace('/usr/lib/x86_64-linux-musl',str(root/'usr/lib/x86_64-linux-musl'))
dst.write_text(s)
PY
            cat > "$TOOL_DIR/musl-gcc" <<EOF
#!/bin/sh
exec x86_64-linux-gnu-gcc "\$@" -specs "$TOOL_DIR/musl-gcc.specs"
EOF
            chmod +x "$TOOL_DIR/musl-gcc"
            CC="$TOOL_DIR/musl-gcc"
            echo "[build] using persistent musl sysroot: $SYSROOT" >&2
        fi
    fi
fi
if ! compiler_works "$CC"; then
    echo 'Install a musl C toolchain on the BUILD host, set CC_x86_64_unknown_linux_musl, or set AURORA_MUSL_SYSROOT.' >&2
    exit 1
fi
export CC_x86_64_unknown_linux_musl="$CC"
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER="$CC"
export RUSTFLAGS='-C target-cpu=x86-64'
TARGET_DIR=${CARGO_TARGET_DIR:-"$ROOT/.agentwork/cargo-portable"}
cargo build --locked --release --target "$TARGET" --target-dir "$TARGET_DIR"
python3 tools/portable/package.py --binaries "$TARGET_DIR/$TARGET/release" "$@"
