# Storage And Index Forensics

Timestamp: 2026-05-10 22:18:47 -05:00

## Verdict

Status: `measured`.

No production storage optimization was performed. This phase only improved `codegraph-mcp audit storage`, ran it on current and frozen Autoresearch-scale SQLite artifacts, and wrote evidence artifacts.

Primary artifact:

- JSON: `reports/audit/artifacts/storage_forensics.json`
- Markdown: `reports/audit/artifacts/storage_forensics.md`

Comparison artifact:

- JSON: `reports/audit/artifacts/storage_forensics_frozen_baseline.json`
- Markdown: `reports/audit/artifacts/storage_forensics_frozen_baseline.md`

## Important Integrity Finding

The current phase-17 Autoresearch status DB is readable for many queries but fails SQLite integrity check.

| DB | Integrity | File family bytes | Edges | Bytes per edge |
| --- | --- | ---: | ---: | ---: |
| `reports/audit/artifacts/17_autoresearch_cold_only.sqlite` | failed | 480,669,696 | 1,044,611 | 460.14 |
| `target/codegraph-bench-report/sweep-20260509-231456/autoresearch-codegraph-attempt3/codegraph.sqlite` | ok | 803,528,704 | 2,050,123 | 391.94 |

This explains the phase-17 `database disk image is malformed` update failure. Current DB storage totals remain useful as a forensic signal, but optimization decisions should be validated against a fresh integrity-clean DB.

## Storage Target Gap

Targets:

- Autoresearch-scale SQLite family intended: `<=250 MiB` (`262,144,000` bytes)
- Stretch: `<=150 MiB` (`157,286,400` bytes)
- Bytes per edge including indexes/dictionaries intended: `<=120`
- Stretch bytes per edge: `<=80`

| Artifact | Size | Gap to 250 MiB | Gap to 150 MiB | Bytes/edge | Gap to 120 B/edge |
| --- | ---: | ---: | ---: | ---: | ---: |
| Current phase-17 DB | 480,669,696 | 218,525,696 | 323,383,296 | 460.14 | 340.14 |
| Frozen baseline DB | 803,528,704 | 541,384,704 | 646,242,304 | 391.94 | 271.94 |

The frozen baseline is integrity-clean and still misses the intended size target by about `516.30 MiB`. The current phase-17 artifact is smaller but not integrity-clean, so it cannot be used to claim a storage win.

## Top 10 Storage Contributors

Current phase-17 DB:

| Rank | Object | Type | Rows | Bytes |
| ---: | --- | --- | ---: | ---: |
| 1 | `edges` | table | 1,044,611 | 154,542,080 |
| 2 | `sqlite_autoindex_qualified_name_dict_1` | index | unknown | 61,640,704 |
| 3 | `qualified_name_dict` | table | 464,610 | 57,090,048 |
| 4 | `entities` | table | 437,263 | 31,379,456 |
| 5 | `sqlite_autoindex_object_id_dict_1` | index | unknown | 24,690,688 |
| 6 | `object_id_dict` | table | 437,978 | 22,188,032 |
| 7 | `idx_edges_tail_relation` | index | unknown | 19,853,312 |
| 8 | `idx_edges_head_relation` | index | unknown | 19,841,024 |
| 9 | `sqlite_autoindex_symbol_dict_1` | index | unknown | 16,699,392 |
| 10 | `idx_edges_span_path` | index | unknown | 16,666,624 |

Frozen baseline DB:

| Rank | Object | Type | Rows | Bytes |
| ---: | --- | --- | ---: | ---: |
| 1 | `edges` | table | 2,050,123 | 165,593,088 |
| 2 | `sqlite_autoindex_qualified_name_dict_1` | index | unknown | 125,501,440 |
| 3 | `qualified_name_dict` | table | 803,354 | 109,445,120 |
| 4 | `entities` | table | 865,896 | 62,488,576 |
| 5 | `sqlite_autoindex_object_id_dict_1` | index | unknown | 48,812,032 |
| 6 | `object_id_dict` | table | 865,896 | 43,884,544 |
| 7 | `idx_edges_head_relation` | index | unknown | 38,608,896 |
| 8 | `idx_edges_tail_relation` | index | unknown | 38,608,896 |
| 9 | `idx_edges_span_path` | index | unknown | 32,833,536 |
| 10 | `sqlite_autoindex_symbol_dict_1` | index | unknown | 30,224,384 |

## Category Breakdown

| Category | Current phase-17 DB | Frozen baseline DB |
| --- | ---: | ---: |
| Dictionary tables | 105,664,512 | 203,608,064 |
| Dictionary unique indexes | 116,621,312 | 231,276,544 |
| Edge indexes | 56,360,960 | 110,051,328 |
| FTS/shadow tables | 73,728 | 20,480 |
| Source-span table/index | 8,192 | 8,192 |

Source snippets are not the main SQLite storage problem in these artifacts. Frozen baseline has zero FTS rows, and current phase-17 has 98 FTS rows, all `entity`; `stores_source_snippets` is `false`.

The standalone `source_spans` table is also not the large footprint right now. It has zero rows in both audited DBs because proof spans are stored inline on edges/entities in the compact schema.

## Edge Byte Metrics

