#!/bin/sh
# Stage the unsigned macOS bundle as a zip for a release.
set -eu
cd "$(dirname "$0")/.."

VERSION="${BERTH_VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)}"
ARCH="${BERTH_ARCH:-$(uname -m)}"
case "$ARCH" in
    arm64|aarch64) ARCH=arm64 ;;
    x86_64|amd64) ARCH=x86_64 ;;
esac

BERTH_VERSION="$VERSION" sh macos/build.sh
mkdir -p target/pkg
OUT="target/pkg/berth-${VERSION}-macos-${ARCH}.zip"
rm -f "$OUT"
ditto -c -k --norsrc --keepParent target/Berth.app "$OUT"
echo "wrote $OUT"
