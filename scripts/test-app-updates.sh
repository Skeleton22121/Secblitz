#!/usr/bin/env bash
# Host tests, all-target compile checks, and non-test production probes. Never
# runs a Windows binary, VM, installer or CLI upgrade. Run from repository root.
set -euo pipefail
test -f src/app_updates.rs
if test -f target/build-tools/cross-env.sh; then
    source target/build-tools/cross-env.sh
fi
# Keep every generated build artifact outside release/production target trees.
test -d /tmp/opencode
export CARGO_TARGET_DIR=/tmp/opencode/app-updates-build
python3 scripts/test-app-updates-production.py --unit-tests
python3 scripts/test-app-updates-production.py --unit-tests --target x86_64-pc-windows-gnu
python3 scripts/test-app-updates-production.py
python3 scripts/test-app-updates-production.py --target x86_64-pc-windows-gnu
# Report BOTH integration targets even if an unrelated coordinator-owned target
# is temporarily broken. Such failures still make this script fail, never pass.
status=0
cargo check --locked --all-targets || status=1
cargo check --locked --all-targets --target x86_64-pc-windows-gnu || status=1
exit "$status"
