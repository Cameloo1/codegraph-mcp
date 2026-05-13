# Relation Counts

Database: `<REPO_ROOT>/Desktop/development/codegraph-mcp/reports/final/artifacts/run_20260510_232527/fixture_workers4.sqlite`

Total edges: `833`

| Relation | Edges | Source spans | Missing span rows | Duplicates | Duplicate status | Derived | Top head types | Top tail types | Type status |
| --- | ---: | ---: | ---: | ---: | --- | ---: | --- | --- | --- |
| `CONTAINS` | 201 | 201 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `DEFINED_IN` | 173 | 173 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `FLOWS_TO` | 71 | 71 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `DEFINES` | 63 | 63 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `EXPORTS` | 37 | 37 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `IMPORTS` | 32 | 32 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `RETURNS` | 32 | 32 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `RETURNS_TO` | 32 | 32 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `READS` | 31 | 31 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `CALLS` | 30 | 30 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `CALLEE` | 28 | 28 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `DECLARES` | 27 | 27 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `ARGUMENT_0` | 19 | 19 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `ALIASED_BY` | 17 | 17 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `ARGUMENT_1` | 8 | 8 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `ASSIGNED_FROM` | 6 | 6 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `CHECKS_ROLE` | 6 | 6 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `ARGUMENT_N` | 4 | 4 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `MOCKS` | 3 | 3 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `WRITES` | 3 | 3 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `ASSERTS` | 2 | 2 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `AUTHORIZES` | 2 | 2 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `EXPOSES` | 2 | 2 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `AWAITS` | 1 | 1 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `COVERS` | 1 | 1 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `STUBS` | 1 | 1 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `TESTS` | 1 | 1 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |

## Notes

- Fast relation-counts reads only the edge table plus relation dictionary; inline source span columns are mandatory in the current schema.
- Fast relation-counts intentionally skips source_spans row joins, exactness grouping, duplicate grouping, and top entity-type grouping on large DBs; those fields are marked not_measured_fast_path.
- Context breakdown is not measured in the fast relation-count summary because runtime context is not a first-class edge column; inspect sampled edges for inferred production/test/mock labels.
