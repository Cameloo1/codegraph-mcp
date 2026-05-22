from __future__ import annotations

import argparse
import json
import subprocess
import sys
import time
from pathlib import Path

from benchmarks.harness.config import load_config
from benchmarks.harness.reports.generate_charts import generate_charts
from benchmarks.harness.runners.run_retrieval_eval import run as run_retrieval


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--config", default="benchmarks/configs/internal_gold_smoke.toml")
    parser.add_argument("--quick", action="store_true")
    parser.add_argument("--output-dir", default=None)
    args = parser.parse_args(argv)
    started = int(time.time())
    output_dir = Path(args.output_dir) if args.output_dir else Path("benchmarks/results/summaries") / f"{started}_smoke"
    output_dir.mkdir(parents=True, exist_ok=True)
    test_cmd = [sys.executable, "-m", "unittest", "discover", "-s", "benchmarks/harness/tests"]
    test_proc = subprocess.run(test_cmd, text=True, capture_output=True, check=False)
    (output_dir / "unit_tests.json").write_text(
        json.dumps(
            {
                "command": test_cmd,
                "exit_code": test_proc.returncode,
                "stdout": test_proc.stdout,
                "stderr": test_proc.stderr,
            },
            indent=2,
        ),
        encoding="utf-8",
    )
    if test_proc.returncode != 0:
        return test_proc.returncode
    config = load_config(args.config)
    modes = ["none", "rg_only"] if args.quick else config.modes
    max_tasks = 5 if args.quick else config.max_tasks
    results = run_retrieval(config, output_dir / "retrieval", modes=modes, max_tasks=max_tasks)
    summary_path = output_dir / "retrieval" / "summary.json"
    if summary_path.exists():
        generate_charts(summary_path, output_dir / "charts")
    smoke_summary = {
        "schema_version": "benchmark_smoke_suite_v1",
        "status": "pass",
        "quick": args.quick,
        "unit_tests": "pass",
        "retrieval_results": len(results),
        "claim_boundary": "local diagnostic smoke; not a public benchmark claim",
    }
    (output_dir / "smoke_summary.json").write_text(json.dumps(smoke_summary, indent=2), encoding="utf-8")
    (output_dir / "smoke_summary.md").write_text(
        "# Benchmark Smoke Suite\n\n"
        "Status: pass\n\n"
        "This is a local diagnostic smoke, not a public benchmark claim.\n",
        encoding="utf-8",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

