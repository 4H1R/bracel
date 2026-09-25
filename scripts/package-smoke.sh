#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
root=$PWD
version=$(python3 -c 'import tomllib; print(tomllib.load(open("crates/bracel/Cargo.toml", "rb"))["package"]["version"])')
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-"$root/target"}
cargo package -p bracel --allow-dirty --no-verify --locked
cargo package -p bracel-cli --allow-dirty --locked
cargo package -p bracel-integrations --allow-dirty --locked --all-features
consumer=$(mktemp -d)
trap 'rm -rf "$consumer"' EXIT
tar -xzf "$CARGO_TARGET_DIR/package/bracel-$version.crate" -C "$consumer"
mkdir -p "$consumer/app/src" "$consumer/app/tests"
cat > "$consumer/app/Cargo.toml" <<TOML
[package]
name = "bracel-consumer-check"
version = "0.0.0"
edition = "2024"

[dependencies]
bracel = { path = "../bracel-$version" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
tower = { version = "0.5", features = ["util"] }
serde_json = "1"
TOML
printf 'fn main() {}\n' > "$consumer/app/src/main.rs"
cp crates/bracel/tests/consumer.rs "$consumer/app/tests/consumer.rs"
cargo test --offline --manifest-path "$consumer/app/Cargo.toml"
echo 'Packaged library passed an independent consumer test; CLI package verified.'
