# Final Local Gate

Source of truth: `MVP.md`.

Generated: 2026-05-10.

Workflow: single agent only. No subagents were used. This phase is report-only: no production logic, schema behavior, benchmark scoring, or storage optimization was changed.

## Verdict

**Final local gate verdict: fail.**

The current system is **not ready for MVP correctness or production-readiness claims**. Unit tests pass, but the strict adversarial gates do not:

- Graph Truth Gate failed 4/10 fixtures.
- Context Packet Gate failed 8/10 fixtures.
- Stage ablation still reports forbidden-edge violations in graph-verification and full-context modes.
- The latest available Autoresearch DB family is about 803.5 MB decimal / 766.3 MiB, and that compact-storage target is not waived.
- Real-repo replay and CGC comparison are unavailable/unknown in this offline local run.
- Fake-agent/dry-run evidence is not counted as model coding success.

## Gate Criteria

| Criterion | Result | Evidence |
| --- | --- | --- |
| No forbidden Graph Truth violations | Pass | Graph Truth: 0/11 forbidden edges and 0/4 forbidden paths matched. |
| Proof paths have source spans | Partial | Graph Truth span failures: 0, but context packets found 0/5 expected proof paths. |
| Stale update tests pass | Pass | Workspace tests passed; rename/delete graph-truth fixtures passed. |
| Context packet gate passes | Fail | 2/10 cases passed; critical-symbol missing rate 0.636. |
| Storage target met or waived | Fail | Latest available DB family: 803,528,704 bytes; no waiver recorded. |
| Real-repo benchmark completes honestly | Unknown/incomplete | Real-repo replay unavailable; CGC skipped. |
| Fake-agent dry run not treated as model success | Pass | Parity/final-gate reports keep superiority unknown. |

## Commands Run

| Check | Command | Result | Output/artifact |
| --- | --- | --- | --- |
| Unit suite | `cargo test --workspace` | Passed | Workspace tests passed; existing ignored tests remained ignored. |
| Graph Truth Gate | `cargo run -p codegraph-cli -- bench graph-truth --cases benchmarks\graph_truth\fixtures --fixture-root . --out-json reports\audit\artifacts\24_graph_truth_gate.json --out-md reports\audit\artifacts\24_graph_truth_gate.md --fail-on-forbidden --fail-on-missing-source-span` | Failed | `cases_passed=6`, `cases_failed=4`. |
| Context Packet Gate | `cargo run -p codegraph-cli -- bench context-packet --cases benchmarks\graph_truth\fixtures --fixture-root . --out-json reports\audit\artifacts\24_context_packet_gate.json --out-md reports\audit\artifacts\24_context_packet_gate.md --top-k 10` | Failed | `cases_passed=2`, `cases_failed=8`. |
| Retrieval ablation | `cargo run -p codegraph-cli -- bench retrieval-ablation --cases benchmarks\graph_truth\fixtures --fixture-root . --out-json reports\audit\artifacts\24_stage_ablation.json --out-md reports\audit\artifacts\24_stage_ablation.md --top-k 10` | Benchmarked | Seven stage modes reported separately. |
| Storage inspection | `.\target\debug\codegraph-mcp.exe audit storage --db target\codegraph-bench-report\sweep-20260509-231456\autoresearch-codegraph-attempt3\codegraph.sqlite --json reports\audit\artifacts\24_storage_large.json --markdown reports\audit\artifacts\24_storage_large.md` | Passed | DB family 803,528,704 bytes; dbstat available. |
| Unresolved calls | `.\target\debug\codegraph-mcp.exe query unresolved-calls --db target\codegraph-bench-report\sweep-20260509-231456\autoresearch-codegraph-attempt3\codegraph.sqlite --limit 100 --offset 0 --json --no-snippets` | Passed | p50 19 ms, p95 20 ms over 5 runs; query plan still scans `edges`. |
| Deterministic indexing | `cargo test -p codegraph-index full_index_worker_count_determinism_preserves_graph_facts --lib` | Passed | 1 test passed. |
| Index timing smoke | `cargo run -p codegraph-cli -- bench synthetic-index --output-dir reports\audit\artifacts\24_synthetic_index --files 64` | Benchmarked | 64 files indexed in 860 ms, 16 workers. |
| Real-repo corpus | `.\target\debug\codegraph-mcp.exe bench real-repo-corpus` | Unknown/incomplete | Network disabled; clone plan recorded but not executed. |
| Parity report | `.\target\debug\codegraph-mcp.exe bench parity-report --output-dir reports\audit\artifacts\24_parity_report` | Reported | No SOTA/model-superiority claim. |
| Built-in final gate | `.\target\debug\codegraph-mcp.exe bench final-gate --output-dir reports\audit\artifacts\24_builtin_final_gate --workspace-root .` | Unknown | Internal fixture passed; CGC skipped, verdict unknown. |

