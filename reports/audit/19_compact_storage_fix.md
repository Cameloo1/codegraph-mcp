# 19 Compact Storage Fix

Verdict: implemented only the storage optimization proven safe by phase 18.

`MVP.md` keeps SQLite as the local proof graph and requires exact graph/source verification to remain trustworthy. This phase therefore avoids deleting proof facts, changing benchmark scoring, or removing indexes needed by default query paths.

## Inputs Read

- `MVP.md`
- `reports/audit/18_storage_experiments.md`
- `reports/audit/03_storage_forensics.md`
- `reports/audit/artifacts/18_storage_experiments_latest.json`
- `reports/audit/artifacts/19_graph_truth_gate.json`
- `crates/codegraph-store/src/sqlite.rs`
- Current compact schema and bulk-index rebuild SQL in `crates/codegraph-store/src/sqlite.rs`

## Optimization Chosen

Only post-bulk `ANALYZE`, `PRAGMA optimize`, and `VACUUM` were promoted from experiment evidence into the production bulk indexing finish path.

The implemented path now:

1. Recreates the existing compact indexes after bulk load.
2. Runs `ANALYZE`.
3. Runs `PRAGMA optimize`.
4. Runs `VACUUM`.
5. Restores normal locking, WAL journaling, synchronous mode, and foreign keys.

Code locations:

- `crates/codegraph-store/src/sqlite.rs`: `BULK_INDEX_CREATE_SQL`
- `crates/codegraph-store/src/sqlite.rs`: `SqliteGraphStore::finish_bulk_index_load`

No schema version or migration change was needed because rows, columns, indexes, and semantics are unchanged.

## Optimizations Rejected

| Candidate | Decision | Reason |
| --- | --- | --- |
| Remove full qualified-name text storage | Rejected | Phase 03/18 prove bloat, but not a replacement uniqueness/export/lookup invariant. |
| Remove dictionary unique text indexes | Rejected | Unsafe without alternate uniqueness and exact lookup guarantees. |
| Drop `idx_edges_head_relation` | Rejected | Phase 18 degraded head/relation lookup from `0 ms` to `163 ms` on the latest DB copy. |
| Drop `idx_edges_tail_relation` | Rejected | Phase 18 degraded tail/relation lookup from `0 ms` to `166 ms` on the latest DB copy. |
| Drop `idx_edges_span_path` | Rejected | Not proven safe for stale-file deletion/update paths. |
| Add relation/exactness partial indexes | Rejected | Phase 17 identifies need, but phase 18 did not measure a safe candidate. |
| Disable FTS by default | Rejected | FTS was not a storage-heavy contributor in the latest DB. |

## Storage Measurements

### Latest Large DB Evidence From Phase 18

The copied Autoresearch DB experiment measured the safe optimization before implementation:

| Checkpoint | DB bytes | Edge index bytes | Dictionary table bytes | Unique text index bytes |
| --- | ---: | ---: | ---: | ---: |
| Before `VACUUM`/`ANALYZE` | 803495936 | 110051328 | 203608064 | 231276544 |
| After `VACUUM` | 744718336 | 110051328 | 203608064 | 197001216 |

Measured reduction: `58777600` bytes, about `56.1 MiB`.

### Fixture DB After This Patch

Artifact DB:

- `reports/audit/artifacts/19_storage_probe_repo/.codegraph/codegraph.sqlite`
- `reports/audit/artifacts/19_storage_probe_after.json`
- `reports/audit/artifacts/19_storage_experiments_fixture.json`

The patched indexer produced an already-compact small fixture DB:

| Measurement | Value |
| --- | ---: |
| Database bytes | 241664 |
| WAL bytes | 0 |
| SHM bytes during read-only inspection | 0 |
| Page count | 59 |
| Freelist count | 0 |
| Edge index bytes | 12288 |
| Dictionary table bytes | 20480 |
| Unique text index bytes | 20480 |

Copied fixture experiment:

| Checkpoint | DB bytes | Edge index bytes | Dict table bytes | Unique text index bytes |
| --- | ---: | ---: | ---: | ---: |
| Before | 241664 | 12288 | 20480 | 20480 |
| After `ANALYZE` | 241664 | 12288 | 20480 | 20480 |
| After `VACUUM` | 241664 | 12288 | 20480 | 20480 |

Interpretation: the new finish path leaves small fresh indexes compact immediately. The large phase 18 DB still shows why this maintenance matters after large bulk loads with prior free pages or b-tree fragmentation.

