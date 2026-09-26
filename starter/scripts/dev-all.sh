#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ -f .env ]]; then set -a; source .env; set +a; fi
: "${DATABASE_URL:?Configure DATABASE_URL in .env}"
cargo build --locked --features batteries,identity,telemetry --bin bracel-starter
workspace=$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys;print(json.load(sys.stdin)["target_directory"])')
binary="$workspace/debug/bracel-starter"
"$binary" migrate
pids=()
cleanup() {
    trap - EXIT INT TERM
    for pid in "${pids[@]}"; do kill -TERM "$pid" 2>/dev/null || true; done
    wait || true
}
trap cleanup EXIT INT TERM
"$binary" serve & pids+=("$!")
"$binary" jobs:work & pids+=("$!")
"$binary" schedule:work & pids+=("$!")
if [[ ${ENABLE_BATTERIES:-false} == true ]]; then
    queues=(incoming)
    [[ -z ${MAIL_LOCAL_PORT:-} ]] || queues+=(mail)
    [[ -z ${WEBHOOK_URL:-} ]] || queues+=(webhooks)
    for queue in "${queues[@]}"; do
        (trap 'exit 0' INT TERM; while true; do "$binary" delivery:once "$queue"; sleep 1; done) & pids+=("$!")
    done
fi
wait -n "${pids[@]}"
