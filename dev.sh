#!/usr/bin/env bash
set -euo pipefail

./test.sh
cargo run -- "$@"

