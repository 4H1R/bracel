#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -p bracel-cli --locked
binary="${CARGO_TARGET_DIR:-target}/debug/bracel"
python3 scripts/lifecycle-e2e.py --bin "$binary" --artifact "${CARGO_TARGET_DIR:-target}/lifecycle-e2e.json"
python3 scripts/installer-e2e.py --bin "$binary" --artifact "${CARGO_TARGET_DIR:-target}/installer-e2e.json"
