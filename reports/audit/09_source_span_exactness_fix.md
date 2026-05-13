# 09 Source Span Exactness Fix

Source of truth: `MVP.md`, `reports/audit/05_relation_source_span_audit.md`, source-span validation code, CALLS/ASSERTS/IMPORTS/AUTH extraction, and graph-truth fixtures.

## Verdict

Proof-grade source-span handling is now stricter and more useful for coding-agent evidence. Proof path validation rejects line-only proof spans, missing files, out-of-range columns, empty snippets, and broad statement spans for supported proof relations. Supported `CALLS`, `IMPORTS`, `ASSERTS`, `CHECKS_ROLE`, and `SANITIZES` spans now have exact-column validation at the proof gate.

The graph remains semantically incomplete: the latest Graph Truth Gate run passes 8/11 fixtures, with zero source-span failures, but still fails on missing semantic relations/entities for role helpers, derived mutation provenance, and unsanitized dataflow.

## Code Changes

| Area | Change |
| --- | --- |
| Proof validation | `codegraph-query` now requires proof-grade spans to include exact start/end columns and validates multi-line spans with first/last-line columns. |
| Relation-specific proof spans | Broad call/assert/auth/sanitizer statement spans are rejected when they contain statement semicolons or leading control/return/declaration syntax. |
| Context snippets | Empty snippets are skipped rather than added to context packets. |
| CALLS indexing | Static line-scanned calls now use balanced parenthesis matching, including nested calls and quoted `)` characters. |
| ASSERTS indexing | Static assertion import-target edges split same-line assertions and point to the exact assertion expression without the semicolon. |
| Fixtures | Restored `source_span_exact_callsite`; updated assertion and stale-update fixture spans to the current exact expression convention. |

## Graph Truth Results

Command:

```text
cargo run -q -p codegraph-cli -- bench graph-truth --cases benchmarks/graph_truth/fixtures --fixture-root benchmarks/graph_truth/fixtures --out-json reports/audit/artifacts/09_source_span_exactness_graph_truth.json --out-md reports/audit/artifacts/09_source_span_exactness_graph_truth.md --fail-on-forbidden --fail-on-missing-source-span --fail-on-unresolved-exact --fail-on-derived-without-provenance --fail-on-test-mock-production-leak --update-mode --verbose
```

| Fixture | Status | Notes |
| --- | --- | --- |
| `admin_user_middleware_role_separation` | failed | Role-check exact spans exist, but truth selectors still miss current Function-scoped role entities. |
| `barrel_export_default_export_resolution` | passed | Default/named/barrel resolution still passes. |
| `derived_closure_edge_requires_provenance` | failed | Missing `WRITES`, `MAY_MUTATE`, and table entity semantics. |
| `dynamic_import_marked_heuristic` | passed | Dynamic import remains non-exact. |
| `file_rename_prunes_old_path` | passed | Rename cleanup still passes. |
| `import_alias_change_updates_target` | passed | Alias retargeting still passes. |
| `same_function_name_only_one_imported` | passed | Same-name imported target still resolves correctly. |
| `sanitizer_exists_but_not_on_flow` | failed | Missing raw-to-write `FLOWS_TO`. |
| `source_span_exact_callsite` | passed | Exact `second()` callsite span is verified. |
| `stale_graph_cache_after_edit_delete` | passed | Stale deleted call is gone and final live call span matches the mutated file. |
| `test_mock_not_production_call` | passed | Mock/test spans and production proof isolation pass. |

Totals: 11 cases, 8 passed, 3 failed. Source-span failures: 0. Forbidden hits: 0.

## Tests

```text
cargo test -q
```

Result: passed.

New or updated coverage:

- proof validation rejects line-only, wrong-file, missing, out-of-range, and broad multi-site proof spans.
- broad spans over two nearby callsites fail.
- broad spans over two nearby assertions fail.
- nested static call records use the matching closing parenthesis.
- same-line static assertions split into separate exact assertion spans.

## Remaining Weak Spans

- Extended heuristic `CHECKS_ROLE` edges still appear beside exact resolver edges and should not be treated as proof-grade.
- Multi-line imports and multi-line chained assertion fixtures are still thin; validation supports columns but extractor coverage is not exhaustive.
- `AUTHORIZES` has validation rules but no dedicated graph-truth fixture in this pass.
- `SANITIZES` exact call spans are supported for direct sanitizer calls, but the unsanitized-flow fixture still exposes missing `FLOWS_TO` semantics rather than span failure.

## Highest-Priority Semantic Bug

Fix semantic security/dataflow extraction next. The current graph can prove exact source spans for observed facts, but it still misses required `CHECKS_ROLE` truth selectors and the `FLOWS_TO` raw-to-sink edge, so it is not yet trustworthy as a complete proof graph.
