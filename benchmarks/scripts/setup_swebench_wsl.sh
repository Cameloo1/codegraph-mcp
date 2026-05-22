#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="${1:-$(pwd)}"
cd "$REPO_ROOT"

python3 -m pip install --user -e "$REPO_ROOT/benchmarks/upstream/SWE-bench"
python3 - <<'PY'
import resource
import swebench
print("resource ok")
print("swebench", swebench.__file__)
PY

python3 -m swebench.harness.run_evaluation \
  --dataset_name princeton-nlp/SWE-bench_Lite \
  --predictions_path gold \
  --max_workers 1 \
  --instance_ids sympy__sympy-20590 \
  --run_id codegraph-setup-gold \
  --timeout 120 \
  --report_dir "$REPO_ROOT/benchmarks/workspaces/swebench_gold_validation/report"