## Graph Truth

Artifacts:

- `reports/audit/artifacts/24_graph_truth_gate.json`
- `reports/audit/artifacts/24_graph_truth_gate.md`

Summary:

| Metric | Value |
| --- | ---: |
| Cases passed | 6/10 |
| Expected entities matched | 14/16 |
| Expected edges matched | 12/17 |
| Forbidden edge hits | 0/11 |
| Expected paths matched | 4/5 |
| Forbidden path hits | 0/4 |
| Path recall | 0.800 |
| Source-span failures | 0 |
| Stale failures | 0 |

Failed fixtures:

- `derived_edge_requires_provenance`: missing `users`, `WRITES`, derived `MAY_MUTATE`, and expected base-proof path.
- `dynamic_import_marked_heuristic`: missing expected unresolved/static-heuristic dynamic `IMPORTS` fact.
- `mock_call_not_production_call`: missing expected `MOCKS`/`STUBS` edges despite production mock-path guardrails.
- `sanitizer_exists_but_not_on_flow`: semantic graph fact matched, but context-symbol scoring still fails this case.

Relation recall gaps:

| Relation | Recall |
| --- | ---: |
| `IMPORTS` | 0.500 |
| `MAY_MUTATE` | 0.000 |
| `MOCKS` | 0.000 |
| `STUBS` | 0.000 |
| `WRITES` | 0.000 |

## Source-Span Proof

Graph Truth source-span failures were zero, and `source_span_exact_callsite` passed. That is good but not enough: the Context Packet Gate found **0/5 expected proof paths**, so end-to-end proof-path usefulness is still failing.

Verdict: **partial**.

## Context Packet Gate

Artifacts:

- `reports/audit/artifacts/24_context_packet_gate.json`
- `reports/audit/artifacts/24_context_packet_gate.md`

Summary:

| Metric | Value |
| --- | ---: |
| Cases passed | 2/10 |
| Context symbol recall@10 | 0.364 |
| Critical symbol missing rate | 0.636 |
| Distractor ratio | 0.054 |
| Proof path coverage | 0.000 |
| Critical snippet coverage | 0.692 |
| Recommended test recall | 0.000 |
| Useful facts per byte | 0.0002607 |
| Useful facts per estimated token | 0.001048 |

Passed cases:

- `dynamic_import_marked_heuristic`
- `file_rename_prunes_old_path`

Context packets are not yet reliably useful: they miss critical symbols, expected proof paths, and expected tests across the adversarial suite.

## Stage Ablation

Artifacts:

- `reports/audit/artifacts/24_stage_ablation.json`
- `reports/audit/artifacts/24_stage_ablation.md`

| Mode | File R@k | Symbol R@k | Path R@k | Relation F1 | FP rate | Forbidden | p50 ms | p95 ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `stage0_exact_only` | 0.767 | 0.640 | 0.000 | unknown | 0.015 | 0 | 1 | 2 |
| `stage1_binary_only` | 1.000 | 0.590 | 0.000 | unknown | 0.018 | 0 | 1 | 1 |
| `stage2_int8_pq_only` | 0.967 | 0.653 | 0.000 | unknown | 0.018 | 0 | 18 | 88 |
| `stage0_plus_stage1` | 1.000 | 0.537 | 0.000 | unknown | 0.016 | 0 | 1 | 2 |
| `stage0_plus_stage1_plus_stage2` | 0.967 | 0.620 | 0.000 | unknown | 0.021 | 0 | 16 | 30 |
| `graph_verification_only` | 0.817 | 0.640 | 0.700 | 0.500 | 0.016 | 1 | 1 | 2 |
| `full_context_packet` | 0.783 | 0.593 | 0.700 | 0.500 | 0.027 | 1 | 5 | 14 |

Interpretation:

- Stage 0 remains the low-latency symbol seed carrier.
- Stage 2 adds fixture-scale latency and cannot claim proof-grade path success.
- Exact graph verification is fast on fixtures, but it is semantically incomplete and still sees one forbidden-edge violation in ablation scoring.

## Storage

Artifacts:

- `reports/audit/artifacts/24_storage_large.json`
- `reports/audit/artifacts/24_storage_large.md`

Latest available large DB inspected:

