# Agent Benchmarking

CodeGraph is benchmarked as agent infrastructure, not as the coding agent, the
model, or an `rg` replacement.

The product question is:

```text
Does the same rg-using coding agent produce better grounded work when
CodeGraph is added as the context, routing, and proof layer?
```

The serious comparison is therefore:

```text
Mode A: same agent + normal rg/search/edit/test tools
Mode B: same agent + normal rg/search/edit/test tools + CodeGraph
```

The task set, model, scaffold, time budget, token/tool budget, evaluator,
repository commit, environment, and scoring code must stay fixed.

See the full benchmark-lab contract in
[Agent Reliability Benchmark Lab](agent-reliability-benchmark-lab.md).

## Benchmark Layers

| Layer | Purpose | Product Verdict? |
|---|---|---|
| v0 | Internal/external retrieval diagnostics, claimability scoring, setup readiness, timing accounting. | No. Component health only. |
| v0.5 | Stronger provider diagnostics: `rg_planned` and `codegraph_planned`. | No. Provider comparison only. |
| v1 | Real agent A/B runs: rg-only agent vs rg + CodeGraph agent. | Yes, if the harness is official-compatible or explicitly diagnostic. |

v0/v0.5 are still valuable. They find retrieval regressions, query leakage,
proof-label bugs, context poison, and timing problems. They do not decide
whether CodeGraph is a useful product.

## Context Provider Modes

The current harness includes these retrieval/provider modes:

| Mode | Meaning |
|---|---|
| `baseline` / `none` | No retrieval context. |
| `rg_only` | Bounded literal ripgrep search. Text matches are not graph proof. |
| `rg_planned` | Stronger human-style rg workflow with file/path discovery, file shortlisting, line evidence, dedupe, and flood control. |
| `codegraph_exact_text` | CodeGraph exact symbols, file/text queries, Stage 0 text evidence, and routing packet behavior with vector and nuance lanes disabled. |
| `codegraph_full` / `codegraph_current` | Current CodeGraph exact/text/routing plus vector and nuance candidate lanes where configured. |
| `codegraph_planned` | TaskIntent/RetrievalPlan-driven CodeGraph provider with role-diverse routing, implementation-trace structure, and follow-up probes. |

These modes are component diagnostics. A low-level provider win is not the same
as a product win. A product win requires the agent using normal `rg` plus
CodeGraph to outperform the same agent using normal `rg` alone.

## What The Harness Measures Today

Retrieval diagnostics measure whether a provider returns known useful files and
context near the top of the packet.

| Metric | Meaning |
|---|---|
| Recall@k | Share of known gold files returned in the first `k` files. |
| MRR | How early the first correct file appears. |
| Precision@k | Share of top `k` returned items that are gold or allowed. |
| Wrong-context rate | Share of top results that are irrelevant or distracting. |
| Context bytes / tokens | Approximate amount of context emitted. |
| Tool calls | Retrieval calls needed for the task. |
| Claimability violations | Candidate/text evidence incorrectly treated as proof. |
| Unsupported-claim violations | Output claims unsupported proof strength. |

Recall@5 and MRR are not enough for CodeGraph's mission. The benchmark must
also measure whether CodeGraph reduces hallucinated plans and edits.

## What v1 Must Measure

Benchmark Layer v1 should compare:

```text
rg-only agent
rg + CodeGraph agent
```

Required metrics:

- resolved percentage;
- test pass rate;
- wrong-file edit rate;
- nonexistent-symbol reference rate;
- unsupported claim rate;
- evidence alignment;
- plan accuracy;
- affected-test coverage;
- time, tokens, tool calls, and context bytes;
- patch size;
- retry count;
- cost per solved task.

Routing-packet quality, plan-accuracy, proof-discipline, hallucination-trap,
and full-codebase complexity tests should be first-class parts of this lab.

## Why SWE-bench Matters

SWE-bench-style evaluation is valuable because it tests complete issue
resolution through real repositories and test harnesses. CodeGraph's goal is not
to become a SWE-bench agent. The goal is to become a context/retrieval/trust
layer that can be compared under SWE-bench-grade discipline.

An official SWE-bench-family score requires real model/agent predictions
evaluated by an official-compatible harness, pinned datasets, recorded Docker
dependencies, retained logs/predictions, and reported skipped/failed tasks.
CodeGraph does not currently claim such a score.

Current SWE-bench Lite pilot status: the local one-task E2E path can generate
real external-agent patches and evaluate them through Docker for `baseline` and
`rg_only` on `sympy__sympy-20590`. CodeGraph patch-quality is still not
measured because CodeGraph modes were skipped before agent execution when
context was invalid for attribution. See
[SWE-bench Readiness](swe-bench-readiness.md) for the exact run and recreate
commands.

## What CodeGraph Should Prove

CodeGraph should prove that it helps an rg-using agent:

- understand task intent and implementation surface;
- find critical files, symbols, tests, and risks;
- preserve `unknown` when evidence is missing;
- avoid wrong-file edits;
- avoid nonexistent-symbol references;
- avoid unsupported source or relation claims;
- use proof/candidate/text labels correctly;
- produce better patches or safer plans under the same budget.

## What CodeGraph Must Not Claim

The following are not valid claims from current diagnostic runs:

- CodeGraph gets a SWE-bench score.
- CodeGraph beats `rg`.
- CodeGraph beats CodeGraphContext.
- CodeGraph improves real-world recall.
- Text evidence proves graph behavior.
- Vector, binary, nuance, routing-packet, or candidate-spool evidence is graph proof.

Valid component wording is narrower:

```text
On this pinned local diagnostic retrieval subset, this provider returned these
gold files with this recall, ranking, cost, and proof-label behavior.
```

Valid product wording requires v1-style agent A/B evidence:

```text
On this pinned diagnostic patch subset, the same agent using rg + CodeGraph had
fewer wrong-file edits and better evidence alignment than the same agent using
rg alone.
```

## Running Local Diagnostics

The benchmark setup and scoring commands live under `benchmarks/`. Use the
operator-grade suite when available:

```powershell
python -m benchmarks.harness.runners.run_benchmark_suite --suite full --output-dir benchmarks/results/summaries/<run_id>
```

Generated results, DBs, logs, upstream checkouts, patches, predictions, and raw
payloads are local/ignored by default.
