#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
d=${DOCKER_BIN:-docker}
command -v openssl >/dev/null # Sign disposable test-fixture access tokens only.
name="starter-smoke-$$"
cleanup() {
  "$d" rm -fv "$name-app" "$name-db" >/dev/null 2>&1 || true
  "$d" network rm "$name" >/dev/null 2>&1 || true
}
trap cleanup EXIT
if [[ ${BRACEL_WORKSPACE:-0} == 1 ]]; then
  "$d" build -t bracel-starter:smoke ..
else
  bash scripts/build-image.sh bracel-starter:smoke
fi
"$d" network create "$name" >/dev/null
"$d" run -d --name "$name-db" --network "$name" -e POSTGRES_USER=starter -e POSTGRES_PASSWORD=starter -e POSTGRES_DB=starter postgres:18.6-bookworm >/dev/null
for _ in {1..60}; do
  if "$d" exec "$name-db" pg_isready -h 127.0.0.1 -U starter -d starter >/dev/null 2>&1; then break; fi
  sleep 1
done
"$d" exec "$name-db" pg_isready -h 127.0.0.1 -U starter -d starter
db="postgres://starter:starter@$name-db:5432/starter"
"$d" run --rm --network "$name" -e DATABASE_URL="$db" bracel-starter:smoke migrate
"$d" run --rm --network "$name" -e DATABASE_URL="$db" bracel-starter:smoke migrate
inspection=$("$d" run --rm --network "$name" -e DATABASE_URL="$db" bracel-starter:smoke inspect --json --database)
[[ "$inspection" == *'"database_status": "checked"'* ]]
[[ "$inspection" == *'"status": "applied"'* ]]
[[ "$inspection" != *'starter:starter'* ]]
"$d" run --rm --network "$name" -e DATABASE_URL="$db" bracel-starter:smoke doctor --json --deploy --database
"$d" run -d --name "$name-app" --network "$name" --read-only --cap-drop ALL --security-opt no-new-privileges -e DATABASE_URL="$db" bracel-starter:smoke >/dev/null
"$d" run --rm --network "$name" curlimages/curl:8.19.0 --fail --retry 20 --retry-connrefused --retry-delay 1 "http://$name-app:3000/readyz"
code=$("$d" run --rm --network "$name" curlimages/curl:8.19.0 -s -o /dev/null -w '%{http_code}' "http://$name-app:3000/example/notes")
[[ "$code" == 404 ]]
"$d" stop --time 20 "$name-app" >/dev/null
[[ $("$d" inspect --format '{{.State.ExitCode}}' "$name-app") == 0 ]]
"$d" rm "$name-app" >/dev/null
"$d" run -d --name "$name-app" --network "$name" --read-only --cap-drop ALL --security-opt no-new-privileges -e DATABASE_URL="$db" -e ENABLE_EXAMPLE=true bracel-starter:smoke >/dev/null
"$d" run --rm --network "$name" curlimages/curl:8.19.0 --fail --retry 20 --retry-connrefused --retry-delay 1 "http://$name-app:3000/readyz"
created=$("$d" run --rm --network "$name" curlimages/curl:8.19.0 --fail -s -H 'Content-Type: application/json' -H 'Authorization: Bearer header-secret-sentinel' -d '{"title":"body-secret-sentinel"}' "http://$name-app:3000/example/notes?token=query-secret-sentinel")
id=$(printf '%s' "$created" | sed -n 's/.*"id":"\([a-f0-9-]*\)".*/\1/p')
[[ -n "$id" ]]
retrieved=$("$d" run --rm --network "$name" curlimages/curl:8.19.0 --fail -s "http://$name-app:3000/example/notes/$id")
[[ "$created" == "$retrieved" ]]
[[ "$created" == '{"data":{'* ]]
[[ "$created" == *'"created_at":'* ]]
listing=$("$d" run --rm --network "$name" curlimages/curl:8.19.0 --fail -s "http://$name-app:3000/example/notes?limit=1")
[[ "$listing" == '{"data":['* ]]
[[ "$listing" == *'"has_more":false,"next_cursor":null'* ]]
filtered=$("$d" run --rm --network "$name" curlimages/curl:8.19.0 --globoff --fail -s "http://$name-app:3000/example/notes?filter[title]=absent&sort=created_at")
[[ "$filtered" == '{"data":[],'* ]]
code=$("$d" run --rm --network "$name" curlimages/curl:8.19.0 -s -o /dev/null -w '%{http_code}' "http://$name-app:3000/example/notes?limit=101")
[[ "$code" == 422 ]]
logs=$("$d" logs "$name-app" 2>&1)
[[ "$logs" == *'request_id'* ]]
[[ "$logs" != *'secret-sentinel'* ]]
[[ "$logs" != *'starter:starter'* ]]
"$d" stop --time 20 "$name-app" >/dev/null
[[ $("$d" inspect --format '{{.State.ExitCode}}' "$name-app") == 0 ]]
# Authentication fixtures are public, disposable and excluded from the image.
"$d" rm "$name-app" >/dev/null
public_key=$(cat tests/fixtures/test-only-public.pem)
"$d" run -d --name "$name-app" --network "$name" --read-only --cap-drop ALL --security-opt no-new-privileges \
  -e DATABASE_URL="$db" -e ENABLE_EXAMPLE=true -e AUTH_MODE=bearer \
  -e AUTH_ISSUER=https://issuer.example -e AUTH_AUDIENCE=starter-api -e AUTH_PUBLIC_KEY_PEM="$public_key" \
  -e RATE_AUTHENTICATED_PER_MINUTE=2 -e CORS_ORIGINS=https://client.example bracel-starter:smoke >/dev/null
