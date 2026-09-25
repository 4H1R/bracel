#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
docker_bin=${DOCKER_BIN:-docker}
case "${1:-help}" in
  up) "$docker_bin" compose up -d --wait ;;
  down) "$docker_bin" compose down ;;
  migrate|run|doctor|inspect)
    if [[ -f .env ]]; then set -a; source .env; set +a; fi
    command=$1
    shift
    if [[ "$command" == run ]]; then command=serve; fi
    cargo run --quiet --locked --bin bracel-starter -- "$command" "$@" ;;
  *) echo 'Usage: bash scripts/dev.sh {up|down|migrate|run|doctor|inspect} [options]'; exit 2 ;;
esac
