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
      x86_64) ARTIFACT="auditmyvisitors-macos-x86_64" ;;
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
