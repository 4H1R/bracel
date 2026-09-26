#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
: "${TEST_DATABASE_URL:?Set TEST_DATABASE_URL to a disposable PostgreSQL database}"
cargo build -p bracel-starter --all-features --locked
for scenario in mutations concurrency realtime websocket delivery outbound files collections jobs tenants operations middleware identity; do
    python3 scripts/api-e2e.py --scenario "$scenario"
done
