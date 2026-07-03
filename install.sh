#!/usr/bin/env bash
# Install the objjc compiler to /usr/local/bin (override with PREFIX).
set -euo pipefail

PREFIX="${PREFIX:-/usr/local}"
BINDIR="$PREFIX/bin"
BINARY="objjc"
RELEASE="target/release/$BINARY"

cd "$(dirname "$0")"

if [ ! -x "$RELEASE" ]; then
  echo "Release binary not found; building..."
  cargo build --release
fi

# Use sudo if we can't write to the target directory.
SUDO=""
if [ ! -w "$BINDIR" ]; then
  SUDO="sudo"
fi

$SUDO install -d "$BINDIR"
$SUDO install -m 0755 "$RELEASE" "$BINDIR/$BINARY"
echo "Installed $BINARY to $BINDIR/$BINARY"
