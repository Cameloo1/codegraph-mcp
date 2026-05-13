# Semantic Correctness Gate

Date: 2026-05-10 20:19:46 -05:00

## Verdict

Verdict: `fail`

Proceed to indexing/storage optimization: `no`

The current repository has 11 graph-truth fixture directories, although the prompt refers to 10. The strict graph-truth run passes 10/11 fixtures, but the remaining failure is not an explicitly unsupported pattern. It is the supported derived persistence/provenance fixture, and it is missing concrete semantic facts.

Context packet checks are implemented and also fail: 0/11 context-packet cases pass, with 12/15 critical symbols missing and 0/11 expected proof paths recovered.

## Commands Run

| Check | Command | Result |
| --- | --- | --- |
| Full unit tests | `cargo test -q` | passed |
| Graph Truth Gate | `cargo run -q -p codegraph-cli -- bench graph-truth --cases benchmarks/graph_truth/fixtures --fixture-root <REPO_ROOT>/Desktop\development\codegraph-mcp --out-json reports/audit/artifacts/12_graph_truth_semantic_gate.json --out-md reports/audit/artifacts/12_graph_truth_semantic_gate.md --fail-on-forbidden --fail-on-missing-source-span --fail-on-unresolved-exact --fail-on-derived-without-provenance --fail-on-test-mock-production-leak --update-mode --verbose` | failed |
| Source-span proof validation | `cargo test -q -p codegraph-query proof_span_validation` | passed, 11/11 |
| Context Packet Gate | `cargo run -q -p codegraph-cli -- bench context-packet --cases benchmarks/graph_truth/fixtures --fixture-root <REPO_ROOT>/Desktop\development\codegraph-mcp --out-json reports/audit/artifacts/12_context_packet_gate.json --out-md reports/audit/artifacts/12_context_packet_gate.md --top-k 5 --budget 2000` | failed |
| Small fixture sampler | relation counts, sampled edges, sampled PathEvidence over a 3-fixture corpus | passed |
| Stale mutation tests | targeted edit/delete/rename/import-alias/stale cleanup tests | passed |

Artifacts:

- `reports/audit/artifacts/12_graph_truth_semantic_gate.json`
- `reports/audit/artifacts/12_graph_truth_semantic_gate.md`
- `reports/audit/artifacts/12_context_packet_gate.json`
- `reports/audit/artifacts/12_context_packet_gate.md`
- `reports/audit/artifacts/12_relation_counts_small_fixture.json`
- `reports/audit/artifacts/12_sample_CALLS_small_fixture.json`
- `reports/audit/artifacts/12_sample_CHECKS_ROLE_small_fixture.json`
- `reports/audit/artifacts/12_sample_FLOWS_TO_small_fixture.json`
- `reports/audit/artifacts/12_sample_ASSERTS_small_fixture.json`
- `reports/audit/artifacts/12_sample_MOCKS_small_fixture.json`
- `reports/audit/artifacts/12_sample_EXPOSES_small_fixture.json`
- `reports/audit/artifacts/12_sample_PathEvidence_small_fixture.json`

## Required Gate Metrics

| Metric | Value |
| --- | ---: |
| Fixture pass/fail | 10 passed / 1 failed / 11 total |
| Required edge pass rate | 18/20 = 90.00% |
| Forbidden edge violation count | 0 |
| Expected path pass rate | 10/11 = 90.91% |
| Forbidden path violation count | 0 |
| Source-span failure count | 0 |
| Unresolved-exact violation count | 0 |
| Derived-without-provenance violation count | 0 |
| Test/mock production leakage count | 0 |
| Stale fact count after mutations | 0 |
| Graph Truth critical context symbol missing count | 1 |
| Context Packet critical context symbol missing count | 12 |

Source-span proof coverage is clean for observed graph-truth proof facts: strict graph truth reported 0 source-span failures, `source_span_exact_callsite` passed, and the targeted proof-span validator passed 11/11 tests. Context packet source-span coverage is not meaningful yet because the context packet gate matched 0/11 expected proof paths.

## Fixture Results

| Fixture | Status | Expected Edges | Expected Paths | Forbidden Edge Hits | Forbidden Path Hits | Span Failures | Stale Failures |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `admin_user_middleware_role_separation` | passed | 2/2 | 2/2 | 0 | 0 | 0 | 0 |
| `barrel_export_default_export_resolution` | passed | 2/2 | 2/2 | 0 | 0 | 0 | 0 |
| `derived_closure_edge_requires_provenance` | failed | 1/3 | 0/1 | 0 | 0 | 0 | 0 |
| `dynamic_import_marked_heuristic` | passed | 1/1 | 0/0 | 0 | 0 | 0 | 0 |
| `file_rename_prunes_old_path` | passed | 1/1 | 1/1 | 0 | 0 | 0 | 0 |
| `import_alias_change_updates_target` | passed | 2/2 | 1/1 | 0 | 0 | 0 | 0 |
| `same_function_name_only_one_imported` | passed | 2/2 | 1/1 | 0 | 0 | 0 | 0 |
| `sanitizer_exists_but_not_on_flow` | passed | 1/1 | 0/0 | 0 | 0 | 0 | 0 |
| `source_span_exact_callsite` | passed | 1/1 | 1/1 | 0 | 0 | 0 | 0 |
| `stale_graph_cache_after_edit_delete` | passed | 1/1 | 1/1 | 0 | 0 | 0 | 0 |
| `test_mock_not_production_call` | passed | 4/4 | 1/1 | 0 | 0 | 0 | 0 |

## Relation Metrics

