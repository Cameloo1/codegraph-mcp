# CodeGraph Benchmark Layer

This directory contains benchmark harness code for measuring CodeGraph as a
context, retrieval, and trust layer for coding agents.

CodeGraph is not benchmarked as the agent and is not benchmarked as an `rg`
replacement. Component diagnostics keep the model, agent scaffold, task set,
budget, evaluator, and environment fixed, then vary the retrieval/context
provider:

- `baseline`
- `rg_only`
- `rg_planned` (v0.5 planned baseline; local diagnostic comparison complete)
- `codegraph_exact_text`
- `codegraph_full`
- `codegraph_planned` (v0.5 planned CodeGraph provider; local diagnostic comparison complete)

The product benchmark is stricter:

```text
Mode A: same agent + normal rg/search/edit/test tools
Mode B: same agent + normal rg/search/edit/test tools + CodeGraph
```

v0/v0.5 results are provider diagnostics. v1 is the first layer that can answer
whether CodeGraph improves agent reliability on top of normal `rg` use.

Provider input is sanitized before a provider runs. Providers may see task text,
visible prompts, visible file/symbol/text hints, stack frames, error messages,
explicit visible config keys, and bounded budget fields. Providers must not see
gold files, gold symbols, expected files, forbidden files/symbols, oracle patch
data, answer metadata, or evaluator notes. The runner writes the sanitized
provider input and leakage audit beside each per-task result.

RepoBench and CrossCodeEval results produced before this sanitizer are
`gold_hint_diagnostic` artifacts when provider query terms were derived from
gold context. Keep them for path/symbol-resolution diagnostics only; do not mix
them with clean retrieval summaries.

The default operator entry point is the durable benchmark suite:

```powershell
python -m benchmarks.harness.runners.run_benchmark_suite --suite full --output-dir benchmarks/results/summaries/<run_id>
```

For a fast local diagnostic smoke that verifies setup readiness, blocked-track
reporting, command logging, artifact hygiene, summary aggregation, and the
internal retrieval path without running SWE-bench patch quality:

```powershell
python -m benchmarks.harness.runners.run_benchmark_suite --suite smoke --output-dir benchmarks/results/summaries/<run_id>
```

The suite writes all operator artifacts under the output directory:

- `run_plan.json`
- `all_results.json`
- `summary.json`
- `summary.md`
- `commands.jsonl`
- `blocked_tracks.json`
- `timing_breakdown.json`
- `quality_per_budget.json`
- `track_artifacts_manifest.json`
- `charts/`

Focused commands remain available for targeted debugging:

```powershell
python -m benchmarks.harness.runners.run_smoke_suite --quick
python -m benchmarks.harness.runners.run_benchmark_suite --suite retrieval --output-dir benchmarks/results/summaries/<run_id>
python -m benchmarks.harness.runners.run_benchmark_suite --suite swebench-focused --output-dir benchmarks/results/summaries/<run_id>
```

Setup verification for external benchmark targets:

```powershell
python -m benchmarks.harness.runners.verify_benchmark_setup --output-dir benchmarks/results/summaries/setup_verification_local
```

Generated results, workspaces, raw logs, predictions, patches, cloned upstream
repos, DBs, WAL/SHM files, and model outputs are ignored/local by default.

## Track Layout

Tracked benchmark definitions now live under dedicated track homes:

```text
benchmarks/tracks/internal_gold/
benchmarks/tracks/repobench/
benchmarks/tracks/crosscodeeval/
benchmarks/tracks/swebench_lite/
benchmarks/tracks/graph_truth/
benchmarks/tracks/openevolve/
```

Each track owns its `configs/`, fixtures or datasets, setup scripts, local
upstream checkout area, generated workspaces, and generated results. Shared
provider, scoring, runner, and schema code stays in `benchmarks/harness/`.
Legacy `benchmarks/configs/*.toml` and `benchmarks/scripts/*` entrypoints are
kept as thin compatibility aliases/wrappers for one migration window.

## Current Setup Shape

- Internal gold retrieval tasks are ready and deterministic.
- RepoBench source and a real Python v1.1 Hugging Face export are present under
  ignored benchmark workspaces; smoke and small retrieval runs can run.
- CrossCodeEval source, extracted JSONL data, and Linux-built tree-sitter
  parser libraries are present; retrieval-context runs and parser-load smoke can
  run. Full generation quality still needs a configured model/inference path.
- SWE-bench source, package install, Docker Desktop, Lite dataset access, and a
  current Linux-container gold validation smoke are verified when run from the
  normal user shell. The Codex CLI external-agent wrapper validates and
  `run_patch_eval` reports setup `ready` when
  `CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND` is set. The Codex sandbox identity
  still cannot access Docker named pipes, so Docker-backed checks must run from
  a normal user/elevated shell or an approved unsandboxed command. The first
  local one-task external-agent smoke has run for `baseline` and `rg_only`, but
  it is still local diagnostic evidence only.

