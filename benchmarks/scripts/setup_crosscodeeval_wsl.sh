#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="${1:-$(pwd)}"
cd "$REPO_ROOT/benchmarks/upstream/cceval"

python3 -m pip install --user tree_sitter==0.20.4
mkdir -p build
python3 scripts/build_ts_lib.py
python3 "$REPO_ROOT/benchmarks/workspaces/crosscodeeval_docker_probe/parser_load_smoke.py"
