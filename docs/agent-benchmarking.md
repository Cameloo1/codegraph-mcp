# Agent Benchmarking

CodeGraph is benchmarked as agent infrastructure, not as the coding agent or
the model. The question is:

```text
Does the same agent, on the same task set and budget, produce better grounded
work when CodeGraph supplies the context layer?
```

That means serious comparisons must keep the task set, model, agent scaffold,
time budget, token/tool budget, evaluator, and environment constant. The only
thing that changes is the context provider.

## Context Provider Modes

The benchmark harness currently uses these retrieval modes:

| Mode | Meaning |
|---|---|
| `baseline` | No retrieval context. |
| `rg_only` | Bounded literal ripgrep search. Text matches are not graph proof. |
| `codegraph_exact_text` | CodeGraph exact symbols, file/text queries, Stage 0 text evidence, and routing packet behavior with vector and nuance lanes disabled. |
| `codegraph_full` | CodeGraph exact/text/routing plus enabled vector and nuance candidate lanes where configured. |

The current `rg_only` mode is intentionally simple. It uses bounded literal
content search and does not yet model a strong human `rg` workflow with
`rg --files`, path search, `rg -l`, query expansion, or manual follow-up. That
stronger baseline is planned because raw `rg` is the right bar for fast exact
developer search.

## What The Harness Measures

The retrieval track measures whether a context provider returns known useful
files near the top of the packet.

| Metric | Meaning |
|---|---|
| Recall@k | Share of known gold files returned in the first `k` files. |
| MRR | How early the first correct file appears. |
| Context bytes | Approximate amount of context emitted by the provider. |
| Tool calls | Retrieval calls needed for the task. |
| Claimability violations | Cases where candidate/text evidence is incorrectly treated as proof. |
| Unsupported-claim violations | Cases where a result claims unsupported proof strength. |

Patch-outcome evaluation is separate. It requires a real external agent command
and, for SWE-bench-family results, official-compatible harness evaluation.
Mock-agent runs are scaffold checks only.

## Why SWE-bench Matters

SWE-bench-style evaluation is valuable because it tests complete issue
resolution through real repositories and test harnesses. CodeGraph's goal is
not to become a SWE-bench agent. The goal is to become a context layer that can
be compared under SWE-bench-grade discipline:

- same model
- same agent scaffold
- same tasks
- same budgets
- same evaluator
- same environment
- only the context provider changes

An official SWE-bench score requires real model/agent predictions evaluated by
the official-compatible harness. CodeGraph does not claim such a score.

## What CodeGraph Should Prove

The benchmark layer should show whether CodeGraph helps an agent:

- retrieve the right files and symbols;
- avoid wrong-file edits;
- avoid nonexistent-symbol references;
- respect proof and claimability boundaries;
- use fewer blind follow-up calls;
- keep context bounded enough for agent loops;
- preserve `unknown` when evidence is missing.

## What CodeGraph Must Not Claim

The following are not valid claims from the current diagnostic runs:

- CodeGraph gets a SWE-bench score.
- CodeGraph beats `rg`.
- CodeGraph beats CodeGraphContext.
- CodeGraph improves real-world recall.
- Text evidence proves graph behavior.
- Vector, binary, or nuance candidates are graph proof.

Valid wording is narrower:

```text
On this pinned local diagnostic retrieval subset, this context provider returned
these gold files with this recall and ranking.
```

## Running Local Diagnostics

The benchmark setup and scoring commands live under `benchmarks/`. Use setup
verification before scoring:

```powershell
python -m benchmarks.harness.runners.verify_benchmark_setup --output-dir benchmarks/results/summaries/setup_verification_local
```

Then run the configured retrieval suites from the benchmark configs. Generated
results, DBs, logs, upstream checkouts, and raw payloads are local/ignored by
default.

Canonical benchmark definitions now live under `benchmarks/tracks/<track>/`.
For example:

```powershell
python -m benchmarks.harness.runners.run_retrieval_eval --config benchmarks/tracks/internal_gold/configs/smoke.toml
python -m benchmarks.harness.runners.run_retrieval_eval --config benchmarks/tracks/repobench/configs/small.toml
python -m benchmarks.harness.runners.run_retrieval_eval --config benchmarks/tracks/crosscodeeval/configs/small.toml
```

Legacy configs under `benchmarks/configs/` remain compatibility aliases during
the migration window.
