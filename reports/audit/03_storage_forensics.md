# 03 Storage Forensics Audit

Verdict: the compact-memory goal is partially met, but dictionary and index bloat is real.

`MVP.md` requires local SQLite storage with compact, proof-oriented graph facts. The current schema uses integer-backed compact rows for `entities`, `edges`, `files`, and `source_spans`, but the latest large DB is still dominated by text dictionaries and their unique indexes. No storage optimization was applied in this phase.

## Sources Inspected

- `MVP.md`
- `reports/audit/02_graph_schema_edge_taxonomy.md`
- `crates/codegraph-store/src/sqlite.rs`
- `crates/codegraph-store/src/traits.rs`
- `crates/codegraph-index/src/lib.rs`
- `crates/codegraph-cli/src/lib.rs`
- `reports/audit/artifacts/03_storage_latest.json`
- `reports/audit/artifacts/03_storage_fixture.json`
- `target/codegraph-bench-report/sweep-20260509-231456/autoresearch-codegraph-attempt3/status.json`

## Inspection Commands

The new read-only storage inspection command is:

```powershell
.\target\debug\codegraph-mcp.exe audit storage --db <path> --json <out.json> --markdown <out.md>
```

It uses SQLite `dbstat` when available and writes both JSON and Markdown. It also works on a tiny fixture DB and does not require the full Autoresearch repo.

Executed artifacts:

- `reports/audit/artifacts/03_storage_fixture.json`
- `reports/audit/artifacts/03_storage_fixture.md`
- `reports/audit/artifacts/03_storage_latest.json`
- `reports/audit/artifacts/03_storage_latest.md`

The prior benchmark status recorded `db_size_bytes = 842441752`, about 803.4 MiB. The current read-only dbstat pass sees `803495936` bytes, about 766.3 MiB, with no WAL/SHM files present. The difference is consistent with roughly 37.1 MiB of WAL/sidecar bytes existing when the benchmark status was captured.

## Size Breakdown

Latest DB: `target/codegraph-bench-report/sweep-20260509-231456/autoresearch-codegraph-attempt3/codegraph.sqlite`.

| Object | Type | Rows | Bytes | DB % | Risk |
| --- | --- | ---: | ---: | ---: | --- |
| `edges` | table | 2050123 | 165593088 | 20.61 | Expected large base fact table. |
| `sqlite_autoindex_qualified_name_dict_1` | index | unknown | 125501440 | 15.62 | Very high; unique index over full qualified-name text. |
| `qualified_name_dict` | table | 803354 | 109445120 | 13.62 | Very high; stores full qualified names despite prefix/suffix ids. |
| `entities` | table | 865896 | 62488576 | 7.78 | Expected large entity table. |
| `sqlite_autoindex_object_id_dict_1` | index | unknown | 48812032 | 6.07 | High; unique index over full object ids. |
| `object_id_dict` | table | 865896 | 43884544 | 5.46 | High; stores full stable ids. |
| `idx_edges_head_relation` | index | unknown | 38608896 | 4.81 | Used by graph traversal. |
| `idx_edges_tail_relation` | index | unknown | 38608896 | 4.81 | Used by reverse traversal. |
| `idx_edges_span_path` | index | unknown | 32833536 | 4.09 | Used by stale-file deletion. |
| `sqlite_autoindex_symbol_dict_1` | index | unknown | 30224384 | 3.76 | Used by exact symbol lookup/interning. |
| `symbol_dict` | table | 789647 | 26824704 | 3.34 | High but expected for symbol lookup. |
| `sqlite_autoindex_qname_prefix_dict_1` | index | unknown | 26296320 | 3.27 | High; prefix dictionary uniqueness. |
| `qname_prefix_dict` | table | 198793 | 23056384 | 2.87 | High relative to prefix count. |

Dictionary table value bytes plus their unique indexes are the central bloat:

| Dictionary | Rows | Value bytes | Unique index bytes |
| --- | ---: | ---: | ---: |
| `qualified_name_dict` | 803354 | 92568940 | 125501440 |
| `object_id_dict` | 865896 | 35501736 | 48812032 |
| `symbol_dict` | 789647 | 19417716 | 30224384 |
| `qname_prefix_dict` | 198793 | 20572503 | 26296320 |
| `path_dict` | 4975 | 341970 | 442368 |

`source_spans` and `stage0_fts` are not storage contributors in this latest DB. `source_spans` has 0 rows and 4096 bytes; FTS shadow tables total 20480 bytes.

## Redundant Text Storage

Full qualified names are stored redundantly.

`qualified_name_dict` stores:

- `prefix_id`
- `suffix_id`
- full `value TEXT NOT NULL UNIQUE`

The latest DB has:

- `qualified_name_dict.row_count = 803354`
- full qualified-name text bytes: `92568940`
- prefix text bytes referenced by those rows: `75923800`
- suffix text bytes referenced by those rows: `15855240`
- unique index bytes for full qualified-name text: `125501440`

The prefix/suffix columns are not enough to avoid storing full text because the table still keeps the full `value` and a unique b-tree over that value. That is the largest single storage concern.

Object ids are also stored as full text in `object_id_dict`; this is currently needed for stable ids and lookup, but the table plus autoindex costs about 92.7 MB in the latest DB.

## Index Use and Risk

