# Agent Reliability Benchmark Lab

CodeGraph should be evaluated as an agent reliability layer, not as an `rg`
replacement.

The product question is:

```text
Does the same coding agent, with normal rg still available, produce better
plans, fewer hallucinated edits, and better patches when CodeGraph is added?
```

The core comparison is therefore:

```text
Mode A: same agent + normal rg/search/edit/test tools
Mode B: same agent + normal rg/search/edit/test tools + CodeGraph
```

Everything else must stay fixed: task set, model, agent scaffold, timeout,
token/tool budget, evaluator, repository commit, Docker image, scoring code, and
reporting rules.

## 1. Benchmark Contract

The benchmark lab must not ask whether CodeGraph can out-search `rg`. `rg` is a
fast literal/path search tool and remains part of the agent's normal workflow.

CodeGraph is judged by the additional reliability it provides:

- better task diagnosis;
- more accurate implementation plans;
- fewer wrong-file edits;
- fewer nonexistent-symbol references;
- fewer unsupported claims;
- better evidence alignment;
- better use of proof/unknown labels;
- better patch outcomes under the same budget.

## 2. Component Diagnostics

Benchmark Layer v0 and v0.5 remain useful, but only as component diagnostics.

They answer:

```text
Are retrieval providers healthy?
Are visible query terms clean?
Are proof labels safe?
Are timing and context costs understood?
```

They do not answer:

```text
Is CodeGraph valuable as a product?
```

That product question belongs to real agent A/B runs.

## 3. Lab Integrity Gates

No result counts unless the harness first proves:

- provider-visible fields exclude gold/evaluator-only fields;
- query-leakage audit passes;
- old contaminated runs are labeled `gold_hint_diagnostic`;
- candidate/text/vector/source-navigation evidence is not promoted to graph proof;
- claimability and unsupported-claim violations are counted;
- command argv, stdout/stderr paths, timings, and exit codes are recorded;
- Docker, external-agent, and cached/live SWE-bench states are separated;
- generated DBs, logs, patches, predictions, and raw payloads stay ignored;
- `public_claim=false` unless a result is intentionally promoted.

## 4. Routing-Packet Quality Tests

Given a messy natural-language task, CodeGraph should produce a compact
investigation map.

Score:

- task intent correctness;
- critical files and symbols;
- file roles;
- risks and unknowns;
- proof paths;
- text/source-navigation/candidate labels;
- validation steps;
- follow-up queries;
- evidence-backed edit plan.

This tests repository cognition rather than raw file hits.

## 5. Plan-Accuracy Tests

Before editing code, the agent writes an implementation plan.

Compare:

```text
rg-only plan
rg + CodeGraph plan
```

Score:

- correct implementation surface;
- correct affected tests;
- no nonexistent symbols;
- no wrong files;
- correct assumptions and unknowns;
- correct validation path;
- less over-editing;
- better architecture explanation.

This is the most direct benchmark for hallucination prevention during planning.

## 6. Proof-Discipline Scoring

Every packet, plan, and answer should be audited against the proof ladder:

```text
text evidence
symbol evidence
candidate evidence
source-navigation evidence
graph relation proof
mutation proof
flow proof
unknown
```

Penalize:

- text evidence treated as behavior proof;
- symbol existence treated as correctness proof;
- vector/candidate evidence treated as graph proof;
- unsupported source/relation claims;
- failure to mark missing evidence as `unknown`.

## 7. Hallucination-Trap Tests

The lab should include adversarial fixtures where coding agents commonly drift:

- same symbol in multiple files;
- same filename in different packages;
- stale docs;
- generated files;
- test-only mocks;
- deleted or renamed files;
- dynamic dispatch;
- config-driven behavior;
- no proof path available.

Score:

- wrong file chosen;
- false relation claim;
- nonexistent-symbol reference;
- bad validation step;
- unsafe confidence;
- missed ambiguity.

## 8. Patch-Outcome Tests

Benchmark Layer v1 is the product benchmark.

Run the same task twice:

```text
rg-only agent
rg + CodeGraph agent
```

Measure:

- resolved percentage;
- test pass rate;
- wrong-file edit rate;
- nonexistent-symbol reference rate;
- unsupported claim rate;
- evidence alignment;
- time, tokens, tool calls, and context bytes;
- patch size;
- retry count;
- cost per solved task.

SWE-bench-family results require real external-agent predictions evaluated by an
official-compatible harness. Setup checks, mock-agent runs, and gold-patch
validation are prerequisites, not patch-quality scores.

## 9. Full-Codebase Complexity Tests

Use tasks where a single grep hit is not enough:

- trace a feature across modules;
- update an API contract;
- find config/runtime/test interactions;
- identify impacted tests and docs;
- detect stale index or dirty evidence;
- explain side effects before editing.

Score:

- missed dependencies;
- affected-test coverage;
- architecture-map accuracy;
- regression-risk detection;
- validation completeness;
- time/token cost compared with rg-only.

These tests decide whether MVP3 and MVP4 are buying real agent reliability.

## 10. Product Decision Gate

The v1 gate should answer one question:

```text
Does adding CodeGraph to the same agent's normal rg/search/edit/test tools make
that agent more reliable on the same pinned tasks?
```

A useful result must show at least one reliable improvement:

- fewer hallucinated plans;
- fewer wrong-file edits;
- fewer nonexistent-symbol references;
- better evidence alignment;
- higher patch success;
- lower time/token cost for correct diagnosis.

If v1 does not improve over the rg-only agent, the roadmap should pause or pivot
before deeper MVP3/MVP4 investment.

The possible future local-diagnostic headline, only after v1 evidence supports
it and a claim gate approves the exact wording, would be:

```text
CodeGraph improves agent reliability on top of normal rg use.
```

The unsafe headline remains:

```text
CodeGraph beats rg.
```
