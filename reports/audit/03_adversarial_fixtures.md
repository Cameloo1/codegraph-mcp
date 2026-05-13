# 03 Adversarial Hand-Labeled Fixture Repos

Source of truth: `MVP.md`.

Inputs read:

- `reports/baselines/BASELINE_LATEST.md`
- `benchmarks/graph_truth/schemas/graph_truth_case.schema.json`
- `reports/audit/02_graph_truth_manifest_schema.md`

## Executive Summary

Ten adversarial graph-truth fixture repos now exist under `benchmarks/graph_truth/fixtures/`. These are diagnostic correctness fixtures, not happy-path benchmark examples. Each case includes source files, a `README.md` explaining the trap, and a strict `graph_truth_case.json` manifest with expected facts and at least one forbidden edge or forbidden path.

The fixtures are intentionally small enough for a future graph-truth runner to index quickly, but strict enough to catch same-name symbol collisions, broad source spans, stale cache facts, alias-retargeting bugs, mock/test leakage, weak sanitizer/auth reasoning, and derived-edge provenance failures.

## Fixture Inventory

| Fixture | Primary trap | Relations covered | Mutation steps | Source-span checks |
| --- | --- | --- | ---: | ---: |
| `same_function_name_only_one_imported` | Same-name function in two files; only one is imported; also tests exact nearby callsite span. | `CALLS`, `IMPORTS` | 0 | 1 |
| `dynamic_import_marked_heuristic` | Runtime dynamic import must not become an exact guessed import. | `IMPORTS` | 0 | 1 |
| `file_rename_prunes_old_path` | Rename must not leave old-path graph facts. | `CALLS` | 6 | 1 |
| `import_alias_change_updates_target` | Alias import retarget must update the exact `CALLS` target. | `CALLS`, `IMPORTS` | 6 | 2 |
| `barrel_export_default_export_resolution` | Default export, named export, and barrel re-export must stay distinct. | `CALLS` | 0 | 2 |
| `test_mock_not_production_call` | Mock/stub/test assertions must not become production proof paths. | `ASSERTS`, `CALLS`, `MOCKS`, `STUBS` | 0 | 3 |
| `sanitizer_exists_but_not_on_flow` | Sanitizer existence must not imply sanitized data flow. | `FLOWS_TO`, `SANITIZES` | 0 | 1 |
| `admin_user_middleware_role_separation` | `requireAdmin` and `requireUser` roles must not be conflated. | `CHECKS_ROLE` | 0 | 2 |
| `derived_closure_edge_requires_provenance` | Derived closure edge must be distinct from base facts and carry provenance. | `CALLS`, `MAY_MUTATE`, `WRITES` | 0 | 3 |
| `stale_graph_cache_after_edit_delete` | Deleted/edited symbols must not remain queryable after reindex. | `CALLS` | 8 | 1 |

## Coverage Summary

- Fixture directories: 10.
- Valid graph-truth manifests: 10.
- Fixtures with forbidden edges or paths: 10.
- Fixtures requiring source-span validation: 10.
- Fixtures with mutation steps: 3.
- Fixtures exercising test/mock/auth/security relations: 3.
- Fixture covering default/barrel export: 1.
- Fixture covering dynamic import/runtime resolution: 1.
- Relations covered: `ASSERTS`, `CALLS`, `CHECKS_ROLE`, `FLOWS_TO`, `IMPORTS`, `MAY_MUTATE`, `MOCKS`, `SANITIZES`, `STUBS`, `WRITES`.

## Assertion Types Used

The manifests use the phase-02 schema fields for:

- `expected_entities`
- `expected_edges`
- `forbidden_edges`
- `expected_paths`
- `forbidden_paths`
- `expected_source_spans`
- `expected_context_symbols`
- `forbidden_context_symbols`
- `expected_tests`
- `forbidden_tests`
- `mutation_steps`
- `performance_expectations`
- `failure_rules`

Each proof-grade expected edge includes source-file/source-span data and exactness requirements. Forbidden assertions are included to make false positives observable, not just missing positives.

## Expected Traps

- Broad name matching should fail on `same_function_name_only_one_imported`.
- Exact guessing of runtime imports should fail on `dynamic_import_marked_heuristic`.
- Stale path cleanup should fail on `file_rename_prunes_old_path` and `stale_graph_cache_after_edit_delete` if update-mode replay is incomplete.
- Alias retargeting should fail on `import_alias_change_updates_target` if old edges survive mutation.
- Export resolver shortcuts should fail on `barrel_export_default_export_resolution`.
- Production/test leakage should fail on `test_mock_not_production_call`.
- Security proximity heuristics should fail on `sanitizer_exists_but_not_on_flow` and `admin_user_middleware_role_separation`.
- Derived fact inflation should fail on `derived_closure_edge_requires_provenance`.
- Source-span broadening should fail wherever the manifest requires exact call/import/assertion/check spans.

## Expected Current-Implementation Failures

The graph-truth runner is not required for this phase, so these are expected-risk notes rather than newly measured fixture results.

- `dynamic_import_marked_heuristic`: likely to fail unless dynamic import facts are explicitly represented as heuristic/unresolved instead of skipped or guessed exact.
- `file_rename_prunes_old_path`: likely to fail if mutation replay and stale path pruning are not wired into the runner.
- `barrel_export_default_export_resolution`: likely to fail unless default/barrel export resolution is compiler- or resolver-backed.
- `test_mock_not_production_call`: likely to fail unless test/mock path context is enforced during proof-path generation.
- `derived_closure_edge_requires_provenance`: likely to fail unless derived/cache edges carry queryable provenance.
- `stale_graph_cache_after_edit_delete`: likely to fail if delete/update cleanup is incomplete.

Cases may pass after later implementation phases, but they should remain strict adversarial gates and should not be weakened to match current behavior.

## Validation

Commands run:

```powershell
cargo test -p codegraph-bench graph_truth_case_schema -- --nocapture
cargo test -p codegraph-bench adversarial_graph_truth_fixture_cases_validate -- --nocapture
```

Results:

- Schema tests: 4 passed, including the 100-manifest validation performance test.
- Fixture validation test: 1 passed.
- Observed fixture schema validation runtime in the focused test: about 0.06s.
- Observed 100-manifest schema validation runtime in the focused schema test: about 0.07s.

## Next Phase

Build the graph-truth runner against these fixtures. The runner must compare expected and forbidden facts, fail on unsupported assertions instead of silently skipping them, and report false positives, false negatives, source-span failures, stale-update failures, and context-symbol failures per fixture.
