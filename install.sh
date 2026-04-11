#!/usr/bin/env bash
set -euo pipefail

REPO="casoon/auditmyvisitors"
BINARY="audit-my-visitors"
INSTALL_DIR="/usr/local/bin"

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

# Get latest release URL
DOWNLOAD_URL="https://github.com/$REPO/releases/latest/download/$ARTIFACT"

echo "Downloading audit-my-visitors..."
curl -fsSL "$DOWNLOAD_URL" -o "/tmp/$BINARY"
chmod +x "/tmp/$BINARY"

# Install — try /usr/local/bin, fall back to ~/bin
if [ -w "$INSTALL_DIR" ]; then
  mv "/tmp/$BINARY" "$INSTALL_DIR/$BINARY"
else
  echo "No write access to $INSTALL_DIR — installing to ~/bin instead"
  mkdir -p "$HOME/bin"
  mv "/tmp/$BINARY" "$HOME/bin/$BINARY"
  INSTALL_DIR="$HOME/bin"

  # Remind user to add ~/bin to PATH if needed
  if [[ ":$PATH:" != *":$HOME/bin:"* ]]; then
    echo ""
    echo "Add the following to your ~/.zshrc or ~/.bashrc:"
    echo "  export PATH=\"\$HOME/bin:\$PATH\""
  fi
fi

echo ""
echo "✓ audit-my-visitors installed to $INSTALL_DIR/$BINARY"
echo ""
echo "Get started:"
echo "  audit-my-visitors auth login"
