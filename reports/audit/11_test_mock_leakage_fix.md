# 11 Test/Mock Leakage Fix

Source of truth: `MVP.md`.

## Verdict

**Leakage guard implemented for context packets; graph-truth fixture still fails on pre-existing semantic extraction gaps.**

Production context packets now reject test/mock paths by default. Test-impact mode intentionally allows `TESTS`, `ASSERTS`, `MOCKS`, `STUBS`, and other test evidence. Every generated `PathEvidence` now carries explicit path-context metadata so downstream tools can distinguish production proof from test/mock evidence.

The `mock_call_not_production_call` graph-truth fixture still does not fully pass because the current extractor still misses or mis-resolves the hand-labeled exact facts: production `CALLS` resolves to a static reference instead of `src/service.sendEmail`, `MOCKS` points to heuristic dependency/expression targets, and `STUBS` is not emitted for the mock factory. Those are semantic-resolution/extraction issues, not leakage-policy issues.

## Code Surfaces

| Surface | Files |
| --- | --- |
| Path classification and filtering | `crates/codegraph-query/src/lib.rs` |
| Graph-truth path-context check | `crates/codegraph-bench/src/graph_truth.rs` |
| CLI integration test | `crates/codegraph-cli/tests/cli_smoke.rs` |
| Relevant fixture | `benchmarks/graph_truth/fixtures/mock_call_not_production_call/graph_truth_case.json` |
| Prior reports read | `reports/audit/09_graph_truth_gate.md`, `reports/audit/10_stage_ablation.md` |

## Behavior Added

- Added `PathContext` with `production`, `test`, `mock`, and `mixed` classifications.
- Classified paths using relation kind, source file path, endpoint IDs, and edge metadata when present.
- Added production-mode filtering in `context_pack`:
  - default modes such as `impact`, `graph_truth`, and `retrieval_funnel` keep only production paths;
  - modes containing `test`, `spec`, `mock`, or `fixture` allow test/mock paths.
- Added per-path metadata:
  - `path_context`
  - `production_proof_eligible`
  - `proof_scope`
- Added packet-level metadata:
  - `path_context_policy`
  - `test_mock_edges_allowed`
  - `path_context_counts_before_filter`
  - `path_context_counts_after_filter`
  - `rejected_test_mock_path_count`
- Updated Graph Truth Gate to fail if a context-packet path lacks `path_context` or if non-production evidence is marked production-proof eligible.

## Fixture Result

Command:

```powershell
cargo run -p codegraph-cli -- bench graph-truth --cases benchmarks/graph_truth/fixtures/mock_call_not_production_call/graph_truth_case.json --fixture-root . --out-json reports/audit/artifacts/11_mock_fixture_after.json --out-md reports/audit/artifacts/11_mock_fixture_after.md --fail-on-forbidden --fail-on-missing-source-span
```

Result:

| Fixture | Before | After | Notes |
| --- | --- | --- | --- |
| `mock_call_not_production_call` | failed, 0/3 expected edges | failed, 0/3 expected edges | Leakage guard is enforced, but exact semantic facts are still missing. |

The relevant leakage indicators are clean: forbidden hits remain 0 and context-packet failures remain 0. The remaining failures are missing expected entity/edge/path facts from extraction and resolution.

## Remaining Risk

- Endpoint-ID heuristics can conservatively classify names containing `mock` or `stub` as mock context. This is safer for production proof, but may over-label a real production symbol with those words.
- Test-only `CALLS` edges are currently recognized by test-like source paths and endpoint IDs, not by a first-class persisted edge context field.
- The graph schema still lacks a first-class production/test/mock context column, so classification is enforced in query output rather than storage invariants.
- The parser still needs semantic target resolution for `CALLS`, `MOCKS`, and mock-factory `STUBS` before the fixture can pass.

## Tests Run

- `cargo test -p codegraph-query context_packet --lib`
- `cargo test -p codegraph-query --lib`
- `cargo test -p codegraph-bench graph_truth --lib`
- `cargo test -p codegraph-cli --test cli_smoke context_pack_modes_filter_production_and_allow_test_impact_edges`
- `cargo run -p codegraph-cli -- bench graph-truth --cases benchmarks/graph_truth/fixtures/mock_call_not_production_call/graph_truth_case.json --fixture-root . --out-json reports/audit/artifacts/11_mock_fixture_after.json --out-md reports/audit/artifacts/11_mock_fixture_after.md --fail-on-forbidden --fail-on-missing-source-span`

Final workspace test and lint status is recorded in `reports/audit/AUDIT_STATUS.md`.
