#!/bin/sh
# Install a larp release binary after verifying its SHA-256 checksum.
# Usage: curl -fsSL https://raw.githubusercontent.com/pal404error/larp-files/main/install.sh | sh
#   (safer: download this script, read it, then run `sh install.sh`)
# Env: INSTALL_DIR (default: $HOME/.local/bin), LARP_VERSION (default: latest),
#      LARP_DOWNLOAD_BASE (default: https://github.com/$REPO/releases; for mirrors and tests)
#
# The binary must match the SHA256SUMS file published with the same release,
# or nothing is installed. For a stronger check (build provenance signed by
# GitHub Actions), run afterwards:
#   gh attestation verify "$INSTALL_DIR/larp" --repo pal404error/larp-files
set -eu

REPO="pal404error/larp-files"
DIR="${INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${LARP_VERSION:-latest}"
BASE="${LARP_DOWNLOAD_BASE:-https://github.com/$REPO/releases}"

OS="$(uname -s)"
case "$OS" in
  Linux)  ASSET="larp-Linux" ;;
  Darwin) ASSET="larp-macOS" ;;
  *) echo "Unsupported OS: $OS. On Windows, use install.ps1 or download larp-Windows.exe from https://github.com/$REPO/releases" >&2; exit 1 ;;
esac

if [ "$VERSION" = "latest" ]; then
  URL="$BASE/latest/download"
else
  URL="$BASE/download/$VERSION"
fi

if command -v sha256sum >/dev/null 2>&1; then
  sha256() { sha256sum "$1" | awk '{print $1}'; }
elif command -v shasum >/dev/null 2>&1; then
  sha256() { shasum -a 256 "$1" | awk '{print $1}'; }
else
  echo "Need sha256sum or shasum to verify the download; refusing to install." >&2
  exit 1
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT INT TERM

echo "Downloading $ASSET and SHA256SUMS from $URL ..."
curl -fsSL "$URL/$ASSET" -o "$TMP/$ASSET"
if ! curl -fsSL "$URL/SHA256SUMS" -o "$TMP/SHA256SUMS"; then
  echo "This release has no SHA256SUMS file; refusing to install an unverified binary." >&2
  exit 1
fi

EXPECTED="$(awk -v f="$ASSET" '$2 == f || $2 == "*" f { print $1 }' "$TMP/SHA256SUMS")"
if [ -z "$EXPECTED" ]; then
  echo "No checksum for $ASSET in SHA256SUMS; refusing to install." >&2
  exit 1
fi
ACTUAL="$(sha256 "$TMP/$ASSET")"
if [ "$EXPECTED" != "$ACTUAL" ]; then
  echo "Checksum mismatch for $ASSET (expected $EXPECTED, got $ACTUAL); refusing to install." >&2
  exit 1
fi

mkdir -p "$DIR"
chmod +x "$TMP/$ASSET"
mv -f "$TMP/$ASSET" "$DIR/larp"
echo "Installed larp to $DIR/larp (sha256 $ACTUAL)."
echo "Make sure $DIR is on your PATH, then run: larp --version"
