#!/bin/sh
# Builds berth on macOS and wraps it in a .app the Finder can open.
#
# Run from anywhere:  ./macos/build.sh
#
# Needs Zig 0.17.0 on PATH (mise, brew, or a downloaded tarball) and the
# Xcode Command Line Tools, which supply the frameworks this links against.
set -eu

cd "$(dirname "$0")/.."

if ! command -v zig >/dev/null 2>&1; then
    echo "zig not found. Install 0.17.0 first:  mise use -g zig@0.17.0" >&2
    exit 1
fi

echo "zig: $(zig version)"
zig build -Doptimize=ReleaseFast

APP="zig-out/berth.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

cp zig-out/bin/berth "$APP/Contents/MacOS/berth"
cp macos/Info.plist "$APP/Contents/Info.plist"
cp macos/berth.icns "$APP/Contents/Resources/berth.icns"

# An unsigned bundle runs locally; Gatekeeper only complains once it has been
# downloaded. This ad-hoc signature keeps macOS from treating it as damaged.
codesign --force --deep --sign - "$APP" 2>/dev/null || \
    echo "warning: ad-hoc codesign failed; the app will still run locally" >&2

echo "built $APP"
echo "open it with:  open $APP"
