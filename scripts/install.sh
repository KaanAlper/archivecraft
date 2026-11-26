#!/usr/bin/env sh
set -eu

REPO="KaanAlper/archivecraft"
INSTALL_DIR="${ARCHIVECRAFT_INSTALL_DIR:-$HOME/.local/bin}"

if [ "${1:-}" = "--uninstall" ]; then
  if [ -x "$INSTALL_DIR/archivecraft-cli" ]; then
    "$INSTALL_DIR/archivecraft-cli" integrate --remove || true
  fi
  rm -f "$INSTALL_DIR/archivecraft" "$INSTALL_DIR/archivecraft-cli"
  echo "ArchiveCraft removed from $INSTALL_DIR."
  exit 0
fi

OS="$(uname -s)"
ARCH="$(uname -m)"
if [ "$OS" != "Linux" ]; then
  echo "This installer is for Linux (detected: $OS)." >&2
  exit 1
fi
case "$ARCH" in
  x86_64|amd64) ARCH="x86_64" ;;
  *)
    echo "Unsupported architecture: $ARCH (prebuilt Linux archives currently target x86_64)." >&2
    exit 1
    ;;
esac

need() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Missing required command: $1" >&2
    exit 1
  fi
}
need curl
need tar

API_URL="https://api.github.com/repos/$REPO/releases/latest"
JSON="$(curl -fsSL "$API_URL")"
TAG="$(printf '%s\n' "$JSON" | sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)"
if [ -z "$TAG" ]; then
  echo "Could not find a published GitHub release for $REPO." >&2
  exit 1
fi
VER="${TAG#v}"
ASSET="archivecraft-${VER}-linux-${ARCH}.tar.gz"
BASE_URL="https://github.com/$REPO/releases/download/$TAG"

TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

echo "Downloading $ASSET ($TAG)..."
curl -fsSL "$BASE_URL/$ASSET" -o "$TMPDIR/$ASSET"

if curl -fsSL "$BASE_URL/SHA256SUMS.txt" -o "$TMPDIR/SHA256SUMS.txt" 2>/dev/null; then
  if command -v sha256sum >/dev/null 2>&1; then
    EXPECTED="$(grep "  $ASSET\$" "$TMPDIR/SHA256SUMS.txt" | awk '{print $1}' || true)"
    if [ -n "$EXPECTED" ]; then
      ACTUAL="$(sha256sum "$TMPDIR/$ASSET" | awk '{print $1}')"
      if [ "$EXPECTED" != "$ACTUAL" ]; then
        echo "SHA-256 mismatch for $ASSET!" >&2
        exit 1
      fi
      echo "SHA-256 verified."
    fi
  fi
fi

mkdir -p "$INSTALL_DIR"
tar -xzf "$TMPDIR/$ASSET" -C "$TMPDIR"
install -m 0755 "$TMPDIR/archivecraft" "$INSTALL_DIR/archivecraft"
install -m 0755 "$TMPDIR/archivecraft-cli" "$INSTALL_DIR/archivecraft-cli"

echo "Installing desktop entry (.desktop), icon and file-manager context menus..."
"$INSTALL_DIR/archivecraft-cli" integrate || true

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "${XDG_DATA_HOME:-$HOME/.local/share}/applications" >/dev/null 2>&1 || true
fi

echo ""
echo "ArchiveCraft $TAG installed to $INSTALL_DIR."
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *) echo "Note: add $INSTALL_DIR to your PATH to run 'archivecraft' from the terminal." ;;
esac
