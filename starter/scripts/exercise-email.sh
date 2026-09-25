#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
root=$PWD
copy="$root/.scratch/email-$(date +%Y%m%d%H%M%S)-$$"
mkdir -p "$copy"
cp Cargo.toml rust-toolchain.toml build.rs "$copy/"
if [[ -f Cargo.lock ]]; then
  cp Cargo.lock "$copy/"
else
  cp ../Cargo.lock "$copy/"
  sed -i "s|path = \"../crates/bracel\"|path = \"$root/../crates/bracel\"|" "$copy/Cargo.toml"
fi
if ! grep -q '^\[workspace\]' "$copy/Cargo.toml"; then
  printf '\n[workspace]\n' >> "$copy/Cargo.toml"
fi
cp -R src "$copy/src"
mkdir -p "$copy/templates" "$copy/tests"
cp docs/features/email-example/email.rs.txt "$copy/src/email.rs"
cp docs/features/email-example/note.html.txt "$copy/templates/note.html"
cp docs/features/email-example/email-test.rs.txt "$copy/tests/email.rs"
printf '\npub mod email;\n' >> "$copy/src/lib.rs"
echo "Disposable recipe copy: $copy"
d=${DOCKER_BIN:-docker}
name="starter-mail-trial-$$"
trap '"$d" rm -f "$name" >/dev/null 2>&1 || true' EXIT
"$d" run -d --name "$name" -p 127.0.0.1:11025:1025 -p 127.0.0.1:18025:8025 axllent/mailpit:v1.31.2 >/dev/null
curl --fail --silent --retry 20 --retry-connrefused --retry-delay 1 http://127.0.0.1:18025/api/v1/messages >/dev/null
cd "$copy"
cargo add lettre@0.11.23 --no-default-features --features builder,smtp-transport,tokio1-rustls-tls
cargo add askama@0.16.1
cargo fmt --all
cargo clippy --locked --all-targets -- -D warnings
SMTP_TEST_PORT=11025 cargo test --locked --test email
python3 "$root/scripts/assert-mailpit.py"
echo 'Email recipe passed. Optional implementation remains only in the disposable copy.'
