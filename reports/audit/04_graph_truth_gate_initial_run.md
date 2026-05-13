# Graph Truth Gate

Verdict: `failed`

Cases: 10 total, 2 passed, 8 failed.

## Case Results

| Case | Status | Expected Edges | Base Edges | Derived Edges | Forbidden Hits | Span Failures | Context Failures |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `admin_user_middleware_role_separation` | `failed` | 0/2 | 164 | 0 | 0 | 0 | 2 |
| `barrel_export_default_export_resolution` | `failed` | 0/2 | 71 | 0 | 0 | 0 | 1 |
| `derived_closure_edge_requires_provenance` | `failed` | 1/3 | 57 | 0 | 0 | 0 | 1 |
| `dynamic_import_marked_heuristic` | `failed` | 0/1 | 48 | 0 | 0 | 0 | 1 |
| `file_rename_prunes_old_path` | `passed` | 1/1 | 37 | 0 | 0 | 0 | 0 |
| `import_alias_change_updates_target` | `failed` | 2/2 | 50 | 0 | 0 | 1 | 0 |
| `same_function_name_only_one_imported` | `passed` | 2/2 | 94 | 0 | 0 | 0 | 0 |
| `sanitizer_exists_but_not_on_flow` | `failed` | 0/1 | 87 | 0 | 0 | 0 | 1 |
| `stale_graph_cache_after_edit_delete` | `failed` | 1/1 | 31 | 0 | 0 | 1 | 0 |
| `test_mock_not_production_call` | `failed` | 1/4 | 108 | 0 | 0 | 0 | 0 |

## Relation Metrics

| Relation | Precision | Recall | Expected | Forbidden Hits |
| --- | ---: | ---: | ---: | ---: |
| `ASSERTS` | unknown | 0.000 | 1 | 0 |
| `CALLS` | 1.000 | 0.750 | 8 | 0 |
| `CHECKS_ROLE` | unknown | 0.000 | 2 | 0 |
| `FLOWS_TO` | unknown | 0.000 | 1 | 0 |
| `IMPORTS` | 1.000 | 0.667 | 3 | 0 |
| `MAY_MUTATE` | unknown | 0.000 | 1 | 0 |
| `MOCKS` | unknown | 0.000 | 1 | 0 |
| `SANITIZES` | unknown | unknown | 0 | 0 |
| `STUBS` | unknown | 0.000 | 1 | 0 |
| `WRITES` | unknown | 0.000 | 1 | 0 |

## Edge Class Counts

Observed base edges: 747. Observed derived/cache edges: 0.

| Fact Class | Observed Edges |
| --- | ---: |
| `base_exact` | 429 |
| `base_heuristic` | 31 |
| `inverse` | 157 |
| `reified_callsite` | 69 |
| `test_mock` | 61 |

## Top False Positives


## Top False Negatives

- `admin_user_middleware_role_separation`: missing expected entity src/auth.requireAdmin
- `admin_user_middleware_role_separation`: missing expected entity src/auth.requireUser
- `admin_user_middleware_role_separation`: missing expected entity role:admin
- `admin_user_middleware_role_separation`: missing expected entity role:user
- `admin_user_middleware_role_separation`: missing required edge src/auth.requireAdmin -CHECKS_ROLE-> role:admin at src/auth.ts:2
- `admin_user_middleware_role_separation`: missing required edge src/auth.requireUser -CHECKS_ROLE-> role:user at src/auth.ts:6
- `admin_user_middleware_role_separation`: missing expected path path://roles/admin
- `admin_user_middleware_role_separation`: missing expected path path://roles/user
- `barrel_export_default_export_resolution`: missing expected entity src/defaultFeature.default
- `barrel_export_default_export_resolution`: missing required edge src/use.runDefault -CALLS-> src/defaultFeature.default at src/use.ts:4
- `barrel_export_default_export_resolution`: missing required edge src/use.runNamed -CALLS-> src/namedFeature.feature at src/use.ts:8
- `barrel_export_default_export_resolution`: missing expected path path://barrel/default
- `barrel_export_default_export_resolution`: missing expected path path://barrel/named
- `derived_closure_edge_requires_provenance`: missing expected entity src/store.ordersTable
- `derived_closure_edge_requires_provenance`: missing required edge src/store.saveOrder -WRITES-> src/store.ordersTable at src/store.ts:4
- `derived_closure_edge_requires_provenance`: missing required edge src/service.submitOrder -MAY_MUTATE-> src/store.ordersTable at src/service.ts:4
- `derived_closure_edge_requires_provenance`: missing expected path path://derived/provenance
- `dynamic_import_marked_heuristic`: missing expected entity dynamic_import:./plugins/+name
- `dynamic_import_marked_heuristic`: missing required edge src/loader.loadPlugin -IMPORTS-> dynamic_import:./plugins/+name at src/loader.ts:2
- `sanitizer_exists_but_not_on_flow`: missing required edge src/register.saveComment.raw -FLOWS_TO-> src/register.writeComment at src/register.ts:6

## Source Span Failures

- `import_alias_change_updates_target`: source span end column mismatch: observed src/use.ts:1:1-1:48, expected column 49
- `stale_graph_cache_after_edit_delete`: source span start column mismatch: observed src/use.ts:4:3-4:15, expected column 10

## Stale-Update Failures


## Test/Mock Leakage Failures


## Derived/Provenance Failures


## Notes

- Graph Truth Gate indexes each fixture and compares observed graph facts against hand-labeled expected and forbidden facts.
- A failed gate is expected until semantic resolution, source-span exactness, stale update, and provenance gaps are fixed.
