#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
exec bash starter/scripts/openapi.sh "$@"
