#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
mode=${1:-check}
case "$mode" in
  write|check) ;;
  *) echo 'Usage: bash scripts/openapi.sh {write|check}'; exit 2 ;;
esac
generated=$(mktemp docs/.openapi.XXXXXX)
trap 'rm -f "$generated"' EXIT
cargo run --quiet --locked --bin openapi > "$generated"
if [[ "$mode" == write ]]; then
  mv "$generated" docs/openapi.json
else
  diff -u docs/openapi.json "$generated"
fi
if [[ ${BRACEL_OPTIONAL_CONTRACTS:-false} == true ]]; then
  generated_optional=$(mktemp docs/.openapi-packages.XXXXXX)
  trap 'rm -f "$generated" "$generated_optional"' EXIT
  cargo run --quiet --locked --bin openapi --features batteries > "$generated_optional"
  if [[ "$mode" == write ]]; then
    mv "$generated_optional" docs/openapi-packages.json
  else
    diff -u docs/openapi-packages.json "$generated_optional"
  fi
fi
