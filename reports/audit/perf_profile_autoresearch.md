# Autoresearch Phase-Level Performance Profile

Generated: 2026-05-11 11:18:16 -05:00

Run ID: `20260511_105332`

Verdict: `profiled_fail_targets_visible`.

This is profiling-only work. It does not change graph semantics, resolver behavior, storage layout, or benchmark scoring. The clean Autoresearch DB remains integrity-clean, and the strict Graph Truth Gate with `--update-mode` still passes 11/11 fixtures.

## Artifacts

| Artifact | Path |
| --- | --- |
| Consolidated JSON | `reports/audit/perf_profile_autoresearch.json` |
| Cold index profile | `reports/audit/artifacts/perf_profile_autoresearch_cold_index_20260511_105332.json` |
| Cold raw JSONL | `reports/audit/artifacts/perf_profile_autoresearch_cold_index_20260511_105332.raw.jsonl` |
| Repeat index profile | `reports/audit/artifacts/perf_profile_autoresearch_repeat_index_20260511_105332.json` |
| Repeat raw JSONL | `reports/audit/artifacts/perf_profile_autoresearch_repeat_index_20260511_105332.raw.jsonl` |
| Update integrity profile | `reports/audit/artifacts/perf_profile_autoresearch_update_integrity_20260511_105332.json` |
| Context pack profile | `reports/audit/artifacts/perf_profile_autoresearch_context_pack_20260511_105332.json` |
| Storage audit profile input | `reports/audit/artifacts/perf_profile_autoresearch_storage_20260511_105332.json` |

## Summary

| Operation | Target | Observed | Status | Primary cause |
| --- | ---: | ---: | --- | --- |
| Cold index | 60,000 ms | 599,595 ms profile, 601,466 ms shell | fail | edge writes, dictionary interning, entity writes, full integrity gates, atomic final validation |
| Repeat unchanged | 5,000 ms | 13,060 ms profile, 15,989 ms shell | fail | opening the large SQLite DB, then quick integrity check |
| Single-file update | 750 ms | 64,020 ms profiled update, 65,214 ms step wall | fail | full cache refresh and 1,000,000-edge reload after one changed file |
| Context pack | 2,000 ms | 28,285 ms profile, 29,181 ms shell | fail | full edge preload with table scan plus large DB open |
| Storage audit | offline | 80,012 ms | measured | dbstat/storage accounting on 1.71 GB DB |

## Cold Index Breakdown

Cold profile wall: 599,595 ms. Shell wall: 601,466 ms. Profile-vs-shell delta: 1,871 ms, 0.31%.

The earlier unexplained cold-index gap is now accounted for: the atomic temp DB finalization and final visible DB validation take 146,161 ms. The inner index is therefore about 453,434 ms.

| Span | Time | Share of profile wall | Count / items |
| --- | ---: | ---: | ---: |
| `integrity_check` | 187,290 ms | 31.24% | 3 / 0 |
| `edge_insert` | 164,363 ms | 27.41% | 3,866,873 / 3,866,873 |
| `dictionary_lookup_insert` / `symbol_interning` | 144,145 ms | 24.04% | 45,618,322 / 45,618,322 |
| `entity_insert` | 62,563 ms | 10.43% | 1,630,529 / 1,630,529 |
| `reducer` | 30,753 ms | 5.13% | 39 / 4,975 |
| `extract_entities_and_relations` | 23,655 ms | 3.95% | 4,975 / 4,975 |
| `index_creation` / `fts_build` | 9,654 ms | 1.61% | 2 / 0 |

Notes: SQLite and dictionary spans are nested inside higher-level entity/edge insertion spans, so they are diagnostic costs, not additive exclusive time. Source spans are currently carried through edge writes in this storage path, so `source_span_insert` is present but zero for this run.

## Repeat Unchanged Breakdown

Repeat profile wall: 13,060 ms. Files read, hashed, and parsed: 0. Selected internal spans explain all but 269 ms, about 2.06% of profile wall.

| Span | Time | Share |
| --- | ---: | ---: |
| `open_store` | 9,451 ms | 72.36% |
| `integrity_check` | 2,908 ms | 22.27% |
| `file_walk` | 333 ms | 2.55% |
| `metadata_diff` | 93 ms | 0.71% |
| `wal_checkpoint` | 7 ms | 0.05% |

This means repeat unchanged is not spending time parsing. It is mostly paying the cost of opening a 1.71 GB SQLite DB and running the post-index quick integrity gate.

## Single-File Update Breakdown

Mutation file: `python/autoresearch_utils/metrics_tools.py`.

The incremental update read, hashed, and parsed exactly one file. It inserted 18 entities and 35 edges, skipped 0 duplicate edges, deleted current facts for 1 file, and marked 4,096 PathEvidence rows dirty/refreshed. Selected internal spans explain all but 208 ms of the 64,020 ms update profile, about 0.33%.