| Relation | Expected | Matched | Recall | Forbidden Hits |
| --- | ---: | ---: | ---: | ---: |
| `ASSERTS` | 1 | 1 | 1.000 | 0 |
| `CALLS` | 9 | 9 | 1.000 | 0 |
| `CHECKS_ROLE` | 2 | 2 | 1.000 | 0 |
| `FLOWS_TO` | 1 | 1 | 1.000 | 0 |
| `IMPORTS` | 3 | 3 | 1.000 | 0 |
| `MAY_MUTATE` | 1 | 0 | 0.000 | 0 |
| `MOCKS` | 1 | 1 | 1.000 | 0 |
| `SANITIZES` | 0 | 0 | n/a | 0 |
| `STUBS` | 1 | 1 | 1.000 | 0 |
| `WRITES` | 1 | 0 | 0.000 | 0 |

## False Positives

Exact false positives: none found.

Forbidden edges found: 0.

Forbidden paths found: 0.

## False Negatives

- `derived_closure_edge_requires_provenance`: missing expected entity `src/store.ordersTable`
- `derived_closure_edge_requires_provenance`: missing required edge `src/store.saveOrder -WRITES-> src/store.ordersTable at src/store.ts:4`
- `derived_closure_edge_requires_provenance`: missing required edge `src/service.submitOrder -MAY_MUTATE-> src/store.ordersTable at src/service.ts:4`
- `derived_closure_edge_requires_provenance`: missing expected path `path://derived/provenance`

## Source Spans

Graph Truth source-span failures: 0.

Targeted proof-span validation: 11 passed / 0 failed.

Small-corpus sampler source snippets:

| Relation | Samples | Spans Loaded | Missing Snippets |
| --- | ---: | ---: | ---: |
| `CALLS` | 8 | 8 | 0 |
| `CHECKS_ROLE` | 6 | 6 | 0 |
| `FLOWS_TO` | 20 | 20 | 0 |
| `ASSERTS` | 2 | 2 | 0 |
| `MOCKS` | 3 | 3 | 0 |
| `EXPOSES` | 2 | 2 | 0 |

Source-span quality problem: spans and snippets load on the sampled fixture corpus, but sampled records still have missing repo commit metadata, and many `CALLS`/`FLOWS_TO` samples have empty metadata. That is not a source-span crash, but it limits auditability.

## Context Packet Gate

| Metric | Value |
| --- | ---: |
| Cases passed | 0/11 |
| Expected context symbols | 15 |
| Matched context symbols at top-k | 3 |
| Critical context symbols missing | 12 |
| Forbidden context symbols included | 2 |
| Expected proof paths | 11 |
| Matched proof paths | 0 |
| Expected critical snippets | 17 |
| Matched critical snippets | 3 |
| Expected tests | 1 |
| Matched tests | 0 |

This is a hard blocker for saying the end-to-end proof packet is trustworthy, even though most underlying graph-truth fixtures now pass.

## Relation Sampler

Small corpus: `admin_user_middleware_role_separation`, `sanitizer_exists_but_not_on_flow`, and `test_mock_not_production_call`.

Index result: 7 files, 216 entities, 372 edges, 0 parse errors, 99 ms total wall time.

Top relation counts:

| Relation | Edges | Source Spans |
| --- | ---: | ---: |
| `CONTAINS` | 80 | 80 |
| `DEFINED_IN` | 73 | 73 |
| `FLOWS_TO` | 44 | 44 |
| `DEFINES` | 18 | 18 |
| `DECLARES` | 17 | 17 |
| `CALLEE` | 16 | 16 |
| `ARGUMENT_0` | 15 | 15 |
| `READS` | 15 | 15 |
| `IMPORTS` | 12 | 12 |
| `RETURNS` | 11 | 11 |

Missing metadata problems:

- `repo_commit` is missing on all sampled edge records.
- `metadata_empty` appears on 7/8 sampled `CALLS` and 20/20 sampled `FLOWS_TO`.
- The sampler produced 20 PathEvidence samples, but all 20 were audit-generated fallback paths because stored PathEvidence count was 0.
- 19/20 sampled PathEvidence records have empty metadata, and 20/20 are missing repo commit metadata.

PathEvidence quality problem: fallback one-edge paths are reviewable, with snippets loaded, but they do not prove that stored proof paths are being persisted and retrieved.

## Current Unsupported Patterns

- Dynamic or computed roles are not exact.
- Role checks inferred only from comments, strings, or nearby literals are ignored as exact facts.
- Sanitizer existence or unused sanitizer imports do not create `SANITIZES`.
- Complex interprocedural sanitizer/dataflow paths remain unsupported unless represented by direct local patterns.
- Unsupported `authorize(req.user)`-style calls remain heuristic evidence, not exact proof.
- Framework-specific route syntaxes beyond the supported route factory and existing parser heuristics remain heuristic/unsupported unless a syntax pattern is added.
- Stored PathEvidence is absent in the small-corpus sampler.
- Context packet assembly currently does not recover graph-truth proof paths.

## Readiness Decision

The system is not allowed to proceed to smarter indexing/storage optimization.

Reasons:

- The current strict Graph Truth Gate is not green.
- The remaining graph-truth failure is a supported semantic bug, not a documented unsupported pattern.
- Context packets are failing every fixture and do not surface expected proof paths.
- Stored PathEvidence is absent in the small fixture sampler.

Highest-priority semantic bug: implement provenance-backed persistence/dataflow semantics for table-like sinks so `src/store.ordersTable`, `WRITES`, and provenance-backed `MAY_MUTATE` are emitted without heuristic/proof-grade contamination.
