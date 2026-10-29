#!/usr/bin/env bash
set -euo pipefail

REPO="casoon/auditmyvisitors"
BINARY="auditmyvisitors"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

# Detect OS and architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin)
    case "$ARCH" in
      arm64)  ARTIFACT="auditmyvisitors-macos-arm64" ;;
      x86_64)
        echo "Intel Macs are no longer supported."
        echo ""
        echo "If this is an Apple Silicon Mac, you are in a Rosetta shell —"
        echo "open a native terminal and run the installer again."
        echo ""
        echo "Otherwise, build from source: https://github.com/$REPO#build-from-source"
        exit 1
        ;;
      *)      echo "Unsupported architecture: $ARCH" && exit 1 ;;
    esac
    ;;
  Linux)
    case "$ARCH" in
      x86_64) ARTIFACT="auditmyvisitors-linux-x86_64" ;;
      *)      echo "Unsupported architecture: $ARCH" && exit 1 ;;
    esac
    ;;
  *)
    echo "Unsupported OS: $OS"
    echo "For Windows, download the binary manually from:"
    echo "https://github.com/$REPO/releases/latest"
    exit 1
    ;;
esac

# Get latest release tag
VERSION="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
  | grep '"tag_name"' | head -1 | sed 's/.*"tag_name": *"//;s/".*//')"

if [ -z "$VERSION" ]; then
  echo "Could not determine latest version." && exit 1
fi

DOWNLOAD_URL="https://github.com/$REPO/releases/download/$VERSION/$ARTIFACT"

echo "Installing auditmyvisitors $VERSION ..."
curl -fsSL "$DOWNLOAD_URL" -o "/tmp/$BINARY"

# Verify against the release's SHA256SUMS before installing anything
EXPECTED="$(curl -fsSL "https://github.com/$REPO/releases/download/$VERSION/SHA256SUMS" \
  | awk -v n="$ARTIFACT" '$2 == n { print $1 }')"
if command -v sha256sum >/dev/null 2>&1; then
  ACTUAL="$(sha256sum "/tmp/$BINARY" | cut -d ' ' -f 1)"
elif command -v shasum >/dev/null 2>&1; then
  ACTUAL="$(shasum -a 256 "/tmp/$BINARY" | cut -d ' ' -f 1)"
else
  rm -f "/tmp/$BINARY"
  echo "No sha256sum or shasum found; cannot verify the download." && exit 1
fi
if [ -z "$EXPECTED" ] || [ "$EXPECTED" != "$ACTUAL" ]; then
  rm -f "/tmp/$BINARY"
  echo "Checksum mismatch for $ARTIFACT $VERSION; not installing." && exit 1
fi
chmod +x "/tmp/$BINARY"

mkdir -p "$INSTALL_DIR"
mv "/tmp/$BINARY" "$INSTALL_DIR/$BINARY"

# Remind user to add install dir to PATH if needed
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
  echo ""
  echo "Add the following to your ~/.zshrc or ~/.bashrc:"
  echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
fi

echo ""
echo "✓ auditmyvisitors $VERSION installed to $INSTALL_DIR/$BINARY"
echo ""
echo "Get started:"
echo "  auditmyvisitors"
