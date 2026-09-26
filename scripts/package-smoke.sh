#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
root=$PWD
version=$(python3 -c 'import tomllib; print(tomllib.load(open("crates/bracel/Cargo.toml", "rb"))["package"]["version"])')
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-"$root/target"}
cargo package --workspace --exclude bracel-starter --allow-dirty --locked --all-features
consumer=$(mktemp -d)
trap 'rm -rf "$consumer"' EXIT
tar -xzf "$CARGO_TARGET_DIR/package/bracel-$version.crate" -C "$consumer"
for package in bracel-jobs bracel-data bracel-integrations bracel-realtime bracel-delivery bracel-files; do
    tar -xzf "$CARGO_TARGET_DIR/package/$package-$version.crate" -C "$consumer"
done
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

[patch.crates-io]
bracel = { path = "../bracel-$version" }
bracel-jobs = { path = "../bracel-jobs-$version" }
bracel-data = { path = "../bracel-data-$version" }
bracel-integrations = { path = "../bracel-integrations-$version" }
bracel-realtime = { path = "../bracel-realtime-$version" }
bracel-delivery = { path = "../bracel-delivery-$version" }
bracel-files = { path = "../bracel-files-$version" }
TOML
printf 'fn main() {}\n' > "$consumer/app/src/main.rs"
cp crates/bracel/tests/consumer.rs "$consumer/app/tests/consumer.rs"
cargo test --offline --manifest-path "$consumer/app/Cargo.toml"
echo 'Packaged library passed an independent consumer test; CLI package verified.'
