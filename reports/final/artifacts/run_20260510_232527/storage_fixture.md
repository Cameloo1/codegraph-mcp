# Storage Inspection

Database: `<REPO_ROOT>/Desktop/development/codegraph-mcp/reports/final/artifacts/run_20260510_232527/fixture_workers4.sqlite`

- DBSTAT available: `true`
- Database bytes: `630784`
- WAL bytes: `0`
- SHM bytes: `32768`
- File family bytes: `663552`
- Page size: `4096`
- Page count: `154`
- Freelist count: `3`

## Integrity Check

- Status: `ok`
- Checked: `true`
- Max errors captured: `20`

- `ok`

## Aggregate Metrics

- Tables: `29`
- Indexes: `26`
- Observed table rows: `3240`
- Edge count: `833`
- Average database bytes per edge: `796.58`
- Average edge table bytes per edge: `83.59`
- Average edge table plus edge-index bytes per edge: `152.43`
- Source-span rows: `0`
- Average source-span table bytes per row: `0.00`

## Category Breakdown

| Category | Bytes |
| --- | ---: |
| Dictionary tables | 114688 |
| Dictionary unique indexes | 110592 |
| Edge indexes | 57344 |
| Source-span table/index | 8192 |
| FTS/shadow tables | 28672 |
| Snippet-like objects | 0 |

## Table and Index Sizes

