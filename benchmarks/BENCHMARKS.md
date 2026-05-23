# Benchmark Plan

## Track 1: Official Benchmark Track

Purpose: produce official-style scores where possible.

Rules:

- Use upstream harnesses as-is.
- Pin upstream commit and dataset version.
- Pin Docker/images/dependencies when used.
- Record the exact model and agent scaffold.
- Store raw predictions/logs separately under ignored result paths.
- Do not modify official scoring.

## Track 2: CodeGraph Product/Ablation Track

Purpose: measure CodeGraph's contribution as a context layer.

Metrics:

- gold file recall@k
- gold symbol recall@k
- gold span recall@k
- MRR
- precision@1/@5/@10
- non-gold top-k counts and wrong-context rate
- gold density in returned context
- forbidden file/symbol hits and context-poison rate
- context recall@k
- context bytes and estimated context tokens
- tool calls
- wall time
- cost when model pricing is configured
- time to first useful context
- wrong-file edits
- nonexistent-symbol references
- claimability violations
- unsupported-claim violations
- no-proof behavior correctness
- patch resolved/pass rate where a real patch evaluator exists

Provider-visible task fields are separated from evaluator-only fields. Providers
may use prompt/task text, visible file/symbol/text hints, stack frames, error
messages, visible config keys, language, repo metadata explicitly allowed by
the task, and budget fields. Providers must not receive gold files/symbols,
gold spans, gold context paths, expected answer files/symbols, forbidden
files/symbols/spans, oracle patch files, answer metadata, evaluator notes, or
hidden benchmark labels. The runner records sanitized fields removed and visible
query-term provenance for every task.

Benchmark Layer v0.5 adds `rg_planned` and `codegraph_planned` provider modes.
They are implemented and have been run in local v0.5 internal/external
diagnostic comparisons. Those runs remain local diagnostics only and do not
support a CodeGraph-over-rg or public benchmark claim.

## Upstream Notes

- SWE-bench evaluates patches for real GitHub issues and uses Docker for
  reproducible evaluation. SWE-bench Lite is a 300-task lower-cost subset with
  23 development instances.
- SWE-bench Verified is a human-filtered 500-task subset.
- SWE-bench-Live is monthly-updated and later-stage because the moving dataset
  makes reproducibility and cost control harder.
- RepoBench v1.1 provides repository-level Python and Java code-completion data.
- CrossCodeEval is a multilingual cross-file completion benchmark for Python,
  Java, TypeScript, and C#.

## Local Result Status

Internal product-ablation reports are diagnostic unless explicitly promoted.
Official benchmark results are only official-compatible when upstream scoring was
used without modification and the full run metadata is retained.

## Setup Verification

Normal usage is the one-command benchmark suite:

```powershell
python -m benchmarks.harness.runners.run_benchmark_suite --suite full --output-dir benchmarks/results/summaries/<run_id>
```

Focused suite modes:

```powershell
python -m benchmarks.harness.runners.run_benchmark_suite --suite smoke --output-dir benchmarks/results/summaries/<run_id>
python -m benchmarks.harness.runners.run_benchmark_suite --suite retrieval --output-dir benchmarks/results/summaries/<run_id>
python -m benchmarks.harness.runners.run_benchmark_suite --suite swebench-focused --output-dir benchmarks/results/summaries/<run_id>
```

The suite owns preflight, run planning, command logging, blocked-track records,
timing accounting, quality-per-budget metrics, aggregation, charts, and final
reports. It writes `run_plan.json` before executing tracks and records skipped
or blocked tracks in `blocked_tracks.json`.

Targeted setup verification remains available:

```powershell
python -m benchmarks.harness.runners.verify_benchmark_setup --output-dir benchmarks/results/summaries/setup_verification_local
```

The verifier checks:

- internal gold task loading
- external adapter readiness
- Docker/SWE-bench readiness
- external agent command readiness
- provider isolation
- config validity
- ignored/local output paths
- pinned source metadata
- tracked large-artifact leaks

## Canonical Track Homes

Each benchmark target owns its tracked config and local generated roots under
`benchmarks/tracks/<track>/`:

| Track | Canonical home | Primary configs |
|---|---|---|
| Internal gold | `benchmarks/tracks/internal_gold/` | `configs/smoke.toml`, `configs/full.toml` |
| RepoBench | `benchmarks/tracks/repobench/` | `configs/smoke.toml`, `configs/small.toml` |
| CrossCodeEval | `benchmarks/tracks/crosscodeeval/` | `configs/smoke.toml`, `configs/small.toml` |
| SWE-bench Lite | `benchmarks/tracks/swebench_lite/` | `configs/smoke.toml`, `configs/lite_10.toml` |
| Graph truth | `benchmarks/tracks/graph_truth/` | fixture-driven graph/proof cases |
| OpenEvolve lab | `benchmarks/tracks/openevolve/` | experimental policy-lab configs |

Legacy paths under `benchmarks/configs/` and `benchmarks/scripts/` are
compatibility aliases/wrappers. New runs should prefer the track-local paths.
Full multi-track reports may still aggregate under `benchmarks/results/`, but
per-track generated workspaces, upstream checkouts, and results belong under
the owning track and remain ignored/local.

## Current External Targets

### RepoBench

Status: source checkout pinned, real Python v1.1 dataset export ready for
retrieval-context smoke and configured small retrieval runs.

Required setup:

```powershell
benchmarks/tracks/repobench/scripts/setup_repobench.ps1 -Rows 20
```

Exported real rows live under ignored benchmark workspaces. The tiny tracked
fixture under `benchmarks/tracks/repobench/fixtures/` remains
adapter-test-only and is not official data.

