#!/usr/bin/env bash
# Install the objjc compiler to /usr/local/bin (override with PREFIX).
set -euo pipefail

PREFIX="${PREFIX:-/usr/local}"
BINDIR="$PREFIX/bin"
SHAREDIR="$PREFIX/share/objj"
BINARY="objjc"
RELEASE="target/release/$BINARY"
FRAMEWORKS="../Frameworks"

cd "$(dirname "$0")"

if [ ! -x "$RELEASE" ]; then
  echo "Release binary not found; building..."
  cargo build --release
fi

# Use sudo if we can't write to the target directories.
SUDO=""
if [ ! -w "$PREFIX" ]; then
  SUDO="sudo"
fi

$SUDO install -d "$BINDIR"
$SUDO install -m 0755 "$RELEASE" "$BINDIR/$BINARY"
echo "Installed $BINARY to $BINDIR/$BINARY"

# Copy the bundled Frameworks into the shared prefix so the compiler can find
# them regardless of the current project directory.
if [ -d "$FRAMEWORKS" ]; then
  $SUDO install -d "$SHAREDIR"
  $SUDO rm -rf "$SHAREDIR/Frameworks"
  $SUDO cp -R "$FRAMEWORKS" "$SHAREDIR/Frameworks"
  echo "Installed Frameworks to $SHAREDIR/Frameworks"
fi
