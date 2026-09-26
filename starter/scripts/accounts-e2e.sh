#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
: "${TEST_DATABASE_URL:?Set TEST_DATABASE_URL to a disposable PostgreSQL database}"
cargo build --locked --bin bracel-starter
python3 scripts/accounts-e2e.py
