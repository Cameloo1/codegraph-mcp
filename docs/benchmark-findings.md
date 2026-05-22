# Current Benchmark Findings

These findings summarize the latest local diagnostic scoring pass. They are not
public benchmark results and not official leaderboard numbers.

CodeGraph was evaluated as a context/retrieval/trust layer. The benchmark held
the task set, evaluator, scaffold, and budgets constant while varying the
context provider:

- `baseline`
- `rg_only`
- `codegraph_exact_text`
- `codegraph_full`

## Headline

Raw `rg` remains excellent for exact literal search. CodeGraph shows value when
tasks need path-aware retrieval, structured packets, provenance labels, and
claimability boundaries.

The current `rg_only` provider is a bounded literal-content baseline. It does
not yet represent an expert human workflow with `rg --files`, path search,
`rg -l`, or manual query refinement.

## Current CodeGraph Scores

Latest local diagnostic scores from `full_run_20260521_141313`:

| Track | Best CodeGraph result | Boundary |
|---|---|---|
| Internal 20-task retrieval | `codegraph_full`: **52.6% Recall@5**, **0.425 MRR** | local custom harness |
| RepoBench configured subset | `codegraph_full`: **23.1% Recall@5**, **0.225 MRR** | diagnostic 20-row subset, not official RepoBench |
| CrossCodeEval configured subset | `codegraph_full`: **61.1% Recall@5**; `codegraph_exact_text`: **0.652 MRR** | diagnostic retrieval subset, not official generation scoring |
| SWE-bench Lite harness | **1/1 gold-validation smoke passed** for `sympy__sympy-20590` | harness readiness only, not agent/model quality |

These scores are worth showing because they are reproducible local
product-ablation evidence. They are not public benchmark claims.

## Internal Retrieval Ablation

The internal 20-task set covers Buildroot-style text evidence and CodeGraph
self-use tasks such as DB lifecycle, vector accounting, PathEvidence hydration,
no-proof fallback, language coverage, bundle import safety, read-only sidecars,
nuance rescue, bounded graph traversal, and agent JSON output.

| Mode | Tasks | Recall@5 | MRR | Avg context bytes | Avg tool calls | Avg ms/task | Claim violations |
|---|---:|---:|---:|---:|---:|---:|---:|
| `baseline` | 20 | 0.000 | 0.000 | 0.0 | 0.00 | 0.0 | 0 |
| `rg_only` | 20 | 🟢 **0.575** | 🔴 **0.334** | 32714.2 | 3.00 | 61.6 | 0 |
| `codegraph_exact_text` | 20 | 0.456 | 🟢 **0.373** | 48842.8 | 3.45 | 3208.8 | 0 |
| `codegraph_full` | 20 | 🟡 **0.526** | 🟢 **0.425** | 53719.6 | 3.45 | 4017.2 | 0 |

Interpretation:

- `rg_only` had the best Recall@5 on this internal set because many tasks
  include exact literal hooks such as `FOO_SITE`, `generic-package`,
  `Config.in`, `PathEvidence`, and `no_proof_path_found`.
- `codegraph_full` ranked the first useful hit better than `rg_only` by MRR.
- CodeGraph packets cost more time and bytes in this run, but preserve
  structured context and evidence boundaries.

## RepoBench-Style Retrieval

The configured RepoBench subset uses 20 materialized Python v1.1 rows. This is a
local product-ablation diagnostic, not an official full RepoBench score.

RepoBench configured subset note: `rg_only` is 0 because this bounded provider
does literal content search only; it does not use path search, `rg -l`, or
expert follow-up for repository-context rows.

| Mode | Tasks | Recall@5 | MRR | Avg context bytes | Avg tool calls | Avg ms/task | Claim violations |
|---|---:|---:|---:|---:|---:|---:|---:|
| `baseline` | 20 | 0.000 | 0.000 | 0.0 | 0.00 | 0.0 | 0 |
| `rg_only` | 20 | 0.000 | 0.000 | 0.0 | 4.35 | 65.0 | 0 |
| `codegraph_exact_text` | 20 | 0.214 | 0.175 | 25528.5 | 4.00 | 1760.7 | 0 |
| `codegraph_full` | 20 | 0.231 | 0.225 | 27779.5 | 4.00 | 1758.9 | 0 |

