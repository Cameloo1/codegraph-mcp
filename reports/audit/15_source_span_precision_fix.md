# 15 Source Span Precision Fix

Source of truth: `MVP.md`.

## Verdict

**Complete for the supported surfaces in this phase.** `CALLS`, `ASSERTS`, and resolved `IMPORTS` edges use exact syntax-node or exact import-line spans for the tested patterns. Missing, wrong-file, and out-of-range spans already fail proof validation through the phase 07 proof gate.

No broad span model rewrite was needed. The fix work here was to codify exact-span behavior with regression tests and to make the phase 14 resolved `IMPORTS` edge use the import declaration span.

## Span Rules Verified

| Relation | Current exact-span source |
| --- | --- |
| `CALLS` | `source_span_for_node` over the `call_expression` node. |
| `CALLEE` | Same call expression span as the reified callsite. |
| `ASSERTS` | Nested assertion call expression span collected inside test cases. |
| `IMPORTS` | Parser import statement span, and post-index resolved `File IMPORTS target` span from the import declaration line. |
| `CHECKS_ROLE` / `AUTHORIZES` | Best-effort call expression span from extended heuristic extraction; still non-proof-grade unless later validated. |

## Code Locations

| Surface | Files / functions |
| --- | --- |
| Callsite spans | `crates/codegraph-parser/src/lib.rs`: `extract_call`, `source_span_for_node` |
| Assertion spans | `crates/codegraph-parser/src/lib.rs`: `extract_test_call`, `collect_call_expression_nodes` |
| Import spans | `crates/codegraph-parser/src/lib.rs`: `import_statement` extraction; `crates/codegraph-index/src/lib.rs`: `parse_static_imports`, `resolved_import_edge` |
| Proof validation | `crates/codegraph-query/src/lib.rs`: phase 07 source-span validation paths documented in `reports/audit/07_source_span_proof_gate.md` |
| Graph Truth source-span checking | `crates/codegraph-bench/src/graph_truth.rs`: expected span validation and `--fail-on-missing-source-span` |

## Graph Truth Results

| Fixture | Result | Evidence |
| --- | --- | --- |
| `source_span_exact_callsite` | Passed | `reports/audit/artifacts/15_graph_truth_source_span_exact_callsite.json`, `reports/audit/artifacts/15_graph_truth_source_span_exact_callsite.md` |

The fixture verifies the exact `second()` callsite at `src/main.ts:6:5-6:13`, not the adjacent `first()` callsite.

## Tests Added Or Relied On

| Test | Purpose |
| --- | --- |
| `call_edges_use_exact_call_expression_span` | Two nearby calls; `CALLS` to `second` must point to the exact `second()` expression. |
| `assertion_edges_use_exact_assertion_expression_span` | Two nearby assertions; `ASSERTS` for the second assertion must point to that exact assertion expression. |
| `audit_same_name_only_imported_target_gets_exact_call` | Verifies resolved `IMPORTS` uses the exact import declaration span. |
| Existing phase 07 proof tests | Missing, wrong-file, and out-of-range spans fail validation. |

## Remaining Weak Spans

- Extended auth/security/event/persistence/test relations are still heuristic and need AST-aware pattern-specific validation before being treated as proof-grade.
- Standalone `source_spans` rows are still not a reliable edge-id-keyed audit source for every edge; inline edge spans remain the canonical span in current reports.
- Imported call matching in the post-index resolver is still line-scanned, though its output span is exact for the supported single-line call patterns.
- Multi-line import declarations and multi-line/chained assertions need dedicated fixture coverage.

## Next Work

1. Add AST-backed imported call matching so import-resolved call spans are not line-scan dependent.
2. Add fixtures for multi-line calls, chained assertions, and multi-line imports.
3. Promote auth/security spans only after role/auth pattern-specific syntax validation.
