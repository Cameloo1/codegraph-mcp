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
| SWE-bench source checkout | current harness fallback `benchmarks/upstream/SWE-bench` exists; cached SymPy checkout identity validates with local `git -c safe.directory=<checkout>` |
| SWE-bench Lite dataset access | pinned fixture exists for `sympy__sympy-20590` |
| Docker/Linux harness route | ready through unsandboxed Docker Desktop `desktop-linux`; sandbox pipe probes still report permission denial |
| Gold-validation smoke | live one-task gold validation for `sympy__sympy-20590` passed after Docker launch |
| External-agent wrapper | Codex wrapper validates with `-ValidateOnly`; real execution still needs an approved local-only route or explicit approval of provider-visible task/context exposure |
| One-task patch smoke | repo-side no-agent/no-eval preflight passed; real patch/evaluator smoke is blocked before execution |
| CodeGraph-attributed arms | no current real patch run; attribution remains zero |
| Mock-agent scaffold | available, non-quality only |
| Official SWE-bench score | not claimed |

## What Passed

Prior setup evidence showed that the Linux-container harness route can evaluate
a known SWE-bench Lite gold patch for `sympy__sympy-20590` when Docker is
available:

| Item | Result |
|---|---:|
| Total instances | 1 |
| Completed instances | 1 |
| Resolved instances | 1 |
| Error instances | 0 |

This is live setup evidence only. It does not prove CodeGraph improves agent
patch quality, and it is not a real-agent patch-quality run.

Older one-task local patch-smoke evidence for `baseline` and `rg_only` remains
local diagnostic history. It does not describe the current v1 patch-ladder
state: the current gate did not execute patch tasks because external-agent
execution still needs an approved route or local-only replacement.

## What Is Still Missing

Patch-quality scoring now needs an approved external-agent route and then
same-agent A/B runs with reliable context prep for the CodeGraph-attributed
arms. The current readiness and SWE-bench Lite smoke
configs declare the saved Codex wrapper as the canonical external-agent command:

```powershell
python -m benchmarks.harness.v1_readiness --output-dir reports/audit/artifacts/resolve_mvp2_closure_blockers/benchmark_v1_external_preflight/v1_readiness
python -m benchmarks.harness.runners.run_patch_eval --config benchmarks/tracks/swebench_lite/configs/smoke.toml
```

The command can still be overridden explicitly for a host run:

```powershell
$env:CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND = "<fixed agent command>"
```

For this repository's current Codex CLI path, the saved wrapper command is:

```powershell
$env:CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND = "powershell -NoProfile -ExecutionPolicy Bypass -File benchmarks/tracks/swebench_lite/scripts/run_codex_external_patch_agent.ps1"
```

The saved wrapper resolves `codex.cmd` before the PowerShell shim, reads benchmark
JSON from stdin, runs `codex exec` non-interactively in the task workspace, and
prints only a `diff --git` patch to stdout. Use `-ValidateOnly` to check
readiness without making a model call.

Until CodeGraph-attributed patch arms run successfully under the same pinned
agent/model/task/evaluator setup, the benchmark layer cannot claim CodeGraph
patch-quality improvement, solved-task lift, or an official SWE-bench score.
The current blocker is the external-agent approval/local-only route boundary.

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

1. Validate the canonical external-agent wrapper command. The current preflight
   config also allows `CODEGRAPH_BENCH_EXTERNAL_AGENT_COMMAND` to override the
   wrapper when an operator supplies a different fixed agent command.
2. Docker Desktop's Linux engine is already verified ready; rerun patch setup
   only if Docker/SWE-bench state changes.
3. Use an approved local-only external agent or explicitly approve the configured
   Codex wrapper data-exposure risk.
4. Rerun the v1 external preflight until it reports Docker/evaluator ready and
   only the external-agent approval/local-only gate remains.
5. Run one real external-agent SWE-bench Lite smoke with strict same-agent A/B
   attribution checks.
6. 10-task SWE-bench Lite diagnostic subset.
7. 25-task and 50-task Lite diagnostic subsets.
8. Full SWE-bench Lite diagnostic run when cost and runtime are understood.
9. SWE-bench Verified only after Lite runs are boring and reproducible.
10. SWE-bench-Live later for contamination-resistant current tasks.

At each step, CodeGraph should be compared as an added reliability layer:

- rg-only agent;
- rg + CodeGraph exact/text/routing agent;
- rg + CodeGraph full retrieval stack agent;
- later, rg + CodeGraph MVP4 micro-flow proof packets.

## Claim Boundaries

Current safe wording:

```text
The current Benchmark v1 real-agent patch ladder is
blocked_external_agent_approval_after_docker_ready: the one-task repo-side
preflight is runnable and passed, Docker/SWE-bench setup plus live gold
validation passed, but no patch tasks ran because the external-agent route still
needs approval or a local-only replacement.
```

Future wording, only after a green preflight and real same-agent patch runs:

```text
On this pinned local diagnostic subset, the same configured agent solved A
tasks with one context provider and B tasks with another.
```

Future wording, only after measured CodeGraph-attributed real patch runs:

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
