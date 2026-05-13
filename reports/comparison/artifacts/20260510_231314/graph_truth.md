# Graph Truth Gate

Verdict: `failed`

Cases: 11 total, 10 passed, 1 failed.

## Case Results

| Case | Status | Expected Edges | Base Edges | Derived Edges | Forbidden Hits | Span Failures | Context Failures |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `admin_user_middleware_role_separation` | `passed` | 2/2 | 168 | 0 | 0 | 0 | 0 |
| `barrel_export_default_export_resolution` | `passed` | 2/2 | 81 | 0 | 0 | 0 | 0 |
| `derived_closure_edge_requires_provenance` | `failed` | 1/3 | 57 | 0 | 0 | 0 | 1 |
| `dynamic_import_marked_heuristic` | `passed` | 1/1 | 53 | 0 | 0 | 0 | 0 |
| `file_rename_prunes_old_path` | `passed` | 1/1 | 37 | 0 | 0 | 0 | 0 |
| `import_alias_change_updates_target` | `passed` | 2/2 | 50 | 0 | 0 | 0 | 0 |
| `same_function_name_only_one_imported` | `passed` | 2/2 | 94 | 0 | 0 | 0 | 0 |
| `sanitizer_exists_but_not_on_flow` | `passed` | 1/1 | 89 | 0 | 0 | 0 | 0 |
| `source_span_exact_callsite` | `passed` | 1/1 | 52 | 0 | 0 | 0 | 0 |
| `stale_graph_cache_after_edit_delete` | `passed` | 1/1 | 31 | 0 | 0 | 0 | 0 |
| `test_mock_not_production_call` | `passed` | 4/4 | 115 | 0 | 0 | 0 | 0 |

## Relation Metrics

| Relation | Precision | Recall | Expected | Forbidden Hits |
| --- | ---: | ---: | ---: | ---: |
| `ASSERTS` | 1.000 | 1.000 | 1 | 0 |
| `CALLS` | 1.000 | 1.000 | 9 | 0 |
| `CHECKS_ROLE` | 1.000 | 1.000 | 2 | 0 |
| `FLOWS_TO` | 1.000 | 1.000 | 1 | 0 |
| `IMPORTS` | 1.000 | 1.000 | 3 | 0 |
| `MAY_MUTATE` | unknown | 0.000 | 1 | 0 |
| `MOCKS` | 1.000 | 1.000 | 1 | 0 |
| `SANITIZES` | unknown | unknown | 0 | 0 |
| `STUBS` | 1.000 | 1.000 | 1 | 0 |
| `WRITES` | unknown | 0.000 | 1 | 0 |

## Edge Class Counts

Observed base edges: 827. Observed derived/cache edges: 0.

| Fact Class | Observed Edges |
| --- | ---: |
| `base_exact` | 477 |
| `base_heuristic` | 34 |
| `mock` | 3 |
| `reified_callsite` | 73 |
| `test` | 65 |
| `unknown` | 175 |

## Top False Positives


## Top False Negatives

- `derived_closure_edge_requires_provenance`: missing expected entity src/store.ordersTable
- `derived_closure_edge_requires_provenance`: missing required edge src/store.saveOrder -WRITES-> src/store.ordersTable at src/store.ts:4
- `derived_closure_edge_requires_provenance`: missing required edge src/service.submitOrder -MAY_MUTATE-> src/store.ordersTable at src/service.ts:4
- `derived_closure_edge_requires_provenance`: missing expected path path://derived/provenance

## Source Span Failures


## Stale-Update Failures


## Test/Mock Leakage Failures


## Derived/Provenance Failures


## Notes

- Graph Truth Gate indexes each fixture and compares observed graph facts against hand-labeled expected and forbidden facts.
- A failed gate is expected until semantic resolution, source-span exactness, stale update, and provenance gaps are fixed.
