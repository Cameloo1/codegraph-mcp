# SWE-bench Readiness

CodeGraph is being built toward SWE-bench-grade evaluation discipline. That
does not mean CodeGraph has an official SWE-bench score.

CodeGraph is a context/retrieval/trust layer. For SWE-bench-family evaluation,
the right product comparison is:

```text
same task set
same model
same agent scaffold
same budget
same evaluator
same environment
Mode A: same agent + normal rg/search/edit/test tools
Mode B: same agent + normal rg/search/edit/test tools + CodeGraph
```

## Goal

The goal is to answer:

```text
Does the same configured coding agent solve more tasks, make fewer wrong-file
edits, reference fewer nonexistent symbols, and use context more efficiently
when CodeGraph is added to normal rg use?
```

The answer must come from real patch runs through an official-compatible
evaluation path. Harness setup, mock-agent plumbing, and gold-patch validation
are prerequisites, not quality scores.

## Current Status

| Surface | Status |
|---|---|
| SWE-bench source checkout | pinned |
| SWE-bench Lite dataset access | verified |
| Docker/Linux harness route | working |
| Gold-validation smoke | passed for `sympy__sympy-20590` |
| External-agent patch generation | working for `baseline` and `rg_only` on one task |
| Docker patch evaluation | working for `baseline` and `rg_only` on one task |
| CodeGraph patch-quality ablation | not measured; CodeGraph context attribution blocked |
| Mock-agent scaffold | available, non-quality only |
| Official SWE-bench score | not claimed |

## What Passed

The Linux-container harness route completed a SWE-bench Lite gold-validation
smoke for `sympy__sympy-20590`:

| Item | Result |
|---|---:|
| Total instances | 1 |
| Completed instances | 1 |
| Resolved instances | 1 |
| Error instances | 0 |

This proves the local harness path can evaluate a known gold patch through the
Linux container route. It does not prove CodeGraph improves agent patch quality.

## Patch Quality Requirements And Remaining Gate

Any patch-quality scoring needs a real external agent/model command, configured
explicitly, for example through:

```powershell
$env:CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND = "<fixed agent command>"
```

For this repository's Codex CLI path, use the saved wrapper:

```powershell
$env:CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND = "powershell -NoProfile -ExecutionPolicy Bypass -File benchmarks/scripts/run_codex_external_patch_agent.ps1"
```

The wrapper resolves `codex.cmd` before the PowerShell shim, reads benchmark
JSON from stdin, runs `codex exec` non-interactively in the task workspace, and
prints only a `diff --git` patch to stdout. Use `-ValidateOnly` to check
readiness without making a model call.

Without that configured, the benchmark layer can run setup checks and
scaffold-only mock-agent flows, but it cannot claim model quality, solved task
rate, or SWE-bench improvement.

The local Codex wrapper is now sufficient for one-task diagnostic runs, but
CodeGraph patch-quality is still blocked until a CodeGraph context packet is
valid for attribution. A CodeGraph mode must produce usable context, run the
same external agent, and be evaluated through the same SWE-bench harness before
it counts as CodeGraph patch-quality evidence.

## Latest Patch-Quality Smoke

On 2026-05-23, the local official-compatible one-task smoke
`swebench_official_compatible_smoke_20260523_174023` ran real Codex external
agent predictions for `sympy__sympy-20590` and evaluated them through the local
SWE-bench Lite harness.

| Mode | Resolved | Clean source patch | Reason |
|---|---:|---:|---|
| `baseline` | true | false | extra test-file edit |
| `rg_only` | true | false | extra test-file edit |

Both modes edited `sympy/core/_print_helpers.py` and also edited
`sympy/core/tests/test_symbol.py`. This is useful local patch-quality evidence,
but it is not an official SWE-bench score and does not prove CodeGraph value.
CodeGraph modes remain gated until their context packets are valid for
attribution.

The later E2E diagnostic `swebench_lite_e2e_20260523_192718` requested
`baseline`, `rg_only`, `codegraph_exact_text`, and `codegraph_full` for the
same task.

| Mode | Agent ran | Docker eval | Resolved | Context valid for attribution | Clean source patch |
|---|---:|---:|---:|---:|---:|
| `baseline` | yes | yes | true | yes | false |
| `rg_only` | yes | yes | true | yes | false |
| `codegraph_exact_text` | no | no | n/a | no | false |
| `codegraph_full` | no | no | n/a | no | false |

The CodeGraph modes were skipped before agent execution because the shared
CodeGraph prebuild hit the 240 s index timeout and the staged candidate context
did not return `_print_helpers.py`:

```text
blocked_index_timeout; candidate_spool_present_but_no_gold_hit
```

That skip is intentional fail-fast behavior. It prevents an independent agent
patch from being mislabeled as a CodeGraph-attributed result.

### Recreate The One-Task E2E Smoke