| Index | Approx bytes | Current query path | Audit verdict |
| --- | ---: | --- | --- |
| `sqlite_autoindex_qualified_name_dict_1` | 125501440 | `lookup_qualified_name`, exact symbol lookup, qualified-name interning. | Biggest candidate for redesign, unsafe to remove without alternate uniqueness/lookup. |
| `sqlite_autoindex_object_id_dict_1` | 48812032 | `lookup_object_id`, `get_entity`, `get_edge`, edge endpoint lookups, deletes. | Necessary under current schema. |
| `idx_edges_head_relation` | 38608896 | `find_edges_by_head_relation`, forward traversal, derived `DEFINED_IN` fallback. | Necessary until query-plan measurements prove a replacement. |
| `idx_edges_tail_relation` | 38608896 | `find_edges_by_tail_relation`, callers/reverse traversal, derived `DEFINED_IN` fallback. | Necessary until query-plan measurements prove a replacement. |
| `idx_edges_span_path` | 32833536 | `delete_facts_for_file` deletes stale edges by `span_path_id`. | Likely necessary for incremental updates; measure alternatives on copy. |
| `sqlite_autoindex_symbol_dict_1` | 30224384 | `lookup_symbol`, exact symbol lookup, symbol interning. | Necessary under current lookup model. |
| `sqlite_autoindex_qname_prefix_dict_1` | 26296320 | prefix interning for qualified names. | Candidate for redesign if full-qname storage changes. |
| `idx_entities_qname` | 10362880 | exact symbol lookup after resolving qname id. | Useful, small relative to dictionaries. |
| `idx_entities_name` | 10231808 | exact symbol lookup after resolving name id. | Useful, small relative to dictionaries. |
| `idx_entities_path` | 9482240 | `list_entities_by_file`, `delete_facts_for_file`. | Useful for incremental indexing. |

There is no current broad `idx_edges_relation` index. Relation-only scans such as `EDGE_SELECT_BY_RELATION` can therefore scan broadly. Relation-specific partial indexes may help some high-risk queries, but they must be measured against DB size and write cost.

## Bulk Load Timing

Bulk indexing does not appear to create edge/entity lookup indexes too early for large loads. `crates/codegraph-index/src/lib.rs` calls `begin_bulk_index_load` before batch writes, and `crates/codegraph-store/src/sqlite.rs` drops major indexes there. `finish_bulk_index_load` recreates them after the final transaction and runs `PRAGMA optimize`.

There is a small initialization churn because migration creates the indexes and the first bulk load drops them, but that is unlikely to explain the 803.4 MiB family size.

## VACUUM and ANALYZE

This audit did not run `VACUUM` or `ANALYZE` on the production/latest DB. The storage command records page metrics only:

- `page_size_bytes = 4096`
- `page_count = 196166`
- `freelist_count = 0`
- current read-only file family bytes: `803495936`

Because `freelist_count` is 0, `VACUUM` may not recover much from free pages, but it could still change b-tree packing. Measure only on a copied DB.

## Storage Risk Ranking

1. `qualified_name_dict` full text plus unique index: largest likely avoidable cost.
2. `object_id_dict` full ids plus unique index: large, currently foundational.
3. Edge table plus head/tail/span indexes: large but structurally expected for 2.05M edges.
4. `symbol_dict` and `qname_prefix_dict` autoindexes: meaningful but secondary.
5. Missing standalone `source_spans` rows: not a size issue, but an auditability/schema consistency issue.

## Safe Optimization Candidates

These are safe to measure, not safe to apply blindly:

- On a copied DB, run `VACUUM`, `ANALYZE`, and `PRAGMA optimize`, then compare file bytes, page count, query plans, and key query latencies.
- Prototype a qualified-name dictionary variant that enforces uniqueness over `(prefix_id, suffix_id)` and removes or defers full `value` storage; compare exact symbol lookup speed and bundle export compatibility.
- Run `EXPLAIN QUERY PLAN` and latency measurements for `find_edges_by_head_relation`, `find_edges_by_tail_relation`, stale-file deletion, exact symbol lookup, relation counts, and context-pack traversal before considering any index change.
- Build a copied-DB experiment with relation-specific partial indexes for `CALLS`, `CALLEE`, `FLOWS_TO`, `READS`, `WRITES`, `TESTS`, `MOCKS`, and `ASSERTS`, then compare size/write/query costs.
- Measure whether a compact object-id surrogate plus reversible side table can reduce `object_id_dict` and autoindex size without breaking stable external ids.

## Unsafe Optimization Candidates

Do not do these yet:

- Do not remove `idx_edges_head_relation` or `idx_edges_tail_relation`; they protect exact graph traversal.
- Do not remove dictionary unique indexes without replacing uniqueness and lookup invariants.
- Do not drop full qualified names until symbol search, bundle export/import, MCP output, and exact qname lookup have an equivalent path.
- Do not remove `CONTAINS`/`DEFINED_IN` or callsite edges merely because they dominate storage; the graph correctness audit has not proven which are redundant.
- Do not collapse parser-verified and static-heuristic facts to save space; MVP proof labels must remain visible.

## Recommended Measurements Before Changes

1. Copy the latest DB and run before/after `VACUUM`, `ANALYZE`, and `PRAGMA optimize` measurements.
2. Capture `EXPLAIN QUERY PLAN` for every indexed query path listed above.
3. Benchmark relation-count, edge-sample, context-pack, callers/callees, and stale-file deletion on the current DB.
4. Build a one-off alternate qualified-name dictionary schema and compare size plus exact lookup latency.
5. Build a one-off partial-index variant and compare read latency against added index bytes.
6. Re-run the audit sampler after each copied-DB experiment to prove source spans, exactness, and metadata are still readable.
