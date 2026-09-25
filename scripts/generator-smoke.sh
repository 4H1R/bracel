#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
test -n "$TEST_DATABASE_URL"
export CARGO_TARGET_DIR=$(realpath "$(printenv CARGO_TARGET_DIR || echo target)")
cargo build -p bracel-cli --locked
python3 scripts/test_generated_application.py
