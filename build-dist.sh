#!/bin/sh
# Build-time tools: Rust, musl C toolchain, Python3. Runtime package installation: none.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cd "$ROOT"
case "$(uname -m)" in x86_64) TARGET=x86_64-unknown-linux-musl;; *) echo 'This release supports Linux x86_64. Cross-architecture binaries must be built and tested separately.' >&2; exit 1;; esac
rustup target add "$TARGET"
CC=${CC_x86_64_unknown_linux_musl:-musl-gcc}
if ! "$CC" -print-file-name=libc.a >/dev/null 2>&1; then
    echo 'Install a musl C toolchain on the BUILD host (not the target server), or set CC_x86_64_unknown_linux_musl.' >&2
    exit 1
fi
export CC_x86_64_unknown_linux_musl="$CC"
export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER="$CC"
# A portable release must not inherit target-cpu=native.
export RUSTFLAGS='-C target-cpu=x86-64'
TARGET_DIR=${CARGO_TARGET_DIR:-"$ROOT/.agentwork/cargo-portable"}
cargo build --locked --release --target "$TARGET" --target-dir "$TARGET_DIR"
python3 tools/portable/package.py --binaries "$TARGET_DIR/$TARGET/release" "$@"
