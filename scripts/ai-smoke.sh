#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
python3 scripts/check-ai.py
cargo build -p bracel-cli --locked
binary="${CARGO_TARGET_DIR:-target}/debug/bracel"
python3 scripts/ai-e2e.py --bin "$binary" --artifact "${CARGO_TARGET_DIR:-target}/ai-e2e.json"
"$binary" ai sync --check
