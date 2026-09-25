#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p .scratch
context=$(mktemp -d "$PWD/.scratch/image.XXXXXX")
trap 'rm -rf "$context"' EXIT
cp Cargo.toml Cargo.lock rust-toolchain.toml build.rs Dockerfile .dockerignore "$context/"
cp -R src "$context/src"
cd "$context"
mkdir .cargo
# Resolve with the developer's normal Git credentials, outside the Docker build.
# Vendor all locked sources so no credentials or network access enter build layers.
cargo vendor --locked vendor > .cargo/config.toml
"${DOCKER_BIN:-docker}" build -t "${1:-bracel-starter:local}" .
