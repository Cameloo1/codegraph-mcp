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

Run setup verification before any dedicated benchmark run:

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

### SWE-bench Lite

Status: source checkout pinned, editable package install succeeded in the
ignored benchmark venv, Docker Desktop is reachable, Lite dataset access was
verified, and the official harness completed one Linux-container gold validation
for `sympy__sympy-20590`.

Gold validation command:

```powershell
benchmarks/tracks/swebench_lite/scripts/run_harness_linux_container.ps1
```

Patch scoring still needs a real external agent command.

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

Mock-agent patch runs are scaffold-only and never count as model quality.
