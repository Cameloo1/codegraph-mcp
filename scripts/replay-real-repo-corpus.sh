#!/usr/bin/env sh
set -eu

ALLOW_NETWORK=0
CACHE_ROOT=".codegraph-bench-cache/real-repos"

while [ "$#" -gt 0 ]; do
  case "$1" in
    --allow-network) ALLOW_NETWORK=1 ;;
    --cache-root)
      shift
      CACHE_ROOT="${1:-$CACHE_ROOT}"
      ;;
    *) echo "unknown option: $1" >&2; exit 2 ;;
  esac
  shift
done

if [ "$ALLOW_NETWORK" -ne 1 ]; then
  printf '{"status":"skipped","reason":"network disabled; pass --allow-network to clone real repos","cache_root":"%s","workflow":"single-agent-only"}\n' "$CACHE_ROOT"
  exit 0
fi

mkdir -p "$CACHE_ROOT"
echo "Use the PowerShell replay script on Windows or port this shell template before running network clones." >&2
exit 1