| Metric | Current phase-17 DB | Frozen baseline DB |
| --- | ---: | ---: |
| Edge table bytes | 154,542,080 | 165,593,088 |
| Edge index bytes | 56,360,960 | 110,051,328 |
| Edge table bytes/edge | 147.94 | 80.77 |
| Edge table + edge indexes bytes/edge | 201.90 | 134.45 |
| Full DB bytes/edge | 460.14 | 391.94 |

Even when the frozen baseline has a better edge-table density, the full DB is far above the 120 bytes/edge intended target because dictionaries and dictionary unique indexes dominate.

## Query Plan Summary

The expanded audit command now lists every index and maps it to default workflow usage in `storage_forensics.json`. Core query plans were captured for symbol query, text query, relation query, context-pack expansion, impact traversal, and unresolved-calls pagination.

Current phase-17 DB:

| Query | Indexes observed | Full scans observed |
| --- | --- | --- |
| `symbol_query_exact_name` | `idx_entities_name` | `SCAN entities` |
| `text_query_fts` | none reported by parser | `SCAN stage0_fts VIRTUAL TABLE INDEX 0:M6` |
| `relation_query_calls` | `sqlite_autoindex_relation_kind_dict_1` | `SCAN e` |
| `context_pack_outbound` | `idx_edges_head_relation` | subquery scans over `edges` |
| `impact_inbound` | `idx_edges_tail_relation` | subquery scans over `edges` |
| `unresolved_calls_paginated` | `sqlite_autoindex_relation_kind_dict_1`, PK lookups | `SCAN e` |

Index usage highlights:

| Index | Bytes | Core plan usage | Forensic judgment |
| --- | ---: | --- | --- |
| `idx_edges_head_relation` | 19,841,024 | `context_pack_outbound` | keep until a replacement traversal index is proven |
| `idx_edges_tail_relation` | 19,853,312 | `impact_inbound` | keep until a replacement traversal index is proven |
| `idx_edges_span_path` | 16,666,624 | none in core plans | likely experiment candidate |
| `idx_entities_name` | 5,136,384 | `symbol_query_exact_name` | keep or replace with proven seed-resolution index |
| `idx_entities_qname` | 5,201,920 | none in current core plans | experiment candidate, but exact qname lookup must stay fast |
| `idx_entities_path` | 4,747,264 | none in current core plans | experiment candidate only if stale cleanup/list-by-file stays fast |
| `idx_source_spans_path` | 4,096 | none in current core plans | likely removable if source_spans remains empty |
| `idx_retrieval_traces_created` | 4,096 | none in current core plans | low-value but tiny |

## Top 5 Suspected Redundant Structures

1. `qualified_name_dict.value` stores full qualified-name text while `prefix_id` and `suffix_id` are also stored. Frozen baseline full qname text alone is 92,568,940 bytes, plus a 125,501,440 byte unique index.
2. Dictionary unique indexes duplicate large text payloads: frozen baseline unique text indexes total 231,276,544 bytes.
3. `idx_edges_span_path` is broad and not used by audited core query plans. It costs 32,833,536 bytes in the frozen baseline.
4. `idx_entities_qname` and `idx_entities_path` are useful for specific paths, but not observed in the core plan sample. They need workload-specific proof.
5. Heuristic/test/debug-grade facts share the same hot `edges` table as exact/base facts. Current phase-17 has 269,257 `static_heuristic` edges, 307,104 `test` edges, and 171,360 `unknown` class edges mixed into the same table.

## Safe Optimization Candidates To Experiment With

These are candidates for copied-DB experiments only:

1. Drop or replace `idx_edges_span_path` and rerun source-span/audit/UI path queries plus stale cleanup tests.
2. Replace full `qualified_name_dict.value` with reconstructed prefix/suffix reads or a compact hash key, while preserving exact qname output.
3. Measure a dictionary-key redesign that avoids huge unique text autoindexes for long object IDs and qualified names.
4. Add a relation-first edge index or partial `CALLS` heuristic index for unresolved-calls pagination, because the current plan scans `edges`.
5. Split or partition proof-grade base edges from heuristic/test/debug edges, but only after graph truth and context packet behavior is locked.

## Unsafe Optimization Candidates

Do not do these as blind storage cuts:

1. Do not drop `idx_edges_head_relation` or `idx_edges_tail_relation`; they are used by context-pack and impact traversal plans.
2. Do not remove exactness, edge class, context, provenance, or source-span metadata to save bytes.
3. Do not silently delete heuristic/test/mock facts; they must either remain queryable with explicit class labels or move to a clearly separated store.
4. Do not remove dictionary unique indexes without a replacement interning/lookup strategy.
5. Do not optimize against the corrupt phase-17 DB alone. Use an integrity-clean fresh DB and the frozen baseline for comparison.

## Next Experiments

1. Reproduce Autoresearch indexing into a fresh integrity-clean schema-5 DB.
2. Run `audit storage` before and after `VACUUM; ANALYZE` on a copied DB.
3. On a copied DB, drop only `idx_edges_span_path`, rerun core query plans, relation sampler, source-span audit, and stale mutation tests.
4. Prototype qualified-name reconstruction on a copy and measure the qname table plus unique-index delta.
5. Prototype unresolved-calls pagination indexes and require EXPLAIN to stop scanning the full edge table.
6. Only after those copied-DB experiments pass semantic gates, consider production schema/migration changes.
