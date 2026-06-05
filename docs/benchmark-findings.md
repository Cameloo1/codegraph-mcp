# Current Benchmark Findings

These findings summarize the latest local diagnostic benchmark sweep. They are
not public benchmark results, not official leaderboard numbers, and not a
CodeGraph-over-rg claim.

Current MVP2 closure evidence supersedes this historical clean-sweep summary
for public-claim decisions: release comprehensive is clean, the current clean
aggregate is complete local diagnostic evidence, Benchmark v1 real-agent patch
ladders are `blocked_external_complete_actionable`, and `public_claim=false`.

The important framing is:

```text
v0/v0.5 measure component behavior.
v1 must measure rg-only agent vs rg + CodeGraph agent.
```

## Latest Clean Sweep

Source report:

- `reports/final/full_e2e_benchmark_three_run_latest.md`
- `reports/final/full_e2e_benchmark_three_run_latest.json`

The latest clean sweep is a three-run local diagnostic sweep. It ran smoke,
full v0, v0.5 internal, v0.5 external, and a SWE-bench-focused suite alias. It
recorded:

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
| `rg_only` | 0.528 | 0.482 | 0.261 | 135,815 | 595 |
| `codegraph_full` | 0.454 | 0.358 | 0.214 | 54,342 | 1,098 |
| `codegraph_exact_text` | 0.138 | 0.127 | 0.044 | 44,689 | 1,018 |
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
| `rg_planned` | 0.692 | 0.828 | 0.360 | 44,250 | 758 |
| `codegraph_current` | 0.483 | 0.577 | 0.200 | 65,276 | 1,622 |
| `codegraph_planned` | 0.492 | 0.517 | 0.200 | 91,961 | 5,424 |
| `rg_only` | 0.483 | 0.541 | 0.200 | 403,859 | 719 |

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
| `rg_planned` | 0.973 | 0.500 | 0.560 | 288 | 609 |
| `codegraph_planned` | 0.573 | 0.611 | 0.310 | 60,785 | 3,508 |
| `rg_only` | 0.539 | 0.558 | 0.310 | 2,956 | 536 |
| `codegraph_current` | 0.466 | 0.362 | 0.250 | 48,927 | 888 |

Interpretation:

- `rg_planned` recovers many gold files with very low context volume on these
  local diagnostic tasks.
- `codegraph_planned` improves over `codegraph_current` on external local
  Recall@5/MRR, but at much higher warm cost.
- The next CodeGraph performance target is batching `query`/`context-pack` or
  using a warm server/MCP path.

## SWE-bench Status

Older SWE-bench Lite setup evidence remains harness-readiness history only. The
current v1 evidence is more precise: the external-agent wrapper validates when
configured for the run, host/approved Docker and live gold validation are ready,
the production `agent-use` profile is claimable through the external DB, and
checkout identity was verified under the normal user/approved host probe. The
patch ladder is still blocked before real patch execution because the one-task
CodeGraph B-arm context prebuild timed out at 1800s and stayed
non-claimable/non-attributable; the 5/10 task subsets are also not
materialized.

Patch-quality scoring still requires actual external-agent predictions
evaluated through the SWE-bench harness. No current CodeGraph-attributed patch
outcome, official-compatible multi-task score, public benchmark result, or
real-agent patch-quality claim exists. Setup readiness, mock-agent runs,
cached/live gold-validation runs, and blocked preflights are not patch-quality
scores.

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