| Span | Time | Share |
| --- | ---: | ---: |
| `cache_refresh` | 33,792 ms | 52.78% |
| `sql_query_execution` | 13,788 ms | 21.54% |
| `open_store` | 8,921 ms | 13.93% |
| `integrity_check` | 5,849 ms | 9.14% |
| `path_evidence_generation` | 1,252 ms | 1.96% |
| `stale_fact_delete` | 150 ms | 0.23% |

The highest-priority update bug is explicit now: after a one-file mutation, `cache_refresh` rebuilds in-memory signatures and adjacency by loading large global state. The `sql_query_execution` span is the 1,000,000-edge load used by that refresh. The update harness also ran a graph fact hash check (15,589 ms) and post-step full integrity check (72,234 ms); those validation costs are recorded separately from the update step wall.

Single-file update flags:

| Field | Value |
| --- | --- |
| Changed files | `python/autoresearch_utils/metrics_tools.py` |
| Deleted fact files | 1 |
| Inserted facts | 18 entities, 35 edges |
| Dirty PathEvidence count | 4,096 |
| Global hash check ran | true |
| Integrity check ran | true |
| Storage audit ran | false |

## Context Pack Breakdown

Context pack profile wall: 28,285 ms. Shell wall: 29,181 ms. The selected spans explain all but about 12 ms.

| Span | Time | Share |
| --- | ---: | ---: |
| `sql_query_execution` edge load | 15,716 ms | 55.56% |
| `open_store` | 10,649 ms | 37.65% |
| `context_engine_build` | 1,628 ms | 5.76% |
| `snippet_loading` | 281 ms | 0.99% |
| `context_pack_graph_and_packet` | 0.14 ms | <0.01% |
| `json_serialization` | 0.01 ms | <0.01% |

Candidate seeds: 3. Candidate paths after filtering: 0. Snippets loaded: 4,975 files, 62,183,978 source bytes.

The context packet query path is not slow because path ranking itself is slow. It is slow because the current CLI path opens the huge DB, preloads up to 1,000,000 edges, builds an in-memory graph, and preloads all indexed source files before the packet work.

## Context SQL Plans

`load_all_edges_for_context_pack`:

```text
SCAN e
SEARCH exactness USING INTEGER PRIMARY KEY (rowid=?)
SEARCH relation USING INTEGER PRIMARY KEY (rowid=?)
SEARCH span_path USING INTEGER PRIMARY KEY (rowid=?)
SEARCH head USING INTEGER PRIMARY KEY (rowid=?)
SEARCH tail USING INTEGER PRIMARY KEY (rowid=?)
SEARCH extractor USING INTEGER PRIMARY KEY (rowid=?)
```

`list_files_for_context_pack_sources`:

```text
SCAN path USING COVERING INDEX sqlite_autoindex_path_dict_1
SEARCH f USING PRIMARY KEY (path_id=?)
```

SQLite row-scan counts are not exposed through the current `rusqlite` path; the profile emits `EXPLAIN QUERY PLAN`, rows returned, candidate counts, and snippet counts instead.

## Required Span Coverage

The profile schema now emits the requested spans for indexing and context work: `file_walk`, `metadata_diff`, `file_read`, `file_hash`, `parse`, `local_fact_bundle_creation`, `reducer`, `symbol_interning`, `dictionary_lookup_insert`, `entity_insert`, `edge_insert`, `source_span_insert`, `PathEvidence generation`, `FTS build`, `index_creation`, `integrity_check`, `SQL query execution`, `snippet_loading`, and `JSON serialization`.

Additional spans were added because they were real hidden costs: `open_store`, `cache_refresh`, `wal_checkpoint`, `stale_fact_delete`, `atomic_temp_db_finalize`, `atomic_db_replace`, and `atomic_final_db_validate`.

`graph_fact_hash` is timed in the update-integrity harness. The single-file update hash check took 15,589 ms. Storage audit is timed as an offline command and took 80,012 ms.

## Verification

| Check | Result |
| --- | --- |
| `cargo test -q` | passed |
| Graph Truth Gate with `--update-mode` | passed, 11/11 |
| DB integrity in storage audit | ok |
| Semantic behavior changes | none intended; profiling-only instrumentation |

A non-update-mode graph-truth smoke produced 8/11 because mutation fixtures were not replayed. The strict update-mode gate is the relevant semantic check and passed 11/11.

## Next Fixes

1. Remove the global cache refresh from single-file update; update only affected signatures/adjacency or defer refresh behind an explicit query need.
2. Replace `context_pack` full-edge preload with DB-backed seeded expansion or a bounded relation/path query.
3. Avoid opening the full SQLite store for no-op repeat paths when metadata diff can prove no writes are needed.
4. Revisit integrity-gate policy for benchmarking: keep correctness gates, but separate user-facing update latency from optional full validation sweeps.
5. Continue storage work only after the query paths stop requiring million-edge global loads.