- DB path: `target/codegraph-bench-report/sweep-20260509-231456/autoresearch-codegraph-attempt3/codegraph.sqlite`
- Database bytes: 803,495,936
- DB family bytes: 803,528,704
- DB family size: about 803.5 MB decimal / 766.3 MiB
- `dbstat`: available

Top storage objects:

| Object | Type | Bytes | Rows |
| --- | --- | ---: | ---: |
| `edges` | table | 165,593,088 | 2,050,123 |
| `sqlite_autoindex_qualified_name_dict_1` | index | 125,501,440 | unknown |
| `qualified_name_dict` | table | 109,445,120 | unknown |
| `entities` | table | 62,488,576 | unknown |
| `sqlite_autoindex_object_id_dict_1` | index | 48,812,032 | unknown |
| `object_id_dict` | table | 43,884,544 | unknown |
| `idx_edges_head_relation` | index | 38,608,896 | unknown |
| `idx_edges_tail_relation` | index | 38,608,896 | unknown |

Storage verdict: **fail for compact large-repo goal**. Dictionary/index bloat remains real and should only be changed after copied-DB experiments prove correctness and query safety.

## Query Latency

Unresolved-call enumeration is now bounded and usable for agent planning:

- DB: latest available Autoresearch DB.
- Limit: 100.
- Snippets: disabled.
- Count total: skipped by default.
- Returned rows: 100.
- Total p50/p95 over 5 runs: 19 ms / 20 ms.
- Page-query p50/p95: 13 ms / 14 ms.
- Remaining query-plan risk: `SCAN e`.

Verdict: **bounded and usable for paged planning**, but still needs measured relation/exactness indexing before broad full accounting.

## Indexing

Deterministic indexing test:

- `cargo test -p codegraph-index full_index_worker_count_determinism_preserves_graph_facts --lib`
- Result: passed.

Synthetic index timing:

- 64 files indexed.
- 3,648 entities.
- 9,280 edges.
- Total wall time: 860 ms.
- DB write time: 768 ms.
- Worker count: 16.

This is a fixture-scale timing smoke, not a replacement for the 169.9s full Autoresearch cold-index measurement.

## Real Repo / CGC

Real-repo corpus replay:

- Status: unavailable.
- Reason: network disabled; clone plan recorded but not executed.

Built-in final gate:

- Internal compact fixture verdict: pass.
- CGC status: skipped.
- Overall built-in final verdict: unknown.

This must remain unknown. A skipped/incomplete competitor run is not a CodeGraph win.

## Known Unsupported Patterns

- Runtime dynamic imports are not emitted as the expected unresolved `IMPORTS` fact in the dynamic-import fixture.
- Derived `MAY_MUTATE` proof shortcuts are not generated for the adversarial provenance fixture.
- Exact `MOCKS` and `STUBS` extraction is still missing for the mock-call fixture.
- The sanitizer fixture still has a context-symbol scoring caveat, despite blocked false `SANITIZES` paths.
- Context packet selection misses critical symbols and proof paths across most adversarial fixtures.
- Barrel, package, namespace, dynamic, and broader TypeScript/JavaScript resolver patterns remain heuristic or unsupported without compiler/LSP proof.
- Semantic rename identity mapping and full dependency closure remain incomplete beyond focused stale-cleanup cases.
- Extended heuristic relations and multiline source-span patterns need stronger AST-backed validation.
- Real-repo replay and CGC comparison are unavailable in this offline local run.

## Remaining Blockers

1. Fix Graph Truth failures for derived provenance, dynamic import heuristic fact emission, mock/stub extraction, and sanitizer context scoring.
2. Make context packets recover critical symbols, expected proof paths, source-span-backed snippets, and expected tests while suppressing distractors.
3. Resolve the stage-ablation forbidden-edge violation in graph-verification/full-context surfaces.
4. Measure and safely reduce dictionary/index storage bloat without deleting proof facts or needed indexes.
5. Run honest real-repo and CGC comparisons in an environment with required external dependencies and network/cache access.

## Optimization Safety

Safe now:

- Report-only analysis and copied-DB storage experiments.
- Targeted correctness fixes for failing graph-truth and context-packet cases.
- Additional instrumentation around query plans, candidate counts, and proof-path filtering.

Not safe yet:

- Removing facts, dictionaries, or edge indexes to hit storage targets.
- Claiming storage victory from fixture-size DBs while the latest large DB remains around 803.5 MB decimal.
- Optimizing retrieval/vector stages before graph truth and context packet correctness gates pass.
- Treating fake-agent dry runs or incomplete CGC results as model-superiority evidence.
