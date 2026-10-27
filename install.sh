#!/bin/sh
# Install the latest larp release binary.
# Usage: curl -fsSL https://raw.githubusercontent.com/pal404error/larp-files/main/install.sh | sh
# Env: INSTALL_DIR (default: $HOME/.local/bin), LARP_VERSION (default: latest)
set -eu

REPO="pal404error/larp-files"
DIR="${INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${LARP_VERSION:-latest}"

OS="$(uname -s)"
case "$OS" in
  Linux)  ASSET="larp-Linux" ;;
  Darwin) ASSET="larp-macOS" ;;
  *) echo "Unsupported OS: $OS. On Windows, download larp-Windows.exe from https://github.com/$REPO/releases" >&2; exit 1 ;;
esac

if [ "$VERSION" = "latest" ]; then
  URL="https://github.com/$REPO/releases/latest/download/$ASSET"
else
  URL="https://github.com/$REPO/releases/download/$VERSION/$ASSET"
fi

mkdir -p "$DIR"
echo "Downloading $ASSET to $DIR/larp ..."
curl -fsSL "$URL" -o "$DIR/larp"
chmod +x "$DIR/larp"
echo "Installed larp. Make sure $DIR is on your PATH, then run: larp --version"
