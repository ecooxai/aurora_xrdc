#!/usr/bin/env bash
set -euo pipefail

ARCH="$(uname -m)"

# Prefer a rustup-managed toolchain when available. Under sudo, use the
# invoking user's rustup path because root often does not have cargo installed.
CARGO_ENV_HOME="${HOME}"
if [[ "$(id -u)" -eq 0 && -n "${SUDO_UID:-}" && "${SUDO_UID}" != "0" ]]; then
    SUDO_PASSWD_ENTRY="$(getent passwd "${SUDO_UID}" 2>/dev/null || true)"
    if [[ -n "${SUDO_PASSWD_ENTRY}" ]]; then
        IFS=: read -r _ _ _ _ _ SUDO_HOME_FOR_CARGO _ <<<"${SUDO_PASSWD_ENTRY}"
        if [[ -n "${SUDO_HOME_FOR_CARGO}" ]]; then
            CARGO_ENV_HOME="${SUDO_HOME_FOR_CARGO}"
        fi
    fi
fi
if [[ -f "${CARGO_ENV_HOME}/.cargo/env" ]]; then
    if [[ -d "${CARGO_ENV_HOME}/.cargo" ]]; then
        export CARGO_HOME="${CARGO_HOME:-${CARGO_ENV_HOME}/.cargo}"
    fi
    if [[ -d "${CARGO_ENV_HOME}/.rustup" ]]; then
        export RUSTUP_HOME="${RUSTUP_HOME:-${CARGO_ENV_HOME}/.rustup}"
    fi
    # shellcheck disable=SC1090
    SAVED_HOME_FOR_CARGO="${HOME}"
    export HOME="${CARGO_ENV_HOME}"
    . "${CARGO_ENV_HOME}/.cargo/env"
    export HOME="${SAVED_HOME_FOR_CARGO}"
fi

echo "[build] building optimized release binary for ${ARCH}..."
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS="${CARGO_PROFILE_RELEASE_CODEGEN_UNITS:-1}"
export CARGO_PROFILE_RELEASE_INCREMENTAL="${CARGO_PROFILE_RELEASE_INCREMENTAL:-false}"
export CARGO_PROFILE_RELEASE_LTO="${CARGO_PROFILE_RELEASE_LTO:-thin}"
export CARGO_PROFILE_RELEASE_PANIC="${CARGO_PROFILE_RELEASE_PANIC:-abort}"
if [[ "${VIBE_RDESK_TARGET_CPU_NATIVE:-1}" == "1" ]]; then
    export RUSTFLAGS="${RUSTFLAGS:-} -C target-cpu=native"
fi
cargo build --release
