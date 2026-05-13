#!/usr/bin/env sh
set -eu

DRY_RUN=0
VERSION=latest
INSTALL_DIR="${HOME}/.codegraph/bin"

while [ "$#" -gt 0 ]; do
  case "$1" in
    --dry-run) DRY_RUN=1 ;;
    --version)
      shift
      VERSION="${1:-latest}"
      ;;
    --install-dir)
      shift
      INSTALL_DIR="${1:-$INSTALL_DIR}"
      ;;
    *)
      echo "unknown option: $1" >&2
      exit 2
      ;;
  esac
  shift
done

OS="$(uname -s)"
ARCH="$(uname -m)"
case "${OS}:${ARCH}" in
  Linux:x86_64) TARGET="x86_64-unknown-linux-gnu" ;;
  Darwin:arm64) TARGET="aarch64-apple-darwin" ;;
  Darwin:x86_64) TARGET="x86_64-apple-darwin" ;;
  *) echo "unsupported platform: ${OS}:${ARCH}" >&2; exit 2 ;;
esac

ARCHIVE="codegraph-mcp-${TARGET}.tar.gz"
BINARY="${INSTALL_DIR}/codegraph-mcp"

if [ "$DRY_RUN" -eq 1 ]; then
  printf '{"status":"dry_run","version":"%s","target":"%s","archive":"%s","install_dir":"%s","binary":"%s","network":"not used in dry run","workflow":"single-agent-only"}\n' "$VERSION" "$TARGET" "$ARCHIVE" "$INSTALL_DIR" "$BINARY"
  exit 0
fi

mkdir -p "$INSTALL_DIR"
echo "Network download is intentionally not implemented in this template. Use GitHub release archives or cargo install for now." >&2
exit 1

