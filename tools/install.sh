#!/bin/sh
# Builds envhub and installs it as an app bundle (default: ~/Applications/envhub.app).
set -e
cd "$(dirname "$0")/.."
APP="${1:-$HOME/Applications/envhub.app}"

cargo build --release

mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp -f assets/Info.plist "$APP/Contents/Info.plist"
# A real copy, not a symlink: macOS ignores the bundle icon when the executable
# resolves outside the bundle.
cp -f target/release/envhub "$APP/Contents/MacOS/envhub"
cp -f assets/envhub.icns "$APP/Contents/Resources/envhub.icns"
touch "$APP"

echo "installed: $APP"
