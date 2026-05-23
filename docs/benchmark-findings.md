# Current Benchmark Findings

These findings summarize the latest local diagnostic benchmark sweep. They are
not public benchmark results, not official leaderboard numbers, and not a
CodeGraph-over-rg claim.

The important framing is:

```text
v0/v0.5 measure component behavior.
v1 must measure rg-only agent vs rg + CodeGraph agent.
```

## Latest Clean Sweep

Source report:

- `reports/final/full_benchmark_sweep_latest.md`
- `reports/final/full_benchmark_sweep_latest.json`

The latest clean sweep ran smoke, full v0, v0.5 internal, v0.5 external, and a
SWE-bench-focused suite alias after fixing an `rg`/Python output-flood
regression. It recorded:

- claimability violations: 0;
- unsupported-claim violations: 0;
- graph-proof overclaims: 0;
- query-leakage violations: 0;
- normal `.codegraph` mutation: false.

## Full v0 Aggregate

Full v0 aggregates 60 local diagnostic retrieval tasks across internal gold,
RepoBench-style, and CrossCodeEval-style tracks.

| Provider | Recall@5 | MRR | Precision@5 | Context bytes | Warm ms/task |
|---|---:|---:|---:|---:|---:|
| `rg_only` | 0.531 | 0.490 | 0.261 | 132,259 | 195 |
| `codegraph_full` | 0.460 | 0.356 | 0.217 | 52,496 | 1,418 |
| `codegraph_exact_text` | 0.138 | 0.127 | 0.044 | 43,038 | 1,305 |
| `baseline` | 0.000 | 0.000 | 0.000 | 0 | 0 |

Interpretation:

- `rg_only` is strong even as a bounded provider after the output/scope fixes.
- `codegraph_full` is the best current CodeGraph v0 aggregate provider.
- CodeGraph is slower than `rg` in this diagnostic and should not be framed as
  an `rg` replacement.

## v0.5 Internal Diagnostic

The v0.5 internal suite covers 10 task families, including Buildroot gold,
implementation trace, routing packet, no-proof fallback, DB lifecycle/debug,
same-name ambiguity, mock leakage, dataflow distractors, dynamic dispatch, and
large-codebase planning.

| Provider | Recall@5 | MRR | Precision@5 | Context bytes | Warm ms/task |
|---|---:|---:|---:|---:|---:|
| `rg_planned` | 0.758 | 0.925 | 0.400 | 43,903 | 433 |
| `codegraph_current` | 0.483 | 0.588 | 0.200 | 62,877 | 2,521 |
| `codegraph_planned` | 0.492 | 0.517 | 0.200 | 87,897 | 7,391 |
| `rg_only` | 0.475 | 0.553 | 0.200 | 395,999 | 327 |

Interpretation:

- `rg_planned` is currently the strongest local v0.5 retriever by Recall@5,
  MRR, and speed.
- `codegraph_planned` improves routing structure and role coverage over current
  CodeGraph behavior, but it is much slower due repeated CodeGraph subprocesses.
- This is a useful component diagnostic, not the product verdict.

## v0.5 External Diagnostic

The v0.5 external suite covers the configured RepoBench-style and
CrossCodeEval-style local diagnostic subsets after query-leakage hardening.

| Provider | Recall@5 | MRR | Precision@5 | Context bytes | Warm ms/task |
|---|---:|---:|---:|---:|---:|
| `rg_planned` | 0.973 | 0.500 | 0.560 | 288 | 145 |
| `codegraph_planned` | 0.573 | 0.611 | 0.310 | 59,078 | 3,684 |
| `rg_only` | 0.539 | 0.571 | 0.310 | 2,956 | 125 |
| `codegraph_current` | 0.466 | 0.362 | 0.250 | 47,489 | 986 |

Interpretation:

- `rg_planned` recovers many gold files with very low context volume on these
  local diagnostic tasks.
- `codegraph_planned` improves over `codegraph_current` on external local
  Recall@5/MRR, but at much higher warm cost.
- The next CodeGraph performance target is batching `query`/`context-pack` or
  using a warm server/MCP path.

## SWE-bench Status

SWE-bench Lite gold validation has completed for `sympy__sympy-20590` through
the local Linux-container route in a normal user/approved unsandboxed process.
That is harness readiness evidence only.

Patch-quality scoring still requires actual external-agent predictions
evaluated through the SWE-bench harness. Setup readiness, mock-agent runs, and
gold-validation runs are not patch-quality scores.

## What The Results Actually Say

Supported:

- The benchmark harness can run local v0/v0.5 diagnostic suites.
- Query-leakage, proof-label, and claimability boundaries are now first-class
  checks.
- `rg_planned` is a hard, realistic component baseline.
- CodeGraph planned packets add routing/proof/implementation structure, but
  current top-file quality and speed are not enough to claim product success.

Not supported:

- CodeGraph beats `rg`.
- CodeGraph has an official SWE-bench score.
- CodeGraph improves real-agent patch success.
- CodeGraph improves real-world recall.
- Candidate/text/vector/source-navigation evidence is graph proof.

## Correct Next Benchmark

The next product benchmark is not `rg vs CodeGraph`.

It is:

```text
same agent + normal rg
vs
same agent + normal rg + CodeGraph
```

That v1 benchmark must measure wrong-file edits, nonexistent-symbol references,
unsupported claims, evidence alignment, plan accuracy, patch success, time,
tokens, tool calls, and cost under the same task/model/scaffold/budget.

See [Agent Reliability Benchmark Lab](agent-reliability-benchmark-lab.md).