| Object | Type | Rows | Bytes | Payload | Unused | DB % |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `edges` | `table` | 833 | 69632 | 59730 | 7136 | 11.04 |
| `qualified_name_dict` | `table` | 408 | 40960 | 32380 | 6493 | 6.49 |
| `sqlite_stat4` | `table` | 288 | 40960 | 25752 | 13653 | 6.49 |
| `sqlite_autoindex_qualified_name_dict_1` | `index` | unknown | 36864 | 31109 | 4405 | 5.84 |
| `entities` | `table` | 429 | 32768 | 26052 | 5320 | 5.19 |
| `object_id_dict` | `table` | 487 | 28672 | 21312 | 4953 | 4.55 |
| `sqlite_autoindex_object_id_dict_1` | `index` | unknown | 28672 | 22158 | 4973 | 4.55 |
| `idx_edges_head_relation` | `index` | unknown | 20480 | 11727 | 6198 | 3.25 |
| `idx_edges_tail_relation` | `index` | unknown | 20480 | 11725 | 6200 | 3.25 |
| `qname_prefix_dict` | `table` | 165 | 20480 | 11891 | 7816 | 3.25 |
| `sqlite_autoindex_qname_prefix_dict_1` | `index` | unknown | 20480 | 12093 | 7824 | 3.25 |
| `sqlite_autoindex_symbol_dict_1` | `index` | unknown | 20480 | 11219 | 7915 | 3.25 |
| `symbol_dict` | `table` | 430 | 20480 | 10487 | 7902 | 3.25 |
| `idx_edges_span_path` | `index` | unknown | 16384 | 9567 | 4274 | 2.60 |
| `sqlite_schema` | `schema` | unknown | 16384 | 7786 | 8200 | 2.60 |
| `files` | `table` | 28 | 12288 | 7733 | 4411 | 1.95 |
| `stage0_fts_content` | `table` | 17 | 12288 | 5226 | 6942 | 1.95 |
| `bench_runs` | `table` | 0 | 4096 | 0 | 4088 | 0.65 |
| `bench_tasks` | `table` | 0 | 4096 | 0 | 4088 | 0.65 |
| `derived_edges` | `table` | 0 | 4096 | 0 | 4088 | 0.65 |
| `edge_class_dict` | `table` | 7 | 4096 | 83 | 3977 | 0.65 |
| `edge_context_dict` | `table` | 4 | 4096 | 37 | 4035 | 0.65 |
| `entity_kind_dict` | `table` | 22 | 4096 | 234 | 3766 | 0.65 |
| `exactness_dict` | `table` | 2 | 4096 | 37 | 4043 | 0.65 |
| `extractor_dict` | `table` | 7 | 4096 | 235 | 3825 | 0.65 |
| `idx_entities_name` | `index` | unknown | 4096 | 2727 | 74 | 0.65 |
| `idx_entities_path` | `index` | unknown | 4096 | 2403 | 398 | 0.65 |
| `idx_entities_qname` | `index` | unknown | 4096 | 2738 | 63 | 0.65 |
| `idx_retrieval_traces_created` | `index` | unknown | 4096 | 0 | 4088 | 0.65 |
| `idx_source_spans_path` | `index` | unknown | 4096 | 0 | 4088 | 0.65 |
| `language_dict` | `table` | 1 | 4096 | 13 | 4071 | 0.65 |
| `path_dict` | `table` | 28 | 4096 | 1569 | 2407 | 0.65 |
| `path_evidence` | `table` | 0 | 4096 | 0 | 4088 | 0.65 |
| `relation_kind_dict` | `table` | 27 | 4096 | 290 | 3690 | 0.65 |
| `repo_index_state` | `table` | 1 | 4096 | 201 | 3882 | 0.65 |
| `retrieval_traces` | `table` | 0 | 4096 | 0 | 4088 | 0.65 |
| `source_spans` | `table` | 0 | 4096 | 0 | 4088 | 0.65 |
| `sqlite_autoindex_bench_runs_1` | `index` | unknown | 4096 | 0 | 4088 | 0.65 |
| `sqlite_autoindex_bench_tasks_1` | `index` | unknown | 4096 | 0 | 4088 | 0.65 |
| `sqlite_autoindex_derived_edges_1` | `index` | unknown | 4096 | 0 | 4088 | 0.65 |
| `sqlite_autoindex_edge_class_dict_1` | `index` | unknown | 4096 | 89 | 3978 | 0.65 |
| `sqlite_autoindex_edge_context_dict_1` | `index` | unknown | 4096 | 40 | 4036 | 0.65 |
| `sqlite_autoindex_entity_kind_dict_1` | `index` | unknown | 4096 | 255 | 3767 | 0.65 |
| `sqlite_autoindex_exactness_dict_1` | `index` | unknown | 4096 | 38 | 4044 | 0.65 |
| `sqlite_autoindex_extractor_dict_1` | `index` | unknown | 4096 | 241 | 3826 | 0.65 |
| `sqlite_autoindex_language_dict_1` | `index` | unknown | 4096 | 13 | 4072 | 0.65 |
| `sqlite_autoindex_path_dict_1` | `index` | unknown | 4096 | 1596 | 2408 | 0.65 |
| `sqlite_autoindex_path_evidence_1` | `index` | unknown | 4096 | 0 | 4088 | 0.65 |
| `sqlite_autoindex_relation_kind_dict_1` | `index` | unknown | 4096 | 316 | 3691 | 0.65 |
| `sqlite_autoindex_repo_index_state_1` | `index` | unknown | 4096 | 95 | 3990 | 0.65 |
| `sqlite_autoindex_retrieval_traces_1` | `index` | unknown | 4096 | 0 | 4088 | 0.65 |
| `sqlite_stat1` | `table` | 24 | 4096 | 1047 | 2945 | 0.65 |
| `stage0_fts_config` | `table` | 1 | 4096 | 11 | 4074 | 0.65 |
| `stage0_fts_data` | `table` | 8 | 4096 | 3685 | 335 | 0.65 |
| `stage0_fts_docsize` | `table` | 17 | 4096 | 153 | 3867 | 0.65 |
| `stage0_fts_idx` | `table` | 6 | 4096 | 35 | 4035 | 0.65 |

## Table Row Averages

