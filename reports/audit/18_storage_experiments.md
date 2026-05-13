# 18 Storage Experiments

Verdict: safe experiment runner added; no production schema/index change applied.

`MVP.md` keeps SQLite as the local graph store for the MVP and requires exact graph/source verification to remain trustworthy. This phase therefore adds a copied-DB experiment harness so storage reductions can be measured before any production schema change.

## Sources Inspected

- `MVP.md`
- `reports/audit/03_storage_forensics.md`
- `reports/audit/04_relation_counts_and_samples.md`
- `crates/codegraph-cli/src/audit.rs`
- `crates/codegraph-cli/src/lib.rs`
- `crates/codegraph-store/src/sqlite.rs`
- `reports/audit/artifacts/03_storage_latest.json`
- `reports/audit/artifacts/04_relation_counts_latest.json`

## Command

```powershell
codegraph-mcp audit storage-experiments --db <path> --workdir <dir> --json <out.json> --markdown <out.md>
```

The runner copies the input SQLite DB into a per-run experiment folder, mutates only the copy, measures the copy, and removes copied DBs by default. Use `--keep-copies` only when a human needs to inspect the mutated DBs.

## Experiments Implemented

| Experiment | Mutates original DB? | What it measures |
| --- | --- | --- |
| `vacuum_analyze` | No | `ANALYZE`, `PRAGMA optimize`, and `VACUUM` effects on copied DB size, object sizes, and standard query latencies. |
| `drop_recreate_edge_indexes` | No | Size and query impact of dropping `idx_edges_head_relation`, `idx_edges_tail_relation`, and `idx_edges_span_path`, then recreating them on the copied DB. |

Each checkpoint records:

- DB/WAL/SHM bytes
- page metrics
- top table/index dbstat objects
- dictionary table bytes
- unique text index bytes
- edge index bytes
- FTS/source-span/snippet-like bytes
- standard query SQL, latency, row count, `EXPLAIN QUERY PLAN`, index usage, and full scans
- query degradation flags
- graph-truth applicability status

Graph Truth is marked not applicable inside this runner because graph-truth fixtures reindex source fixtures into their own DBs rather than consuming a copied benchmark DB. The storage experiments do not change extractor behavior or benchmark scoring.

## Latest DB Run

Executed:

```powershell
cargo run -p codegraph-cli -- audit storage-experiments --db target\codegraph-bench-report\sweep-20260509-231456\autoresearch-codegraph-attempt3\codegraph.sqlite --workdir reports\audit\artifacts\18_storage_experiments_work --json reports\audit\artifacts\18_storage_experiments_latest.json --markdown reports\audit\artifacts\18_storage_experiments_latest.md
```

Artifacts:

- `reports/audit/artifacts/18_storage_experiments_latest.json`
- `reports/audit/artifacts/18_storage_experiments_latest.md`

The copied DB files were removed after measurement (`copy_removed = true`).

## Size Results

| Experiment | Checkpoint | DB bytes | Edge index bytes | Dictionary table bytes | Unique text index bytes | FTS bytes | Source-span bytes |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `vacuum_analyze` | before | 803495936 | 110051328 | 203608064 | 231276544 | 20480 | 8192 |
| `vacuum_analyze` | after_analyze | 803536896 | 110051328 | 203608064 | 231276544 | 20480 | 8192 |
| `vacuum_analyze` | after_vacuum | 744718336 | 110051328 | 203608064 | 197001216 | 20480 | 8192 |
| `drop_recreate_edge_indexes` | before | 803495936 | 110051328 | 203608064 | 231276544 | 20480 | 8192 |
| `drop_recreate_edge_indexes` | after_drop_edge_indexes | 634626048 | 0 | 203608064 | 197001216 | 20480 | 8192 |
| `drop_recreate_edge_indexes` | after_recreate_edge_indexes | 744718336 | 110051328 | 203608064 | 197001216 | 20480 | 8192 |

Observed effects:

- `VACUUM` on a copied DB reduced file bytes by about 58.8 MiB.
- The unique text index category compacted from about 220.6 MiB to about 187.9 MiB after `VACUUM`.
- Dropping the three edge lookup indexes reduced the copied DB to about 605.2 MiB, but that is not safe by itself.
- Recreating the edge indexes returned the copied DB to the same post-vacuum size.

## Query Impact

The standard query suite includes entity name/qname lookup, head/relation edge lookup, tail/relation edge lookup, span-path edge lookup, and relation-count scan.

Degradation flags from the latest run:

| Query | Checkpoint | Before | After | Verdict |
| --- | --- | ---: | ---: | --- |
| `edge_head_relation_lookup` | `after_drop_edge_indexes` | 0 ms | 163 ms | Degraded; do not drop `idx_edges_head_relation`. |
| `edge_tail_relation_lookup` | `after_drop_edge_indexes` | 0 ms | 166 ms | Degraded; do not drop `idx_edges_tail_relation`. |

`relation_count_scan` stayed scan-oriented and did not prove any edge index removable. `idx_edges_span_path` did not show a large degradation in this limited query suite, but storage audit 03 ties it to stale-file deletion, so it is not safe to remove without a dedicated delete/update benchmark.

## Biggest Structures

The biggest structures remain the same as storage audit 03:

1. `edges` table
2. `sqlite_autoindex_qualified_name_dict_1`
3. `qualified_name_dict`
4. `entities`
5. `sqlite_autoindex_object_id_dict_1`
6. `object_id_dict`
7. `idx_edges_head_relation`
8. `idx_edges_tail_relation`
9. `idx_edges_span_path`
10. `sqlite_autoindex_symbol_dict_1`

The experiment confirms dictionary/index bloat is real, especially full qualified-name text and unique text b-trees.

## Index Verdicts

| Index/category | Verdict |
| --- | --- |
| `idx_edges_head_relation` | Necessary under current query paths; removal degraded lookup. |
| `idx_edges_tail_relation` | Necessary under current query paths; removal degraded lookup. |
| `idx_edges_span_path` | Not proven removable; must be tested with stale delete/update workloads. |
| Unique text autoindexes | Real bloat, but unsafe to remove without alternate uniqueness and lookup invariants. |
| FTS/source-span objects | Not storage-heavy in the latest DB. |

## Safe Candidates

- Run `VACUUM`/`ANALYZE` measurements on copied DBs after large indexing runs; a production policy can be considered only after write-time and artifact-lifecycle costs are measured.
- Prototype a qualified-name dictionary redesign on a copied DB or branch: uniqueness over compact `(prefix_id, suffix_id)` plus a compatibility path for full qname export/search.
- Add dedicated copied-DB measurements for stale delete/update before touching `idx_edges_span_path`.
- Add relation/exactness query measurements from phase 17 before considering any new partial indexes.

## Unsafe Candidates

- Do not remove head/tail edge lookup indexes.
- Do not remove dictionary unique indexes without replacing uniqueness and lookup behavior.
- Do not treat VACUUM size wins as semantic graph correctness wins.
- Do not collapse heuristic/static facts or source-span metadata for size.

## Tests

- `cargo test -p codegraph-cli audit::tests::storage_experiment --lib`
- `cargo test -p codegraph-cli --test cli_smoke audit_commands_write_storage_samples_and_relation_counts`
- `cargo test -p codegraph-cli --test cli_smoke`
- `cargo test --workspace`
- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