Run from a normal user PowerShell in the benchmark lab worktree. Docker-backed
SWE-bench commands may fail from the Codex sandbox identity because Docker
Desktop named pipes are not reachable there.

```powershell
cargo build --release --bin codegraph-mcp

$env:CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND = "powershell -NoProfile -ExecutionPolicy Bypass -File benchmarks/scripts/run_codex_external_patch_agent.ps1 -StationMode always"

powershell -NoProfile -ExecutionPolicy Bypass -File benchmarks/scripts/run_codex_external_patch_agent.ps1 -ValidateOnly -StationMode never

$runId = Get-Date -Format "yyyyMMdd_HHmmss"

python -m benchmarks.harness.runners.run_swebench_patch_smoke `
  --output-dir "benchmarks/results/summaries/swebench_lite_e2e_$runId" `
  --task-fixture benchmarks/tracks/swebench_lite/fixtures/swebench_lite_sympy_20590.json `
  --modes baseline rg_only codegraph_exact_text codegraph_full `
  --codegraph-context-timeout-s 240 `
  --agent-timeout-s 900 `
  --eval-timeout-s 1200 `
  --max-process-tree-rss-mib 12288 `
  --min-system-available-mib 3072
```

For CodeGraph debugging without spending model or Docker time:

```powershell
$runId = Get-Date -Format "yyyyMMdd_HHmmss"

python -m benchmarks.harness.runners.run_swebench_patch_smoke `
  --output-dir "benchmarks/results/summaries/swebench_context_probe_$runId" `
  --task-fixture benchmarks/tracks/swebench_lite/fixtures/swebench_lite_sympy_20590.json `
  --modes codegraph_exact_text codegraph_full `
  --skip-agent `
  --skip-eval `
  --codegraph-context-timeout-s 240
```

Important local outputs:

- `summary.json` and `summary.md` - phase gates and per-mode results.
- `commands.jsonl` - exact argv, timings, exit codes, and resource guard data.
- `patches/` and `predictions/` - generated patch and SWE-bench prediction
  files for measured agent modes.
- `swebench_eval/` - Docker evaluation summaries and per-task reports.
- `external_patch_agent_logs/` - wrapper prompt, status, stdout JSONL, stderr,
  last message, and station launch data.

### Current Troubleshooting Meanings

- `blocked_index_timeout`: CodeGraph context prebuild exceeded the fixed
  context-prep timeout.
- `candidate_spool_present_but_no_gold_hit`: partial candidate context existed
  but did not return the gold source file.
- `skipped_invalid_context`: the runner correctly skipped the agent to avoid
  false CodeGraph attribution.
- `resource_limit`: the resource guard killed a command because process-tree
  or host-memory limits were exceeded; this is a hard failure, not a timeout.

## Official-Compatible Requirements

A SWE-bench-family result can only be described as official-compatible when all
of these are true:

- the upstream harness is pinned and used for scoring;
- the dataset and split are pinned;
- Docker/image dependencies are recorded;
- predictions come from a real configured agent/model run;
- the same agent, model, budgets, and evaluator are used across context modes;
- raw predictions, logs, and run metadata are retained;
- no official scoring code is modified;
- skipped or failed tasks are reported.

Anything less is local diagnostic evidence.

## Planned Progression

The intended path is deliberately incremental:

1. Gold-validation smoke. Complete for `sympy__sympy-20590`.
2. One real external-agent SWE-bench Lite smoke. Complete for `baseline` and
   `rg_only`, with clean-source-patch gate failing due to extra test-file edits.
3. One attributable CodeGraph SWE-bench Lite smoke for the same task. Not yet
   complete; current blocker is CodeGraph context attribution.
4. 10-task SWE-bench Lite diagnostic subset.
5. 25-task and 50-task Lite diagnostic subsets.
6. Full SWE-bench Lite diagnostic run when cost and runtime are understood.
7. SWE-bench Verified only after Lite runs are boring and reproducible.
8. SWE-bench-Live later for contamination-resistant current tasks.

At each step, CodeGraph should be compared as an added reliability layer:

- rg-only agent;
- rg + CodeGraph exact/text/routing agent;
- rg + CodeGraph full retrieval stack agent;
- later, rg + CodeGraph MVP4 micro-flow proof packets.

## Claim Boundaries

Safe wording:

```text
The SWE-bench Lite harness path is ready for gold validation.
```

```text
On this pinned local diagnostic subset, the same configured agent solved A
tasks with one context provider and B tasks with another.
```

```text
On this pinned diagnostic subset, the same rg-using agent made fewer wrong-file
edits with CodeGraph enabled than without CodeGraph.
```

Unsafe wording:

```text
CodeGraph gets X% on SWE-bench.
```

```text
CodeGraph beats SWE-bench, rg, or CodeGraphContext.
```

```text
Mock-agent runs show quality.
```

Those claims require evidence that does not exist yet.