| Table | Rows | Bytes | Payload | Avg bytes/row | Avg payload/row |
| --- | ---: | ---: | ---: | ---: | ---: |
| `edges` | 833 | 69632 | 59730 | 83.59 | 71.70 |
| `qualified_name_dict` | 408 | 40960 | 32380 | 100.39 | 79.36 |
| `sqlite_stat4` | 288 | 40960 | 25752 | 142.22 | 89.42 |
| `entities` | 429 | 32768 | 26052 | 76.38 | 60.73 |
| `object_id_dict` | 487 | 28672 | 21312 | 58.87 | 43.76 |
| `qname_prefix_dict` | 165 | 20480 | 11891 | 124.12 | 72.07 |
| `symbol_dict` | 430 | 20480 | 10487 | 47.63 | 24.39 |
| `files` | 28 | 12288 | 7733 | 438.86 | 276.18 |
| `stage0_fts_content` | 17 | 12288 | 5226 | 722.82 | 307.41 |
| `bench_runs` | 0 | 4096 | 0 | 0.00 | 0.00 |
| `bench_tasks` | 0 | 4096 | 0 | 0.00 | 0.00 |
| `derived_edges` | 0 | 4096 | 0 | 0.00 | 0.00 |
| `edge_class_dict` | 7 | 4096 | 83 | 585.14 | 11.86 |
| `edge_context_dict` | 4 | 4096 | 37 | 1024.00 | 9.25 |
| `entity_kind_dict` | 22 | 4096 | 234 | 186.18 | 10.64 |
| `exactness_dict` | 2 | 4096 | 37 | 2048.00 | 18.50 |
| `extractor_dict` | 7 | 4096 | 235 | 585.14 | 33.57 |
| `language_dict` | 1 | 4096 | 13 | 4096.00 | 13.00 |
| `path_dict` | 28 | 4096 | 1569 | 146.29 | 56.04 |
| `path_evidence` | 0 | 4096 | 0 | 0.00 | 0.00 |
| `relation_kind_dict` | 27 | 4096 | 290 | 151.70 | 10.74 |
| `repo_index_state` | 1 | 4096 | 201 | 4096.00 | 201.00 |
| `retrieval_traces` | 0 | 4096 | 0 | 0.00 | 0.00 |
| `source_spans` | 0 | 4096 | 0 | 0.00 | 0.00 |
| `sqlite_stat1` | 24 | 4096 | 1047 | 170.67 | 43.63 |
| `stage0_fts_config` | 1 | 4096 | 11 | 4096.00 | 11.00 |
| `stage0_fts_data` | 8 | 4096 | 3685 | 512.00 | 460.63 |
| `stage0_fts_docsize` | 17 | 4096 | 153 | 240.94 | 9.00 |
| `stage0_fts_idx` | 6 | 4096 | 35 | 682.67 | 5.83 |

## Dictionary Metrics

| Dictionary | Rows | Value bytes | Unique index bytes |
| --- | ---: | ---: | ---: |
| `qualified_name_dict` | 408 | 28898 | 36864 |
| `object_id_dict` | 487 | 19851 | 28672 |
| `qname_prefix_dict` | 165 | 11307 | 20480 |
| `symbol_dict` | 430 | 9161 | 20480 |
| `path_dict` | 28 | 1481 | 4096 |

## FTS And Snippet Storage

- FTS total bytes: `28672`
- FTS rows: `17`
- FTS payload bytes: `4951`
- Stores source snippets: `false`

| Kind | Rows |
| --- | ---: |
| `entity` | 17 |

## Edge Fact Mix

- Total edges: `833`
- Derived edges: `0`
- Heuristic/unknown edge labels observed: `279`

### Exactness Counts

| Exactness | Edges |
| --- | ---: |
| `parser_verified` | 765 |
| `static_heuristic` | 68 |

### Edge Class Counts

| Edge class | Edges |
| --- | ---: |
| `base_exact` | 480 |
| `base_heuristic` | 35 |
| `mock` | 3 |
| `reified_callsite` | 74 |
| `test` | 65 |
| `unknown` | 176 |

### Edge Context Counts

| Context | Edges |
| --- | ---: |
| `mock` | 3 |
| `production` | 764 |
| `test` | 65 |
| `unknown` | 1 |

## Qualified Name Redundancy

- Stores full qualified-name text: `true`
- Rows: `408`
- Full value bytes: `28898`
- Prefix value bytes: `22554`
- Suffix value bytes: `5986`
- Unique index bytes: `36864`

## Index Usage Report