Current clean retrieval runs derive provider query terms only from visible task
text and explicit visible hints. Any older RepoBench result whose provider
terms came from gold symbols/files is `gold_hint_diagnostic`, not comparable
with clean task-driven retrieval.

### CrossCodeEval

Status: source checkout pinned; Python, Java, TypeScript, and C# JSONL data are
extracted for adapter smoke; Linux-built tree-sitter parser libraries were
created and parser-load smoked.

Parser-library setup:

```powershell
benchmarks/tracks/crosscodeeval/scripts/setup_docker.ps1
```

The Docker setup pins `tree-sitter-python` `v0.20.4`, `tree-sitter-java`
`v0.20.2`, `tree-sitter-typescript` `v0.20.6`, and `tree-sitter-c-sharp`
`v0.20.0` before building. This avoids ABI drift from latest grammar heads.

Full CrossCodeEval generation quality still requires an explicit model/inference
configuration and an upstream scoring run. Retrieval-context runs remain local
diagnostic product-ablation results unless official scoring is used exactly.

Current clean retrieval runs do not derive provider query terms from gold
context filenames, expected completions, or hidden answer metadata. Any older
CrossCodeEval result that did so is `gold_hint_diagnostic`, not comparable with
clean task-driven retrieval.

### SWE-bench Lite

Status: source checkout pinned, editable package install succeeded in the
ignored benchmark venv, Docker Desktop is reachable from the normal user shell,
Lite dataset access was verified, and the official harness completed a current
Linux-container gold validation for `sympy__sympy-20590`.

Cached prior gold validation may be recorded as evidence only. It must not be
counted as a current live gold-validation run. Current live readiness is only
true when Docker/Linux harness prerequisites are ready in the current preflight.
The Codex sandbox identity cannot access Docker Desktop named pipes on this
machine, so Docker-backed SWE-bench commands must run from a normal
user/elevated PowerShell process or an explicitly approved unsandboxed command.

Gold validation command:

```powershell
benchmarks/tracks/swebench_lite/scripts/run_harness_linux_container.ps1
```

Latest live gold validation:

- Run id: `codegraph-live-gold-20260523-083023`
- Result: 1 completed, 1 resolved, 0 errors for `sympy__sympy-20590`
- Report: `benchmarks/workspaces/swebench_gold_validation/gold.codegraph-live-gold-20260523-083023.json`

Patch scoring setup is ready when the Codex wrapper command below is configured.
That setup readiness is not a real patch-quality result.

Configure a real patch-quality run with:

```powershell
$env:CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND = "<your fixed agent command>"
python -m benchmarks.harness.runners.run_patch_eval --config benchmarks/tracks/swebench_lite/configs/patch_runner_external_agent.example.toml
```

Codex CLI can be used through the saved wrapper:

```powershell
$env:CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND = "powershell -NoProfile -ExecutionPolicy Bypass -File benchmarks/tracks/swebench_lite/scripts/run_codex_external_patch_agent.ps1"
python -m benchmarks.harness.runners.run_patch_eval --config benchmarks/tracks/swebench_lite/configs/codex_external_agent.example.toml
```

The wrapper contract is stable: stdin is benchmark task/context JSON, stdout is
the final `diff --git` patch, and raw prompts/logs stay under ignored benchmark
workspaces. The wrapper supports `-ValidateOnly` for readiness checks that do
not call a model.

Latest setup check:

- `CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND` set to the Codex wrapper above.
- `run_patch_eval --config benchmarks/configs/swebench_lite_smoke.toml` returned `status=ready`.
- `run_benchmark_suite --suite swebench-focused --output-dir benchmarks/results/summaries/swebench_live_unblock_20260523_083023` completed live gold validation and patch-quality setup tracks.
- No external-agent predictions were generated by that suite run, so no real-agent patch-quality score or SWE-bench score is claimed.

Mock-agent patch runs are scaffold-only and never count as model quality.

## Timing Model

Benchmark timing is reported in explicit buckets:

- `cold_db_build_ms`
- `vector_sidecar_build_ms`
- `warm_query_ms`
- `warm_context_pack_ms`
- `rg_ms`
- `codegraph_query_subprocess_ms`
- `codegraph_context_pack_subprocess_ms`
- `codegraph_index_subprocess_ms`
- `harness_scoring_ms`
- `harness_bookkeeping_ms`
- `harness_overhead_ms`
- `raw_total_ms`
- `cold_setup_excluded_total_ms`
- `end_to_end_first_use_ms`

Reports must explain raw end-to-end timing and warm/cold-adjusted timing
separately. The setup phase prebuilds one DB per unique repo where CodeGraph
providers are part of the run, then timed provider retrieval uses those
artifacts. Provider timing must say whether cold setup is paid by that task or
amortized.

Index profiling is downstream of truthful timing accounting. Do not start index
profiling work until the suite can separate cold setup, warm retrieval,
subprocess cost, and harness overhead.

## Quality Per Budget

The suite emits `quality_per_budget.json` with:

- recall/MRR per 1k tokens
- recall/MRR per tool call
- context bytes per recalled gold file
- warm milliseconds per recalled gold file
- recall/MRR per 1k context bytes
- files recalled per warm second
- files recalled per tool call
- precision@k, wrong-context rate, and gold density in context
- forbidden-context and context-poison metrics when the task defines forbidden
  files or symbols

If CodeGraph improves Recall@5 or MRR but spends more context, tool calls, or
wall time, the report must say so plainly. This remains local diagnostic
evidence, not a public benchmark claim.
