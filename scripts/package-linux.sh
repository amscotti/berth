#!/bin/sh
# Stage the Linux binary, desktop entry, icon and docs as a tarball.
set -eu
cd "$(dirname "$0")/.."

VERSION="${BERTH_VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)}"
ARCH="${BERTH_ARCH:-$(uname -m)}"
case "$ARCH" in
    arm64|aarch64) ARCH=aarch64 ;;
    x86_64|amd64) ARCH=x86_64 ;;
esac

BIN="${BERTH_BIN:-target/release/berth}"
if [ ! -f "$BIN" ]; then
    echo "missing $BIN — run: cargo build --release -p berth-app --locked" >&2
    exit 1
fi

STAGE="target/pkg/berth-${VERSION}-linux-${ARCH}"
OUT="target/pkg/berth-${VERSION}-linux-${ARCH}.tar.gz"
rm -rf "$STAGE"
mkdir -p "$STAGE"
install -m 755 "$BIN" "$STAGE/berth"
install -m 644 packaging/linux/berth.desktop "$STAGE/berth.desktop"
install -m 644 packaging/linux/berth.png "$STAGE/berth.png"
install -m 644 LICENSE "$STAGE/LICENSE"
install -m 644 README.md "$STAGE/README.md"
mkdir -p target/pkg
rm -f "$OUT"
tar -C target/pkg -czf "$OUT" "berth-${VERSION}-linux-${ARCH}"
echo "wrote $OUT"
