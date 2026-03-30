#!/usr/bin/env bash
set -euo pipefail

# Prefer a rustup-managed toolchain when available in the current workspace.
if [[ -f "${HOME}/.cargo/env" ]]; then
    # shellcheck disable=SC1090
    . "${HOME}/.cargo/env"
fi

cargo test
cargo build