Interpretation:

- The current literal `rg_only` provider returned no gold files on this subset.
- CodeGraph recovered some gold files through indexed file/path/text surfaces.
- This does not prove CodeGraph beats expert `rg`; it proves CodeGraph beats the
  bounded literal-content baseline on this diagnostic subset.

## CrossCodeEval-Style Retrieval

The configured CrossCodeEval subset uses 20 tasks from the extracted
CrossCodeEval data. Python, Java, TypeScript, and C# data are available in the
local setup, but this configured 20-task run is a retrieval-context diagnostic
slice, not a language-balanced run and not full official generation scoring.

CrossCodeEval configured subset note: `rg_only` is 0 because these tasks need
cross-file dependency/context recovery, while the current provider only tries
bounded literal content probes.

| Mode | Tasks | Recall@5 | MRR | Avg context bytes | Avg tool calls | Avg ms/task | Claim violations |
|---|---:|---:|---:|---:|---:|---:|---:|
| `baseline` | 20 | 0.000 | 0.000 | 0.0 | 0.00 | 0.0 | 0 |
| `rg_only` | 20 | 0.000 | 0.000 | 0.0 | 5.95 | 83.8 | 0 |
| `codegraph_exact_text` | 20 | 0.586 | 0.652 | 45157.2 | 4.00 | 1774.0 | 0 |
| `codegraph_full` | 20 | 0.611 | 0.454 | 47816.1 | 4.00 | 1796.2 | 0 |

Interpretation:

- Cross-file retrieval often depends on related module/file identity, not a
  literal term in file contents.
- CodeGraph's file/path/text index performed better than literal-content
  search here.
- `codegraph_exact_text` had higher MRR than `codegraph_full`, which means
  candidate lanes can improve recall while still needing better ranking.

## Trust Metrics

Across the scored retrieval tracks:

| Metric | Result |
|---|---:|
| Claimability violations | 0 |
| Unsupported-claim violations | 0 |

This matters because CodeGraph's benchmark target is not only recall. The
system must also avoid promoting text evidence, vector candidates, binary
candidates, or nuance candidates into graph proof.

## SWE-bench Lite Harness Status

SWE-bench Lite gold validation ran through the Linux-container harness route for
`sympy__sympy-20590`.

| Item | Result |
|---|---:|
| Total instances | 1 |
| Completed instances | 1 |
| Resolved instances | 1 |
| Error instances | 0 |

This is harness readiness evidence. It is not an agent/model quality result.
Patch-quality ablations still require a configured real external agent command.

## What The Results Actually Say

The current evidence supports these statements:

- CodeGraph can run local retrieval ablations against internal, RepoBench-style,
  and CrossCodeEval-style tasks.
- CodeGraph preserves claimability and unsupported-claim boundaries in these
  runs.
- CodeGraph provides value beyond literal grep when tasks depend on path-aware
  or cross-file retrieval.
- `rg` remains a strong exact-search baseline and should be tested again with a
  stronger human-style workflow.

The current evidence does not support these statements:

- CodeGraph has an official SWE-bench score.
- CodeGraph improves patch success rate.
- CodeGraph beats `rg`.
- CodeGraph beats CodeGraphContext.
- CodeGraph has real-world recall.

## Planned Benchmark Improvements

The next retrieval benchmark should compare CodeGraph against a stronger
`rg` workflow:

- `rg --files` for path/name search;
- `rg -l` for file-level content hits;
- `rg -n` for line evidence;
- path-normalized and case-insensitive variants where appropriate;
- deterministic query expansion from task terms.

CodeGraph should also use its own primitives more intelligently:

- route path-looking terms to file/path search;
- route identifier-looking terms to symbol search;
- route prose/docs/config tasks to text evidence;
- use implementation-trace packets for accounting and source-navigation tasks;
- rank exact path/symbol/text hits before candidate-only vector or nuance lanes.
