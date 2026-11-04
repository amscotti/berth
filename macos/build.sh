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

# The zon version is the single source of truth for the app and the bundle.
VERSION=$(sed -n 's/.*\.version = "\([^"]*\)".*/\1/p' build.zig.zon | head -n 1)
if [ -z "$VERSION" ]; then
    echo "could not read the version from build.zig.zon" >&2
    exit 1
fi

# The release app is arm64 and its plist promises macOS 13.0. The target
# carries the floor; the plist is stamped from that same value so the Mach-O
# minimum and Info.plist cannot disagree.
DEFAULT_FLOOR=13.0
case "$(uname -m)" in
    arm64) DEFAULT_TARGET=aarch64-macos.$DEFAULT_FLOOR ;;
    x86_64) DEFAULT_TARGET=x86_64-macos.$DEFAULT_FLOOR ;;
    *) DEFAULT_TARGET=aarch64-macos.$DEFAULT_FLOOR ;;
esac
TARGET="${BERTH_MACOS_TARGET:-$DEFAULT_TARGET}"
case "$TARGET" in
    *-macos.*) MACOS_FLOOR="${TARGET#*-macos.}" ;;
    *-macos) TARGET="$TARGET.$DEFAULT_FLOOR"; MACOS_FLOOR="$DEFAULT_FLOOR" ;;
    *)
        echo "BERTH_MACOS_TARGET must name a macOS target, for example aarch64-macos.13.0" >&2
        exit 1
        ;;
esac

echo "zig: $(zig version)"
echo "berth: $VERSION ($TARGET, macOS $MACOS_FLOOR)"
zig build -Doptimize=ReleaseFast -Dtarget="$TARGET"

APP="zig-out/berth.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

cp zig-out/bin/berth "$APP/Contents/MacOS/berth"
strip "$APP/Contents/MacOS/berth"
sed -e "s|@VERSION@|$VERSION|g" -e "s|@MACOS_FLOOR@|$MACOS_FLOOR|g" \
    macos/Info.plist > "$APP/Contents/Info.plist"
cp macos/berth.icns "$APP/Contents/Resources/berth.icns"

# An unsigned bundle runs locally; Gatekeeper only complains once it has been
# downloaded. A failed ad-hoc signature leaves a bundle macOS may call damaged,
# so this is fatal rather than a warning.
codesign --force --deep --sign - "$APP"
codesign --verify --deep --strict "$APP"

echo "built $APP"
echo "open it with:  open $APP"
