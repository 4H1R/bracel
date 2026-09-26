#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
: "${TEST_DATABASE_URL:?Set TEST_DATABASE_URL to a disposable PostgreSQL database}"
python3 scripts/test_export_starter.py
bash scripts/ai-smoke.sh
cargo fmt --all -- --check
cargo clippy --workspace --locked --all-targets --all-features -- -D warnings
cargo test --workspace --locked --all-targets --all-features
cargo test -p bracel-starter --locked --test architecture
cargo test -p bracel-starter --locked --no-default-features --test architecture
cargo check -p bracel-delivery --locked --no-default-features
for feature in mail webhooks realtime; do
    cargo check -p bracel-delivery --locked --no-default-features --features "$feature"
done
for feature in http mail storage cache telemetry identity; do
    cargo check -p bracel-integrations --locked --no-default-features --features "$feature"
done
cargo check -p bracel --locked --no-default-features
BRACEL_OPTIONAL_CONTRACTS=true bash starter/scripts/openapi.sh check
cargo deny --locked --all-features check
cargo build --workspace --locked --release
bash scripts/package-smoke.sh
bash scripts/generator-smoke.sh
bash scripts/api-e2e.sh
bash starter/scripts/accounts-e2e.sh