| Index | Table | Columns | Bytes | Unique | Origin | Used by core plans | Default workflow usage |
| --- | --- | --- | ---: | --- | --- | --- | --- |
| `sqlite_autoindex_qualified_name_dict_1` | `qualified_name_dict` | `value` | 36864 | `true` | `u` | none | `qualified-name dictionary lookup for exact symbol resolution`, `also backs joins that expose qualified names in query output` |
| `sqlite_autoindex_object_id_dict_1` | `object_id_dict` | `value` | 28672 | `true` | `u` | none | `compact object id dictionary value lookup during writes and id resolution`, `entity/edge joins usually use the INTEGER primary key after lookup` |
| `idx_edges_head_relation` | `edges` | `head_id_key`, `relation_id` | 20480 | `false` | `c` | `context_pack_outbound` | `context-pack outbound proof expansion`, `impact/callees/path traversal from a seed entity` |
| `idx_edges_tail_relation` | `edges` | `tail_id_key`, `relation_id` | 20480 | `false` | `c` | `impact_inbound` | `impact/callers/test-impact reverse traversal`, `reverse proof expansion by target entity` |
| `sqlite_autoindex_qname_prefix_dict_1` | `qname_prefix_dict` | `value` | 20480 | `true` | `u` | none | `qualified-name prefix interning during indexing`, `not directly used by default read workflows` |
| `sqlite_autoindex_symbol_dict_1` | `symbol_dict` | `value` | 20480 | `true` | `u` | none | `symbol dictionary lookup for exact symbol resolution and indexed writes` |
| `idx_edges_span_path` | `edges` | `span_path_id` | 16384 | `false` | `c` | none | `edge lookup by source-span file for audit/UI/source-span workflows`, `not observed in the main symbol/text/context/impact query plans unless path-scoped edge lookup is requested` |
| `idx_entities_name` | `entities` | `name_id` | 4096 | `false` | `c` | `symbol_query_exact_name` | `query symbols exact-name lookup`, `definitions/callers/callees/context-pack/impact seed resolution` |
| `idx_entities_path` | `entities` | `path_id` | 4096 | `false` | `c` | none | `list_entities_by_file during symbol FTS fallback and file-scoped expansion`, `stale cleanup and file lifecycle maintenance by path` |
| `idx_entities_qname` | `entities` | `qualified_name_id` | 4096 | `false` | `c` | none | `query symbols exact qualified-name lookup`, `definitions/callers/callees/context-pack/impact seed resolution` |
| `idx_retrieval_traces_created` | `retrieval_traces` | `created_at_unix_ms` | 4096 | `false` | `c` | none | `trace/history recency lookup`, `not part of default symbol/text/context/impact graph queries` |
| `idx_source_spans_path` | `source_spans` | `path_id` | 4096 | `false` | `c` | none | `source-span lookup and cleanup by file path`, `not used for most proof edges because edge spans are inline in the compact edge table` |
| `sqlite_autoindex_bench_runs_1` | `bench_runs` | `id` | 4096 | `true` | `pk` | none | `benchmark artifact lookup by id` |
| `sqlite_autoindex_bench_tasks_1` | `bench_tasks` | `id` | 4096 | `true` | `pk` | none | `benchmark artifact lookup by id` |
| `sqlite_autoindex_derived_edges_1` | `derived_edges` | `id` | 4096 | `true` | `pk` | none | `stored derived-edge lookup by id when persisted` |
| `sqlite_autoindex_edge_class_dict_1` | `edge_class_dict` | `value` | 4096 | `true` | `u` | none | `edge class lookup for context/audit/proof labeling` |
| `sqlite_autoindex_edge_context_dict_1` | `edge_context_dict` | `value` | 4096 | `true` | `u` | none | `edge context lookup for production/test/mock labeling` |
| `sqlite_autoindex_entity_kind_dict_1` | `entity_kind_dict` | `value` | 4096 | `true` | `u` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_exactness_dict_1` | `exactness_dict` | `value` | 4096 | `true` | `u` | none | `exactness lookup for unresolved-calls and proof/heuristic filtering` |
| `sqlite_autoindex_extractor_dict_1` | `extractor_dict` | `value` | 4096 | `true` | `u` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_language_dict_1` | `language_dict` | `value` | 4096 | `true` | `u` | none | `file language lookup for status and manifest reporting` |
| `sqlite_autoindex_path_dict_1` | `path_dict` | `value` | 4096 | `true` | `u` | none | `path dictionary lookup during indexing, file cleanup, and source-span resolution` |
| `sqlite_autoindex_path_evidence_1` | `path_evidence` | `id` | 4096 | `true` | `pk` | none | `stored PathEvidence lookup by id when persisted` |
| `sqlite_autoindex_relation_kind_dict_1` | `relation_kind_dict` | `value` | 4096 | `true` | `u` | `relation_query_calls`, `unresolved_calls_paginated` | `relation name lookup for relation filters such as CALLS and IMPORTS` |
| `sqlite_autoindex_repo_index_state_1` | `repo_index_state` | `repo_id` | 4096 | `true` | `pk` | none | `repo status lookup by repo id` |
| `sqlite_autoindex_retrieval_traces_1` | `retrieval_traces` | `id` | 4096 | `true` | `pk` | none | `trace lookup by id` |
| `sqlite_autoindex_edges_1` | `edges` | `id_key` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_entities_1` | `entities` | `id_key` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_files_1` | `files` | `path_id` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_source_spans_1` | `source_spans` | `id_key` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_stage0_fts_config_1` | `stage0_fts_config` | `k` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_stage0_fts_idx_1` | `stage0_fts_idx` | `segid`, `term` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |

## Core Query Plans

### `symbol_query_exact_name`

Default workflow: query symbols / definitions / seed resolution

Status: `ok`

Indexes: `idx_entities_name`

Full scans: `SCAN entities`

| ID | Parent | Detail |
| ---: | ---: | --- |
| 4 | 0 | `SEARCH e USING COVERING INDEX idx_entities_name (name_id=?)` |
| 7 | 0 | `SCALAR SUBQUERY 1` |
| 16 | 7 | `SCAN entities` |

### `text_query_fts`

Default workflow: query text / query files / symbol FTS fallback

Status: `ok`

Indexes: none

Full scans: `SCAN stage0_fts VIRTUAL TABLE INDEX 0:M6`

| ID | Parent | Detail |
| ---: | ---: | --- |
| 4 | 0 | `SCAN stage0_fts VIRTUAL TABLE INDEX 0:M6` |
| 26 | 0 | `USE TEMP B-TREE FOR ORDER BY` |

### `relation_query_calls`

Default workflow: query relation/path samples by relation

Status: `ok`

Indexes: `sqlite_autoindex_relation_kind_dict_1`

Full scans: `SCAN e`

| ID | Parent | Detail |
| ---: | ---: | --- |
| 4 | 0 | `SCAN e` |
| 9 | 0 | `SCALAR SUBQUERY 1` |
| 13 | 9 | `SEARCH relation_kind_dict USING COVERING INDEX sqlite_autoindex_relation_kind_dict_1 (value=?)` |

### `context_pack_outbound`

Default workflow: context-pack proof path expansion from a seed

Status: `ok`

Indexes: `idx_edges_head_relation`

Full scans: `SCAN edges`, `SCAN edges`

| ID | Parent | Detail |
| ---: | ---: | --- |
| 4 | 0 | `SEARCH e USING COVERING INDEX idx_edges_head_relation (head_id_key=? AND relation_id=?)` |
| 7 | 0 | `SCALAR SUBQUERY 1` |
| 16 | 7 | `SCAN edges` |
| 26 | 0 | `SCALAR SUBQUERY 2` |
| 35 | 26 | `SCAN edges` |

### `impact_inbound`

Default workflow: impact/callers/test-impact reverse traversal

Status: `ok`

Indexes: `idx_edges_tail_relation`

Full scans: `SCAN edges`, `SCAN edges`

| ID | Parent | Detail |
| ---: | ---: | --- |
| 4 | 0 | `SEARCH e USING COVERING INDEX idx_edges_tail_relation (tail_id_key=? AND relation_id=?)` |
| 7 | 0 | `SCALAR SUBQUERY 1` |
| 16 | 7 | `SCAN edges` |
| 26 | 0 | `SCALAR SUBQUERY 2` |
| 35 | 26 | `SCAN edges` |

### `unresolved_calls_paginated`

Default workflow: query unresolved-calls paginated

Status: `ok`

Indexes: `sqlite_autoindex_relation_kind_dict_1`, `id_key=?`, `rowid=?`

Full scans: `SCAN e`

| ID | Parent | Detail |
| ---: | ---: | --- |
| 12 | 0 | `SCAN e` |
| 17 | 0 | `SCALAR SUBQUERY 1` |
| 21 | 17 | `SEARCH relation_kind_dict USING COVERING INDEX sqlite_autoindex_relation_kind_dict_1 (value=?)` |
| 29 | 0 | `SEARCH tail USING PRIMARY KEY (id_key=?)` |
| 34 | 0 | `SEARCH tail_name USING INTEGER PRIMARY KEY (rowid=?)` |
| 37 | 0 | `SEARCH exactness USING INTEGER PRIMARY KEY (rowid=?)` |
| 40 | 0 | `SEARCH tail_qname USING INTEGER PRIMARY KEY (rowid=?)` |
| 43 | 0 | `SEARCH tail_extractor USING INTEGER PRIMARY KEY (rowid=?)` |


## Notes

- Read-only audit: no VACUUM, ANALYZE, index drop, or storage rewrite was applied.
- dbstat byte totals include SQLite b-tree pages and FTS shadow objects when available.
