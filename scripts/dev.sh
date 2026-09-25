#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export COMPOSE_PROJECT_NAME=${COMPOSE_PROJECT_NAME:-$(basename "$PWD")}
exec bash starter/scripts/dev.sh "$@"