## Setup Commands

Use the ignored benchmark venv for external setup:

```powershell
python -m venv benchmarks/tracks/repobench/workspaces/benchmark-setup-venv
benchmarks/tracks/repobench/workspaces/benchmark-setup-venv/Scripts/python.exe -m pip install datasets
python -m pip install -e benchmarks/tracks/swebench_lite/upstream/SWE-bench
```

RepoBench Python v1.1 export:

```powershell
benchmarks/tracks/repobench/scripts/setup_repobench.ps1 -Rows 20
```

CrossCodeEval parser build:

```powershell
benchmarks/tracks/crosscodeeval/scripts/setup_docker.ps1
```

The Docker setup pins grammar repositories to 0.20-era tags before building so
the generated parser libraries match CrossCodeEval's `tree_sitter==0.20.4`
API. Native MSVC and WSL scripts are also available under
`benchmarks/tracks/crosscodeeval/scripts/`.

SWE-bench Lite gold validation through a Linux container:

```powershell
benchmarks/tracks/swebench_lite/scripts/run_harness_linux_container.ps1
```

This validates the official harness path for one gold task. On 2026-05-23,
`codegraph-live-gold-20260523-083023` completed 1/1 instances and resolved
`sympy__sympy-20590` under Docker Desktop from the normal user shell. It is not
a model quality result.

Patch-quality runs require an explicit external agent command. Missing
`CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND` is a blocked track, not a green result:

```powershell
$env:CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND = "<fixed agent command>"
python -m benchmarks.harness.runners.run_patch_eval --config benchmarks/tracks/swebench_lite/configs/patch_runner_external_agent.example.toml
```

For the local Codex CLI wrapper, use:

```powershell
$env:CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND = "powershell -NoProfile -ExecutionPolicy Bypass -File benchmarks/scripts/run_codex_external_patch_agent.ps1"
python -m benchmarks.harness.runners.run_patch_eval --config benchmarks/tracks/swebench_lite/configs/codex_external_agent.example.toml
```

On 2026-05-23 the same wrapper command validated, `run_patch_eval` reported
`status=ready`, and the SWE-focused operator suite completed its live gold
validation and patch-quality setup tracks under
`benchmarks/results/summaries/swebench_live_unblock_20260523_083023`. No real
external-agent patch predictions were generated by that suite run, so no patch
quality score or SWE-bench score is claimed.

Latest one-task patch-quality smoke:

- Run id: `swebench_official_compatible_smoke_20260523_174023`
- Task: `sympy__sympy-20590`
- Modes: `baseline`, `rg_only`
- Result: both modes generated real external-agent patches and resolved under
  the local SWE-bench Lite harness.
- Clean-source-patch gate: both modes failed because the patches also edited
  `sympy/core/tests/test_symbol.py`.
- Claim boundary: local one-task diagnostic only; no public SWE-bench score,
  no CodeGraph attribution, and no CodeGraph-over-rg claim.

CodeGraph patch modes remain gated: a CodeGraph agent run counts only when the
context packet is valid for attribution. Current SymPy CodeGraph context prep
still blocks on cold index timeout, and staged candidate context must find
gold-relevant evidence before an agent run is attributed to CodeGraph.

The wrapper reads benchmark JSON from stdin, runs `codex exec`
non-interactively in the task workspace, and writes only a `diff --git` patch to
stdout. Raw Codex logs and prompts are written under ignored benchmark
workspaces. `-ValidateOnly` checks command resolution without making a model
call. Local runs open the Windows agent station by default; use `-StationMode
never` for noninteractive execution.

## Timing and Claim Boundaries

Suite reports separate cold setup from warm retrieval:

- cold DB build
- optional vector sidecar build
- warm query
- warm context-pack
- repeated `rg`/CodeGraph CLI subprocess cost
- harness scoring/bookkeeping/overhead
- raw end-to-end first-use timing

Cold setup is never hidden inside one task average. Cached SWE-bench gold
validation evidence may be recorded, but it is not counted as a current live
gold-validation run. Docker failures and missing external-agent commands are
blocked tracks. All benchmark outputs here are local diagnostic evidence only;
do not turn them into public claims.

Recall/MRR alone are not enough for benchmark honesty. Retrieval summaries also
include precision, non-gold distractor rates, returned file/symbol counts, gold
density, forbidden-context hits, context-poison counts/rates, first forbidden
rank, forbidden context bytes, and dangerous-context flags. If a provider
returns the right file plus harmful or distracting context, the poison and
wrong-context metrics must show it.

See:

- `benchmarks/BENCHMARKS.md`
- `benchmarks/BENCHMARK_CLAIMS.md`
- `docs/agent-reliability-benchmark-lab.md`
- `benchmarks/upstream/pinned_sources.json`
