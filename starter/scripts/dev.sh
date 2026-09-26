#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
docker_bin=${DOCKER_BIN:-docker}
project=${COMPOSE_PROJECT_NAME:-$(basename "$PWD")}
case "${1:-help}" in
  up) "$docker_bin" compose --project-name "$project" up -d --wait ;;
  down) "$docker_bin" compose --project-name "$project" down ;;
  migrate|run|doctor|inspect|auth:mail-work|auth:mail-once|auth:cleanup)
    if [[ -f .env ]]; then set -a; source .env; set +a; fi
    command=$1
    shift
    if [[ "$command" == run ]]; then command=serve; fi
    cargo run --quiet --locked --bin bracel-starter -- "$command" "$@" ;;
  *) echo 'Usage: bash scripts/dev.sh {up|down|migrate|run|doctor|inspect|auth:mail-work|auth:mail-once|auth:cleanup} [options]'; exit 2 ;;
esac