## Query Latency Smoke

Latest large copied-DB phase 18 `VACUUM`/`ANALYZE` run:

| Query | Before | After `VACUUM` | Index status |
| --- | ---: | ---: | --- |
| `entity_name_lookup` | 0 ms | 0 ms | `idx_entities_name` |
| `entity_qname_lookup` | 0 ms | 0 ms | `idx_entities_qname` |
| `edge_head_relation_lookup` | 0 ms | 0 ms | `idx_edges_head_relation` |
| `edge_tail_relation_lookup` | 0 ms | 0 ms | `idx_edges_tail_relation` |
| `edge_span_path_lookup` | 0 ms | 0 ms | `idx_edges_span_path` |
| `relation_count_scan` | 656 ms | 658 ms | covering `idx_edges_tail_relation` |

No degradation crossed the storage-experiment threshold.

Small fixture smoke after patch:

- `query unresolved-calls --limit 10 --json --no-snippets` returned `2` rows.
- Total reported elapsed time: `6 ms`.
- Snippets were not loaded.
- Query remains bounded and instrumented.

## Correctness Proof

No facts or indexes were removed. The patch only compacts the database after the bulk loader has already rebuilt the existing indexes.

Additional tests:

- `finish_bulk_index_load_vacuums_free_pages_without_losing_graph_facts`
- `finish_bulk_index_load_preserves_default_edge_lookup_indexes`

These tests verify that:

- `VACUUM` clears free pages created by deleted storage.
- entity, edge, and source-span row counts survive `finish_bulk_index_load`.
- `idx_edges_head_relation`, `idx_edges_tail_relation`, `idx_edges_span_path`, and `idx_source_spans_path` still exist.
- default head/relation and tail/relation edge lookups use the expected indexes.

Graph Truth after this patch:

| Result | Count |
| --- | ---: |
| Cases total | 10 |
| Cases passed | 6 |
| Cases failed | 4 |
| Forbidden edge hits | 0 |
| Source-span failures | 0 |

Passing fixtures stayed green:

- `admin_user_roles_not_conflated`
- `file_rename_prunes_old_path`
- `import_alias_change_updates_target`
- `same_name_only_one_imported`
- `source_span_exact_callsite`
- `stale_cache_after_delete`

Remaining failing fixtures are unchanged semantic/extraction gaps, not storage regressions:

- `derived_edge_requires_provenance`
- `dynamic_import_marked_heuristic`
- `mock_call_not_production_call`
- `sanitizer_exists_but_not_on_flow`

## Tests Run

- `cargo fmt --all`
- `cargo test -p codegraph-store finish_bulk_index_load`
- `cargo test -p codegraph-store`
- `cargo test -p codegraph-cli audit::tests::storage_experiment --lib`
- `cargo run -p codegraph-cli -- index reports\audit\artifacts\19_storage_probe_repo --profile --json`
- `cargo run -p codegraph-cli -- audit storage --db reports\audit\artifacts\19_storage_probe_repo\.codegraph\codegraph.sqlite --json reports\audit\artifacts\19_storage_probe_after.json --markdown reports\audit\artifacts\19_storage_probe_after.md`
- `cargo run -p codegraph-cli -- audit storage-experiments --db reports\audit\artifacts\19_storage_probe_repo\.codegraph\codegraph.sqlite --workdir reports\audit\artifacts\19_storage_experiments_fixture_work --json reports\audit\artifacts\19_storage_experiments_fixture.json --markdown reports\audit\artifacts\19_storage_experiments_fixture.md`
- `cargo run -p codegraph-cli -- bench graph-truth --cases benchmarks\graph_truth\fixtures --fixture-root . --out-json reports\audit\artifacts\19_graph_truth_gate.json --out-md reports\audit\artifacts\19_graph_truth_gate.md --fail-on-forbidden --fail-on-missing-source-span`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo run -p codegraph-cli -- query unresolved-calls --db reports\audit\artifacts\19_storage_probe_repo\.codegraph\codegraph.sqlite --limit 10 --offset 0 --json --no-snippets`

## Next Work

Before any further storage change, measure candidate relation/exactness partial indexes and qualified-name dictionary redesigns on copied DBs. Do not remove dictionary text, uniqueness indexes, edge lookup indexes, source spans, or FTS until a copied-DB experiment proves size wins, query safety, and graph-truth preservation.
