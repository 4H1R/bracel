#!/usr/bin/env bash
# Install the prebuilt Bracel CLI into a user-owned directory.
set -euo pipefail
version=''
install_dir="${HOME}/.local/bin"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --version) version="${2:?Missing version}"; shift 2 ;;
    --install-dir) install_dir="${2:?Missing installation directory}"; shift 2 ;;
    *) echo 'Usage: bash install.sh [--version VERSION] [--install-dir DIRECTORY]' >&2; exit 2 ;;
  esac
done
command -v curl >/dev/null || { echo 'Install curl, then run this installer again.' >&2; exit 1; }
case "$(uname -s)/$(uname -m)" in
  Linux/x86_64) target=x86_64-unknown-linux-gnu ;;
  Darwin/x86_64) target=x86_64-apple-darwin ;;
  Darwin/arm64) target=aarch64-apple-darwin ;;
  *) echo 'No prebuilt CLI for this platform. Install bracel-cli using Cargo.' >&2; exit 1 ;;
esac
curl_args=(--fail --silent --show-error --location --proto '=https' --proto-redir '=https' --connect-timeout 20 --max-time 180 --retry 2)
if [[ -z "$version" ]]; then
  latest=$(curl "${curl_args[@]}" --head --output /dev/null --write-out '%{url_effective}' https://github.com/4H1R/bracel/releases/latest)
  version=${latest##*/}
fi
version=${version#v}
[[ "$version" =~ ^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$ ]] || { echo 'Expected a stable release version.' >&2; exit 1; }
mkdir -p "$install_dir"
install_dir=$(cd "$install_dir" && pwd)
stage=$(mktemp -d "$install_dir/.install-XXXXXXXX")
trap 'rm -rf -- "$stage"' EXIT
asset="bracel-$target"
base="https://github.com/4H1R/bracel/releases/download/v$version"
curl "${curl_args[@]}" --output "$stage/SHA256SUMS" "$base/SHA256SUMS"
curl "${curl_args[@]}" --output "$stage/$asset" "$base/$asset"
expected=$(awk -v asset="$asset" '$2 == asset || $2 == "*" asset {print $1}' "$stage/SHA256SUMS")
if command -v sha256sum >/dev/null; then
  actual=$(sha256sum "$stage/$asset" | awk '{print $1}')
else
  actual=$(shasum -a 256 "$stage/$asset" | awk '{print $1}')
fi
[[ "$expected" =~ ^[a-f0-9]{64}$ && "$actual" == "$expected" ]] || { echo 'Release checksum mismatch. Existing installation preserved.' >&2; exit 1; }
chmod 755 "$stage/$asset"
[[ "$("$stage/$asset" --version)" == "bracel $version" ]] || { echo 'Unexpected CLI version. Existing installation preserved.' >&2; exit 1; }
# The verified download is on the destination filesystem; replacement is atomic.
mv -f "$stage/$asset" "$install_dir/bracel"
printf 'Installed Bracel %s to %s/bracel\n' "$version" "$install_dir"
case ":$PATH:" in
  *":$install_dir:"*) ;;
  *) printf 'Add this directory to your shell PATH: %s\n' "$install_dir" ;;
esac
printf 'Run: "%s/bracel" setup\nNew projects use Docker; setup checks Git and Docker.\n' "$install_dir"
