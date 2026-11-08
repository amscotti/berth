#!/bin/sh
# Build berth and wrap it in an unsigned .app. No codesign, no notarization,
# no keychain prompt. See docs/RELEASE.md.
set -eu
cd "$(dirname "$0")/.."

echo "rust: $(rustc --version)"
cargo build --release -p berth-app

APP="target/Berth.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

cp target/release/berth "$APP/Contents/MacOS/berth"
COMMIT=$(git rev-parse --short HEAD 2>/dev/null || echo unknown)
sed "s/__BERTH_COMMIT__/$COMMIT/" macos/Info.plist > "$APP/Contents/Info.plist"
cp macos/berth.icns "$APP/Contents/Resources/berth.icns"

if [ "${BERTH_SIGN:-0}" = "1" ]; then
    echo "BERTH_SIGN=1 requires an explicit BERTH_SIGN_IDENTITY; refusing to prompt" >&2
    exit 1
fi
echo "signing skipped (BERTH_SIGN=0)"
echo "built $APP"
echo "open it with:  open $APP"
