#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
export BRACEL_WORKSPACE=1
exec bash starter/scripts/container-smoke.sh "$@"
