# OpenEvolve Current Priorities

Status: experimental lab objectives for the `openevolve-lab` branch.

OpenEvolve is useful for CodeGraph only when the target is narrow, measurable,
and fenced by deterministic evaluators. It should search policy choices, not
rewrite the product.

## Current Product Baseline

The candidate-spool firehose is no longer the main problem to solve. CodeGraph
now has:

- bounded candidate-spool selection before persistence;
- compact candidate packet aggregation;
- SQLite candidate-spool query indexing;
- budget-graceful optional spool behavior;
- source-binding lifecycle checks for stale, changed, deleted, and renamed files;
- explicit candidate-only labels that cannot become graph proof;
- a 200k synthetic candidate gate with bounded query/status latency.

The latest bounded working-set gate recorded:

| Metric | Current evidence |
|---|---:|
| Generated candidate inputs | 200,000 |
| Selected candidate packets | 768 |
| Spool payload bytes | 1,465,493 |
| Query index bytes | 13,418,496 |
| Path query p95 | 15.44 ms |
| Symbol query p95 | 12.86 ms |
| Text query p95 | 6.00 ms |
| Status p95 | 5.84 ms |
| Graph proof from spool | false |

These are local readiness metrics, not public benchmark claims.

## Rebased Objectives

### 1. Larger-Corpus Replay

Replay baseline and evolved policies against a larger fixed candidate inventory.
The first tiny fixture proved the harness shape, but not real robustness.

Success means:

- required gold files and symbols remain present;
- Recall@5 and MRR do not regress;
- packet bytes and packet count stay bounded;
- query latency remains low;
- claimability and unsupported-claim violations stay at zero.

### 2. Candidate Query Ranking

Tune ranking over the current SQLite query index. The target is not "make the
spool smaller"; the target is "return the useful file/path/symbol/text packet
earlier without flooding."

Useful knobs:

- path/title priority;
- symbol priority;
- text-token priority;
- role diversity;
- per-file and per-directory diversity;
- fallback ordering when exact hits are sparse.

### 3. Planned CodeGraph Provider

Use OpenEvolve only as an idea generator for `codegraph_planned` policy:

```text
TaskIntent
RetrievalPlan
query files for path-like clues
query symbols for identifier-like clues
query text for prose/config/docs clues
candidate spool/query-index context when graph DB is not ready
role-diverse reranking
implementation_trace for source-navigation/accounting tasks
```

The product implementation must still be written and reviewed normally.

### 4. Strong rg Baseline Policy

Use a strong human-style `rg` baseline for fair comparison:

```text
rg --files
rg -l
rg -n
path-aware search
case/path-normalized follow-up
dedupe
flood control
bounded snippets
```

The goal is not to make `rg` look weak. The goal is to understand where
CodeGraph adds value over a serious shell-search workflow.

### 5. Runtime Vector Selection Tuning

Tune runtime vector chunk-selection weights only after planned retrieval exposes
a real quality gap. The current compact runtime sidecar is not the main blocker.

## No Longer Primary Objectives

These are now regression/tuning tasks against an existing baseline:

- reducing the old 410 MB candidate-spool firehose from scratch;
- inventing packet aggregation from scratch;
- making OpenEvolve patch broad Rust product code;
- treating an evolved policy as proof;
- merging `openevolve-lab` wholesale into `fix`.

## Promotion Rule

An OpenEvolve winner becomes useful only after:

1. replay against fixed fixtures;
2. comparison to the current baseline;
3. human/Codex review of the candidate idea;
4. manual product implementation on `fix`;
5. targeted Rust tests;
6. release-binary smoke;
7. benchmark rerun with claim boundaries intact.

OpenEvolve output is optimization evidence. CodeGraph gates decide what ships.
