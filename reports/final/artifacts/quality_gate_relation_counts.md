# Relation Counts

Database: `reports/final/artifacts/compact_gate_autoresearch_proof.sqlite`

Total edges: `170114`

| Relation | Edges | Source spans | Missing span rows | Duplicates | Duplicate status | Derived | Top head types | Top tail types | Type status |
| --- | ---: | ---: | ---: | ---: | --- | ---: | --- | --- | --- |
| `CONTAINS` | 48532 | 48532 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `DEFINED_IN` | 48109 | 48109 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `ARGUMENT_0` | 19654 | 19654 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `FLOWS_TO` | 14801 | 14801 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `DECLARES` | 11468 | 11468 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `ARGUMENT_1` | 5225 | 5225 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `IMPORTS` | 3322 | 3322 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `DEFINES` | 3231 | 3231 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `ARGUMENT_N` | 3101 | 3101 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `CALLS` | 2363 | 2363 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `CALLEE` | 2177 | 2177 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `READS` | 1970 | 1970 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `ASSIGNED_FROM` | 1494 | 1494 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `MAY_MUTATE` | 1203 | 1203 | 0 | 0 | `not_measured_fast_path` | 1203 | unknown | unknown | `not_measured_fast_path` |
| `WRITES` | 1104 | 1104 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `EXPORTS` | 1103 | 1103 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `RETURNS` | 447 | 447 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `RETURNS_TO` | 447 | 447 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `ALIASED_BY` | 186 | 186 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |
| `MUTATES` | 177 | 177 | 0 | 0 | `not_measured_fast_path` | 0 | unknown | unknown | `not_measured_fast_path` |

## Notes

- Fast relation-counts reads only the edge table plus relation dictionary; inline source span columns are mandatory in the current schema.
- Fast relation-counts intentionally skips source_spans row joins, exactness grouping, duplicate grouping, and top entity-type grouping on large DBs; those fields are marked not_measured_fast_path.
- Context breakdown is not measured in the fast relation-count summary because runtime context is not a first-class edge column; inspect sampled edges for inferred production/test/mock labels.
