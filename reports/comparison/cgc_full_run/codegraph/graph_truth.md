# Graph Truth Gate

Verdict: `failed`

Cases: 11 total, 8 passed, 3 failed.

## Case Results

| Case | Status | Expected Edges | Base Edges | Derived Edges | Forbidden Hits | Span Failures | Context Failures |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `admin_user_middleware_role_separation` | `passed` | 2/2 | 164 | 0 | 0 | 0 | 0 |
| `barrel_export_default_export_resolution` | `passed` | 2/2 | 81 | 0 | 0 | 0 | 0 |
| `derived_closure_edge_requires_provenance` | `passed` | 3/3 | 61 | 1 | 0 | 0 | 0 |
| `dynamic_import_marked_heuristic` | `passed` | 1/1 | 53 | 0 | 0 | 0 | 0 |
| `file_rename_prunes_old_path` | `failed` | 0/1 | 37 | 0 | 2 | 0 | 1 |
| `import_alias_change_updates_target` | `failed` | 0/2 | 50 | 0 | 2 | 1 | 0 |
| `same_function_name_only_one_imported` | `passed` | 2/2 | 94 | 0 | 0 | 0 | 0 |
| `sanitizer_exists_but_not_on_flow` | `passed` | 1/1 | 88 | 0 | 0 | 0 | 0 |
| `source_span_exact_callsite` | `passed` | 1/1 | 52 | 0 | 0 | 0 | 0 |
| `stale_graph_cache_after_edit_delete` | `failed` | 1/1 | 37 | 0 | 0 | 1 | 0 |
| `test_mock_not_production_call` | `passed` | 4/4 | 115 | 0 | 0 | 0 | 0 |

## Relation Metrics

| Relation | Precision | Recall | Expected | Forbidden Hits |
| --- | ---: | ---: | ---: | ---: |
| `ASSERTS` | 1.000 | 1.000 | 1 | 0 |
| `CALLS` | 0.778 | 0.778 | 9 | 2 |
| `CHECKS_ROLE` | 1.000 | 1.000 | 2 | 0 |
| `FLOWS_TO` | 1.000 | 1.000 | 1 | 0 |
| `IMPORTS` | 1.000 | 0.667 | 3 | 0 |
| `MAY_MUTATE` | 1.000 | 1.000 | 1 | 0 |
| `MOCKS` | 1.000 | 1.000 | 1 | 0 |
| `SANITIZES` | unknown | unknown | 0 | 0 |
| `STUBS` | 1.000 | 1.000 | 1 | 0 |
| `WRITES` | 1.000 | 1.000 | 1 | 0 |

## Edge Class Counts

Observed base edges: 832. Observed derived/cache edges: 1.

| Fact Class | Observed Edges |
| --- | ---: |
| `base_exact` | 478 |
| `base_heuristic` | 35 |
| `derived` | 1 |
| `mock` | 4 |
| `reified_callsite` | 74 |
| `test` | 64 |
| `unknown` | 177 |

## Top False Positives

- `file_rename_prunes_old_path`: forbidden edge exists: src/use.run -CALLS-> src/oldName.renamedTarget at src/use.ts:4 observed as edge://ffb93c8d325d428275113416f4c57758
- `file_rename_prunes_old_path`: forbidden path exists: path://rename/run-to-old
- `import_alias_change_updates_target`: forbidden edge exists: src/use.run -CALLS-> src/alpha.alphaTarget at src/use.ts:4 observed as edge://2e2200dfa652158faff8ce92dbcbc159
- `import_alias_change_updates_target`: forbidden path exists: path://alias/run-to-alpha

## Top False Negatives

- `file_rename_prunes_old_path`: missing expected entity src/newName.renamedTarget
- `file_rename_prunes_old_path`: missing required edge src/use.run -CALLS-> src/newName.renamedTarget at src/use.ts:4
- `file_rename_prunes_old_path`: missing expected path path://rename/run-to-new
- `import_alias_change_updates_target`: missing required edge src/use -IMPORTS-> src/beta.betaTarget at src/use.ts:1
- `import_alias_change_updates_target`: missing required edge src/use.run -CALLS-> src/beta.betaTarget at src/use.ts:4
- `import_alias_change_updates_target`: missing expected path path://alias/run-to-beta
- `stale_graph_cache_after_edit_delete`: missing expected path path://stale/run-live

## Source Span Failures

- `import_alias_change_updates_target`: expected source span not observed: src/use.ts:1-1
- `stale_graph_cache_after_edit_delete`: source span start column mismatch: observed src/use.ts:4:10-4:22, expected column 3

## Stale-Update Failures

- `file_rename_prunes_old_path`: case has 6 mutation_steps but --update-mode was not supplied
- `import_alias_change_updates_target`: case has 6 mutation_steps but --update-mode was not supplied
- `stale_graph_cache_after_edit_delete`: case has 8 mutation_steps but --update-mode was not supplied

## Test/Mock Leakage Failures


## Derived/Provenance Failures


## Notes

- Graph Truth Gate indexes each fixture and compares observed graph facts against hand-labeled expected and forbidden facts.
- A failed gate is expected until semantic resolution, source-span exactness, stale update, and provenance gaps are fixed.
