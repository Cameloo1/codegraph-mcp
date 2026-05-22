#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="${1:-$(pwd)}"
CCEVAL="$REPO_ROOT/benchmarks/tracks/crosscodeeval/upstream/cceval"
if [[ ! -d "$CCEVAL" && -d "$REPO_ROOT/benchmarks/upstream/cceval" ]]; then
  CCEVAL="$REPO_ROOT/benchmarks/upstream/cceval"
fi
cd "$CCEVAL"

python3 -m pip install --user tree_sitter==0.20.4
mkdir -p build
python3 scripts/build_ts_lib.py
PROBE="$REPO_ROOT/benchmarks/tracks/crosscodeeval/workspaces/crosscodeeval_docker_probe/parser_load_smoke.py"
if [[ ! -f "$PROBE" ]]; then
  PROBE="$REPO_ROOT/benchmarks/workspaces/crosscodeeval_docker_probe/parser_load_smoke.py"
fi
python3 "$PROBE"
