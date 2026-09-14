#!/bin/sh
# Installs the latest macOS release into /Applications.
#
# Downloads are quarantined by macOS, and envhub is not notarized, so the app
# would refuse to open ("damaged") until the flag is removed. Needs the gh CLI
# because the repository is private.
set -e

repo="${ENVHUB_REPO:-tarikalim/EnvHub}"
dest="${1:-/Applications}"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

gh release download -R "$repo" -p 'envhub-macos-*.tar.gz' -D "$tmp"
tar -xzf "$tmp"/envhub-macos-*.tar.gz -C "$tmp"

rm -rf "$dest/envhub.app"
mv "$tmp/envhub.app" "$dest/envhub.app"
xattr -dr com.apple.quarantine "$dest/envhub.app"

echo "installed: $dest/envhub.app"