"$d" run --rm --network "$name" curlimages/curl:8.19.0 --fail --retry 20 --retry-connrefused --retry-delay 1 "http://$name-app:3000/readyz"
code=$("$d" run --rm --network "$name" curlimages/curl:8.19.0 -s -o /dev/null -w '%{http_code}' "http://$name-app:3000/example/notes")
[[ "$code" == 401 ]]
b64url() { openssl base64 -A | tr '+/' '-_' | tr -d '='; }
header=$(printf '%s' '{"alg":"RS256","typ":"at+jwt"}' | b64url)
expiry=$(( $(date +%s) + 600 ))
payload=$(printf '{"iss":"https://issuer.example","aud":"starter-api","sub":"smoke","scope":"notes:read","exp":%s}' "$expiry" | b64url)
signature=$(printf '%s' "$header.$payload" | openssl dgst -sha256 -sign tests/fixtures/test-only-private.pem | b64url)
token="$header.$payload.$signature"
for _ in 1 2; do
  code=$("$d" run --rm --network "$name" curlimages/curl:8.19.0 -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $token" "http://$name-app:3000/example/notes")
  [[ "$code" == 200 ]]
done
limited=$("$d" run --rm --network "$name" curlimages/curl:8.19.0 -s -i -H "Authorization: Bearer $token" -H 'Origin: https://client.example' "http://$name-app:3000/example/notes")
[[ "$limited" == *'429 Too Many Requests'* ]]
[[ "$limited" == *'retry-after:'* ]]
[[ "$limited" == *'access-control-allow-origin: https://client.example'* ]]
logs=$("$d" logs "$name-app" 2>&1)
[[ "$logs" != *"$token"* ]]
"$d" stop --time 20 "$name-app" >/dev/null
[[ $("$d" inspect --format '{{.State.ExitCode}}' "$name-app") == 0 ]]
echo 'Container smoke passed: migrations, diagnostics, readiness, example settings, filters, bearer auth, rate limits/CORS, persistence, log redaction, non-root/read-only runtime, SIGTERM.'
