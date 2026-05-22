# CodeGraph Benchmark Layer

This directory contains benchmark harness code for measuring CodeGraph as a
context, retrieval, and trust layer for coding agents.

CodeGraph is not benchmarked as the agent. Serious comparisons must keep the
model, agent scaffold, task set, budget, evaluator, and environment fixed, then
vary only the context provider:

- `baseline`
- `rg_only`
- `codegraph_exact_text`
- `codegraph_full`

The default safe entry point is the internal retrieval smoke:

```powershell
python -m benchmarks.harness.runners.run_smoke_suite --quick
```

Setup verification for external benchmark targets:

```powershell
python -m benchmarks.harness.runners.verify_benchmark_setup --output-dir benchmarks/results/summaries/setup_verification_local
```

Generated results, workspaces, raw logs, predictions, patches, cloned upstream
repos, DBs, WAL/SHM files, and model outputs are ignored/local by default.

## Current Setup Shape

- Internal gold retrieval tasks are ready and deterministic.
- RepoBench source and a real Python v1.1 Hugging Face export are present under
  ignored benchmark workspaces; smoke and small retrieval runs can run.
- CrossCodeEval source, extracted JSONL data, and Linux-built tree-sitter
  parser libraries are present; retrieval-context runs and parser-load smoke can
  run. Full generation quality still needs a configured model/inference path.
- SWE-bench source, package install, Docker Desktop, Lite dataset access, and a
  Linux-container gold validation smoke are verified. Patch quality still needs
  an explicit external agent command.

## Setup Commands

Use the ignored benchmark venv for external setup:

```powershell
python -m venv benchmarks/workspaces/benchmark-setup-venv
benchmarks/workspaces/benchmark-setup-venv/Scripts/python.exe -m pip install datasets
benchmarks/workspaces/benchmark-setup-venv/Scripts/python.exe -m pip install -e benchmarks/upstream/SWE-bench
```

RepoBench Python v1.1 export:

```powershell
benchmarks/workspaces/benchmark-setup-venv/Scripts/python.exe -c "from datasets import load_dataset; ds=load_dataset('tianyang/repobench_python_v1.1', split='cross_file_first', streaming=True); print(next(iter(ds)))"
```

CrossCodeEval parser build:

```powershell
benchmarks/scripts/setup_crosscodeeval_docker.ps1
```

The Docker setup pins grammar repositories to 0.20-era tags before building so
the generated parser libraries match CrossCodeEval's `tree_sitter==0.20.4`
API. Native MSVC and WSL scripts are also available under `benchmarks/scripts/`.

SWE-bench Lite gold validation through a Linux container:

```powershell
benchmarks/scripts/run_swebench_harness_linux_container.ps1
```

This validates the official harness path for one gold task. It is not a model
quality result.

Patch-quality runs require:

```powershell
$env:CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND = "<fixed agent command>"
python -m benchmarks.harness.runners.run_patch_eval --config benchmarks/configs/patch_runner_external_agent.example.toml
```

For the local Codex CLI wrapper, use:

```powershell
$env:CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND = "powershell -NoProfile -ExecutionPolicy Bypass -File benchmarks/scripts/run_codex_external_patch_agent.ps1"
python -m benchmarks.harness.runners.run_patch_eval --config benchmarks/configs/codex_external_agent.example.toml
```

The wrapper reads benchmark JSON from stdin, runs `codex exec`
non-interactively in the task workspace, and writes only a `diff --git` patch to
stdout. Raw Codex logs and prompts are written under ignored benchmark
workspaces. `-ValidateOnly` checks command resolution without making a model
call.

See:

- `benchmarks/BENCHMARKS.md`
- `benchmarks/BENCHMARK_CLAIMS.md`
- `benchmarks/upstream/pinned_sources.json`
