#!/bin/sh
set -eu
cd "$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
cargo test --locked
node --check web/app.js
node --test tests/*.test.mjs
sh tests/launch_scripts_test.sh
python3 tests/package_audit_test.py
