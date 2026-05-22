# OpenEvolve Optimizer Plan For CodeGraph

Status: experimental design note for the `openevolve-lab` branch.

This document explains how CodeGraph can use OpenEvolve impactfully without
turning it into a product dependency, a proof source, or an automatic patch
writer.

OpenEvolve is useful here as a bounded policy-search engine. It can propose
small retrieval and ranking policy variants; CodeGraph's fixed evaluators score
those variants for recall, latency, packet size, diversity, and claim-boundary
safety. Any useful idea still requires replay, review, manual product
implementation, and normal CodeGraph gates before it can move to `fix`.

## Why This Exists

CodeGraph's strongest product direction is not "replace rg" or "be the agent."
It is:

```text
agent task
  -> bounded routing and retrieval policy
  -> claim-labeled context packet
  -> graph/source proof where available
  -> explicit unknowns where proof is unavailable
```

The old candidate-spool firehose is no longer the central problem. CodeGraph now
has a bounded, query-indexed, budget-aware candidate working set:

| Metric | Current local gate evidence |
|---|---:|
| Generated candidate inputs | 200,000 |
| Selected candidate packets | 768 |
| Spool payload bytes | 1,465,493 B |
| Query index bytes | 13,418,496 B |
| Path query p95 | 15.44 ms |
| Symbol query p95 | 12.86 ms |
| Text query p95 | 6.00 ms |
| Status p95 | 5.84 ms |
| Graph proof from spool | false |

These are local readiness metrics, not public benchmark claims.

The new question is:

```text
How should CodeGraph rank, route, and select from these bounded candidate
surfaces so agents get useful context sooner?
```

That is an OpenEvolve-shaped problem because it is narrow, measurable, and
evaluator-driven.

## What OpenEvolve Is

OpenEvolve is an evolutionary coding loop around LLMs:

```text
initial program
  -> LLM proposes diff or rewrite
  -> evaluator runs candidate
  -> metrics returned
  -> candidate stored in population/checkpoint database
  -> MAP-Elites/islands select future parents
  -> repeat
```

For CodeGraph, the evaluator is the safety boundary. OpenEvolve should generate
ideas; CodeGraph's deterministic tests decide what survives.

## What OpenEvolve Must Not Do

OpenEvolve must not:

- patch broad Rust product code directly;
- create or mutate normal `.codegraph`;
- weaken graph proof, source span, or claimability boundaries;
- drop required gold hits just to reduce bytes;
- count speed wins when recall collapses;
- write secrets, raw model logs, DBs, patches, or generated artifacts into git;
- stage, commit, push, or open PRs automatically;
- produce public benchmark claims.

## Branch And Artifact Boundaries

Use:

```text
fix
  release-ready product changes
  stable benchmark harness
  front-facing docs and claim boundaries

openevolve-lab
  experimental OpenEvolve targets
  evaluator variants
  candidate policy ideas
  sanitized experiment summaries
  replay notes

benchmarks/workspaces/**
  ignored raw runs, checkpoints, logs, DBs, patches, model outputs
```

Do not merge `openevolve-lab` wholesale into `fix`. Promote only reviewed
product ideas.

## Current Objectives

### 1. Larger-Corpus Replay

Replay the current baseline policy and evolved candidates on a larger fixed
candidate inventory.

Success criteria:

- required gold files and symbols remain present;
- Recall@5 and MRR do not regress;
- packet bytes and packet count stay bounded;
- query latency improves or stays bounded;
- claimability and unsupported-claim violations remain zero.

### 2. Candidate Query Ranking

Tune ranking over the current SQLite candidate-spool query index.

Useful policy surfaces:

- path/title priority;
- filename priority;
- symbol priority;
- text-token priority;
- source role diversity;
- per-file and per-directory diversity;
- fallback order when exact hits are sparse.

The target is not "make the spool smaller." The target is "return the useful
file/path/symbol/text packet earlier without flooding."

### 3. Planned CodeGraph Provider Policy

Use OpenEvolve only as an idea generator for `codegraph_planned`.

The planned provider should route by clue type:

```text
path-like clue       -> query files / candidate path index
identifier-like clue -> query symbols
prose/config clue    -> query text
source-navigation    -> implementation_trace
graph-ready DB       -> graph/source verification
graph-not-ready DB   -> candidate-only staged context
```

The product implementation must still be written and reviewed normally.

### 4. Strong rg Baseline Policy

Build a fair `rg_planned` baseline:

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
a real quality gap. The compact runtime vector sidecar is useful, but it is not
the main blocker right now.

## First Run To Do Next

The next useful run is not another generic evolution loop. It is a replay:

```text
baseline candidate policy
  vs
best evolved tiny-fixture candidate
  over
larger fixed candidate inventory
```

Record:

- Recall@5;
- symbol Recall@5;
- MRR;
- packet bytes;
- packet count;
- query latency;
- candidate diversity;
- missing-required count;
- claimability violations;
- unsupported-claim violations.

Hard fail if any required gold hit disappears or if candidate/text/vector
evidence is promoted to graph proof.

## Promotion Path

An evolved candidate can influence product code only through this path:

1. Replay against fixed fixtures.
2. Compare against the current baseline.
3. Inspect the candidate manually.
4. Convert the idea into a small product patch on `fix`.
5. Run targeted Rust tests.
6. Run release-binary smoke.
7. Run benchmark smoke with claim boundaries intact.
8. Commit only the reviewed product implementation.

OpenEvolve output is optimization evidence. CodeGraph gates decide what ships.

## Non-Goals

Do not use OpenEvolve first for:

- full cold-index architecture rewrites;
- schema migrations;
- production agent-use profile durability;
- real-time delta sync;
- MVP3 validation correctness;
- MVP4 proof-bearing micro-flow extraction;
- public benchmark claims.

## Relationship To MVP2, MVP3, And MVP4

MVP2 may use OpenEvolve to explore retrieval ranking and planned provider
policies. It cannot use OpenEvolve output as proof.

MVP3 may validate deterministic artifacts produced after an optimizer-informed
policy is implemented. Optimizer provenance is diagnostic metadata, not
validation evidence.

MVP4 may eventually tune retrieval over micro-flow packet handles, caps, and
ranking. It must not let OpenEvolve generate proof-bearing micro-flow facts.
Proof comes only from deterministic micro-node and micro-edge extraction with
source spans and provenance.
