# Graph Truth Gate

Verdict: `passed`

Cases: 11 total, 11 passed, 0 failed.

## Case Results

| Case | Status | Expected Edges | Base Edges | Derived Edges | Forbidden Hits | Span Failures | Context Failures |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `admin_user_middleware_role_separation` | `passed` | 2/2 | 164 | 0 | 0 | 0 | 0 |
| `barrel_export_default_export_resolution` | `passed` | 2/2 | 81 | 0 | 0 | 0 | 0 |
| `derived_closure_edge_requires_provenance` | `passed` | 3/3 | 61 | 1 | 0 | 0 | 0 |
| `dynamic_import_marked_heuristic` | `passed` | 1/1 | 53 | 0 | 0 | 0 | 0 |
| `file_rename_prunes_old_path` | `passed` | 1/1 | 37 | 0 | 0 | 0 | 0 |
| `import_alias_change_updates_target` | `passed` | 2/2 | 50 | 0 | 0 | 0 | 0 |
| `same_function_name_only_one_imported` | `passed` | 2/2 | 94 | 0 | 0 | 0 | 0 |
| `sanitizer_exists_but_not_on_flow` | `passed` | 1/1 | 88 | 0 | 0 | 0 | 0 |
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
| `MAY_MUTATE` | 1.000 | 1.000 | 1 | 0 |
| `MOCKS` | 1.000 | 1.000 | 1 | 0 |
| `SANITIZES` | unknown | unknown | 0 | 0 |
| `STUBS` | 1.000 | 1.000 | 1 | 0 |
| `WRITES` | 1.000 | 1.000 | 1 | 0 |

## Edge Class Counts

Observed base edges: 826. Observed derived/cache edges: 1.

| Fact Class | Observed Edges |
| --- | ---: |
| `base_exact` | 475 |
| `base_heuristic` | 34 |
| `derived` | 1 |
| `mock` | 4 |
| `reified_callsite` | 73 |
| `test` | 64 |
| `unknown` | 176 |

## Top False Positives


## Top False Negatives


## Source Span Failures


## Stale-Update Failures


## Test/Mock Leakage Failures


## Derived/Provenance Failures


## Notes

- Graph Truth Gate indexes each fixture and compares observed graph facts against hand-labeled expected and forbidden facts.
- A failed gate is expected until semantic resolution, source-span exactness, stale update, and provenance gaps are fixed.
