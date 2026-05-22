# SWE-bench Readiness

CodeGraph is being built toward SWE-bench-grade evaluation discipline. That
does not mean CodeGraph has an official SWE-bench score.

CodeGraph is a context/retrieval/trust layer. For SWE-bench-family evaluation,
the right comparison is:

```text
same task set
same model
same agent scaffold
same budget
same evaluator
same environment
different context provider
```

## Goal

The goal is to answer:

```text
Does the same configured coding agent solve more tasks, make fewer wrong-file
edits, reference fewer nonexistent symbols, and use context more efficiently
when CodeGraph is enabled?
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
| Patch-quality ablation | blocked until a real external agent command is configured |
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

## What Is Still Missing

Patch-quality scoring needs a real external agent/model command, configured
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

Until that is configured, the benchmark layer can run setup checks and
scaffold-only mock-agent flows, but it cannot claim model quality, solved task
rate, or SWE-bench improvement.

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

1. Gold-validation smoke.
2. One real external-agent SWE-bench Lite smoke.
3. 10-task SWE-bench Lite diagnostic subset.
4. 25-task and 50-task Lite diagnostic subsets.
5. Full SWE-bench Lite diagnostic run when cost and runtime are understood.
6. SWE-bench Verified only after Lite runs are boring and reproducible.
7. SWE-bench-Live later for contamination-resistant current tasks.

At each step, CodeGraph should be compared as a context provider:

- no context;
- strong `rg` workflow;
- CodeGraph exact/text/routing;
- CodeGraph full retrieval stack.

## Claim Boundaries

Safe wording:

```text
The SWE-bench Lite harness path is ready for gold validation.
```

```text
On this pinned local diagnostic subset, the same configured agent solved A
tasks with one context provider and B tasks with another.
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
