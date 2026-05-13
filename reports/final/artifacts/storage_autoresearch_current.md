# Storage Inspection

Database: `reports/audit/artifacts/17_autoresearch_cold_only.sqlite`

- DBSTAT available: `true`
- Database bytes: `480669696`
- WAL bytes: `0`
- SHM bytes: `32768`
- File family bytes: `480702464`
- Page size: `4096`
- Page count: `117351`
- Freelist count: `0`

## Integrity Check

- Status: `failed`
- Checked: `true`
- Max errors captured: `20`

- `*** in database main ***
Tree 8 page 8 cell 5: Rowid 88344 out of order
Tree 8 page 8 cell 4: Rowid 73650 out of order
Tree 9 page 98456 cell 11: 2nd reference to page 102627
Tree 9 page 98456 cell 30: 2nd reference to page 102314
Tree 9 page 98456 cell 29: 2nd reference to page 102229
Tree 9 page 98456 cell 28: 2nd reference to page 102198
Tree 9 page 98456 cell 27: 2nd reference to page 102131
Tree 9 page 98456 cell 26: 2nd reference to page 102173
Tree 9 page 98456 cell 25: 2nd reference to page 102112
Tree 9 page 98456 cell 24: 2nd reference to page 102037
Tree 9 page 98456 cell 23: 2nd reference to page 102026
Tree 9 page 98456 cell 22: 2nd reference to page 101883
Tree 9 page 98456 cell 21: 2nd reference to page 102058
Tree 9 page 98456 cell 20: 2nd reference to page 101956
Tree 9 page 98456 cell 19: 2nd reference to page 101927
Tree 9 page 98456 cell 18: 2nd reference to page 101912
Tree 9 page 98456 cell 17: 2nd reference to page 101891
Tree 9 page 98456 cell 16: 2nd reference to page 101853
Tree 9 page 98456 cell 15: 2nd reference to page 101776
Tree 9 page 98456 cell 14: 2nd reference to page 101759`

## Aggregate Metrics

- Tables: `27`
- Indexes: `26`
- Observed table rows: `2923024`
- Edge count: `1044611`
- Average database bytes per edge: `460.17`
- Average edge table bytes per edge: `147.94`
- Average edge table plus edge-index bytes per edge: `201.90`
- Source-span rows: `0`
- Average source-span table bytes per row: `0.00`

## Category Breakdown

| Category | Bytes |
| --- | ---: |
| Dictionary tables | 105664512 |
| Dictionary unique indexes | 116621312 |
| Edge indexes | 56360960 |
| Source-span table/index | 8192 |
| FTS/shadow tables | 73728 |
| Snippet-like objects | 0 |

## Table and Index Sizes

| Object | Type | Rows | Bytes | Payload | Unused | DB % |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `edges` | `table` | 1044611 | 154542080 | 130357144 | 20334654 | 32.15 |
| `sqlite_autoindex_qualified_name_dict_1` | `index` | unknown | 61640704 | 51920049 | 7942697 | 12.82 |
| `qualified_name_dict` | `table` | 464610 | 57090048 | 53153723 | 1039494 | 11.88 |
| `entities` | `table` | 437263 | 31379456 | 26287180 | 3688461 | 6.53 |
| `sqlite_autoindex_object_id_dict_1` | `index` | unknown | 24690688 | 20529229 | 2776606 | 5.14 |
| `object_id_dict` | `table` | 437978 | 22188032 | 19270252 | 214615 | 4.62 |
| `idx_edges_tail_relation` | `index` | unknown | 19853312 | 16620973 | 40346 | 4.13 |
| `idx_edges_head_relation` | `index` | unknown | 19841024 | 16620805 | 28262 | 4.13 |
| `sqlite_autoindex_symbol_dict_1` | `index` | unknown | 16699392 | 13384455 | 1950375 | 3.47 |
| `idx_edges_span_path` | `index` | unknown | 16666624 | 13445420 | 38547 | 3.47 |
| `symbol_dict` | `table` | 438672 | 14839808 | 12080341 | 82503 | 3.09 |
| `sqlite_autoindex_qname_prefix_dict_1` | `index` | unknown | 13451264 | 11480308 | 1563471 | 2.80 |
| `qname_prefix_dict` | `table` | 96482 | 11415552 | 10582381 | 189182 | 2.37 |
| `idx_entities_qname` | `index` | unknown | 5201920 | 3859024 | 15871 | 1.08 |
| `idx_entities_name` | `index` | unknown | 5136384 | 3793850 | 15701 | 1.07 |
| `idx_entities_path` | `index` | unknown | 4747264 | 3404942 | 16629 | 0.99 |
| `files` | `table` | 1540 | 487424 | 429645 | 50195 | 0.10 |
| `sqlite_autoindex_path_dict_1` | `index` | unknown | 139264 | 118824 | 15425 | 0.03 |
| `path_dict` | `table` | 1561 | 131072 | 117432 | 5464 | 0.03 |
| `stage0_fts_content` | `table` | 98 | 36864 | 27559 | 8690 | 0.01 |
| `stage0_fts_data` | `table` | 12 | 24576 | 10209 | 14159 | 0.01 |
| `sqlite_schema` | `schema` | unknown | 12288 | 7635 | 4278 | 0.00 |
| `bench_runs` | `table` | 0 | 4096 | 0 | 4088 | 0.00 |
| `bench_tasks` | `table` | 0 | 4096 | 0 | 4088 | 0.00 |
| `derived_edges` | `table` | 0 | 4096 | 0 | 4088 | 0.00 |
| `edge_class_dict` | `table` | 7 | 4096 | 83 | 3977 | 0.00 |
| `edge_context_dict` | `table` | 4 | 4096 | 37 | 4035 | 0.00 |
| `entity_kind_dict` | `table` | 26 | 4096 | 256 | 3728 | 0.00 |
| `exactness_dict` | `table` | 2 | 4096 | 37 | 4043 | 0.00 |
| `extractor_dict` | `table` | 6 | 4096 | 199 | 3865 | 0.00 |
| `idx_retrieval_traces_created` | `index` | unknown | 4096 | 0 | 4088 | 0.00 |
| `idx_source_spans_path` | `index` | unknown | 4096 | 0 | 4088 | 0.00 |
| `language_dict` | `table` | 6 | 4096 | 52 | 4012 | 0.00 |
| `path_evidence` | `table` | 0 | 4096 | 0 | 4088 | 0.00 |
| `relation_kind_dict` | `table` | 37 | 4096 | 432 | 3508 | 0.00 |
| `repo_index_state` | `table` | 0 | 4096 | 0 | 4088 | 0.00 |
| `retrieval_traces` | `table` | 0 | 4096 | 0 | 4088 | 0.00 |
| `source_spans` | `table` | 0 | 4096 | 0 | 4088 | 0.00 |
| `sqlite_autoindex_bench_runs_1` | `index` | unknown | 4096 | 0 | 4088 | 0.00 |
| `sqlite_autoindex_bench_tasks_1` | `index` | unknown | 4096 | 0 | 4088 | 0.00 |
| `sqlite_autoindex_derived_edges_1` | `index` | unknown | 4096 | 0 | 4088 | 0.00 |
| `sqlite_autoindex_edge_class_dict_1` | `index` | unknown | 4096 | 89 | 3978 | 0.00 |
| `sqlite_autoindex_edge_context_dict_1` | `index` | unknown | 4096 | 40 | 4036 | 0.00 |
| `sqlite_autoindex_entity_kind_dict_1` | `index` | unknown | 4096 | 281 | 3729 | 0.00 |
| `sqlite_autoindex_exactness_dict_1` | `index` | unknown | 4096 | 38 | 4044 | 0.00 |
| `sqlite_autoindex_extractor_dict_1` | `index` | unknown | 4096 | 204 | 3866 | 0.00 |
| `sqlite_autoindex_language_dict_1` | `index` | unknown | 4096 | 57 | 4013 | 0.00 |
| `sqlite_autoindex_path_evidence_1` | `index` | unknown | 4096 | 0 | 4088 | 0.00 |
| `sqlite_autoindex_relation_kind_dict_1` | `index` | unknown | 4096 | 468 | 3509 | 0.00 |
| `sqlite_autoindex_repo_index_state_1` | `index` | unknown | 4096 | 0 | 4088 | 0.00 |
| `sqlite_autoindex_retrieval_traces_1` | `index` | unknown | 4096 | 0 | 4088 | 0.00 |
| `stage0_fts_config` | `table` | 1 | 4096 | 11 | 4074 | 0.00 |
| `stage0_fts_docsize` | `table` | 98 | 4096 | 882 | 2814 | 0.00 |
| `stage0_fts_idx` | `table` | 10 | 4096 | 64 | 3994 | 0.00 |

## Table Row Averages

| Table | Rows | Bytes | Payload | Avg bytes/row | Avg payload/row |
| --- | ---: | ---: | ---: | ---: | ---: |
| `edges` | 1044611 | 154542080 | 130357144 | 147.94 | 124.79 |
| `qualified_name_dict` | 464610 | 57090048 | 53153723 | 122.88 | 114.41 |
| `entities` | 437263 | 31379456 | 26287180 | 71.76 | 60.12 |
| `object_id_dict` | 437978 | 22188032 | 19270252 | 50.66 | 44.00 |
| `symbol_dict` | 438672 | 14839808 | 12080341 | 33.83 | 27.54 |
| `qname_prefix_dict` | 96482 | 11415552 | 10582381 | 118.32 | 109.68 |
| `files` | 1540 | 487424 | 429645 | 316.51 | 278.99 |
| `path_dict` | 1561 | 131072 | 117432 | 83.97 | 75.23 |
| `stage0_fts_content` | 98 | 36864 | 27559 | 376.16 | 281.21 |
| `stage0_fts_data` | 12 | 24576 | 10209 | 2048.00 | 850.75 |
| `bench_runs` | 0 | 4096 | 0 | 0.00 | 0.00 |
| `bench_tasks` | 0 | 4096 | 0 | 0.00 | 0.00 |
| `derived_edges` | 0 | 4096 | 0 | 0.00 | 0.00 |
| `edge_class_dict` | 7 | 4096 | 83 | 585.14 | 11.86 |
| `edge_context_dict` | 4 | 4096 | 37 | 1024.00 | 9.25 |
| `entity_kind_dict` | 26 | 4096 | 256 | 157.54 | 9.85 |
| `exactness_dict` | 2 | 4096 | 37 | 2048.00 | 18.50 |
| `extractor_dict` | 6 | 4096 | 199 | 682.67 | 33.17 |
| `language_dict` | 6 | 4096 | 52 | 682.67 | 8.67 |
| `path_evidence` | 0 | 4096 | 0 | 0.00 | 0.00 |
| `relation_kind_dict` | 37 | 4096 | 432 | 110.70 | 11.68 |
| `repo_index_state` | 0 | 4096 | 0 | 0.00 | 0.00 |
| `retrieval_traces` | 0 | 4096 | 0 | 0.00 | 0.00 |
| `source_spans` | 0 | 4096 | 0 | 0.00 | 0.00 |
| `stage0_fts_config` | 1 | 4096 | 11 | 4096.00 | 11.00 |
| `stage0_fts_docsize` | 98 | 4096 | 882 | 41.80 | 9.00 |
| `stage0_fts_idx` | 10 | 4096 | 64 | 409.60 | 6.40 |

## Dictionary Metrics

| Dictionary | Rows | Value bytes | Unique index bytes |
| --- | ---: | ---: | ---: |
| `qualified_name_dict` | 464610 | 48547386 | 61640704 |
| `object_id_dict` | 437978 | 17956318 | 24690688 |
| `symbol_dict` | 438672 | 10737300 | 16699392 |
| `qname_prefix_dict` | 96482 | 10213061 | 13451264 |
| `path_dict` | 1561 | 111624 | 139264 |

## FTS And Snippet Storage

- FTS total bytes: `73728`
- FTS rows: `98`
- FTS payload bytes: `25939`
- Stores source snippets: `false`

| Kind | Rows |
| --- | ---: |
| `entity` | 98 |

## Edge Fact Mix

- Total edges: `1044611`
- Derived edges: `0`
- Heuristic/unknown edge labels observed: `535643`

### Exactness Counts

| Exactness | Edges |
| --- | ---: |
| `parser_verified` | 775354 |
| `static_heuristic` | 269257 |

### Edge Class Counts

| Edge class | Edges |
| --- | ---: |
| `base_exact` | 289191 |
| `base_heuristic` | 95026 |
| `reified_callsite` | 181930 |
| `test` | 307104 |
| `unknown` | 171360 |

### Edge Context Counts

| Context | Edges |
| --- | ---: |
| `production` | 737506 |
| `test` | 307104 |
| `unknown` | 1 |

## Qualified Name Redundancy

- Stores full qualified-name text: `true`
- Rows: `408753`
- Full value bytes: `48519529`
- Prefix value bytes: `35375124`
- Suffix value bytes: `8200948`
- Unique index bytes: `61640704`

## Index Usage Report

| Index | Table | Columns | Bytes | Unique | Origin | Used by core plans | Default workflow usage |
| --- | --- | --- | ---: | --- | --- | --- | --- |
| `sqlite_autoindex_qualified_name_dict_1` | `qualified_name_dict` | `value` | 61640704 | `true` | `u` | none | `qualified-name dictionary lookup for exact symbol resolution`, `also backs joins that expose qualified names in query output` |
| `sqlite_autoindex_object_id_dict_1` | `object_id_dict` | `value` | 24690688 | `true` | `u` | none | `compact object id dictionary value lookup during writes and id resolution`, `entity/edge joins usually use the INTEGER primary key after lookup` |
| `idx_edges_tail_relation` | `edges` | `tail_id_key`, `relation_id` | 19853312 | `false` | `c` | `impact_inbound` | `impact/callers/test-impact reverse traversal`, `reverse proof expansion by target entity` |
| `idx_edges_head_relation` | `edges` | `head_id_key`, `relation_id` | 19841024 | `false` | `c` | `context_pack_outbound` | `context-pack outbound proof expansion`, `impact/callees/path traversal from a seed entity` |
| `sqlite_autoindex_symbol_dict_1` | `symbol_dict` | `value` | 16699392 | `true` | `u` | none | `symbol dictionary lookup for exact symbol resolution and indexed writes` |
| `idx_edges_span_path` | `edges` | `span_path_id` | 16666624 | `false` | `c` | none | `edge lookup by source-span file for audit/UI/source-span workflows`, `not observed in the main symbol/text/context/impact query plans unless path-scoped edge lookup is requested` |
| `sqlite_autoindex_qname_prefix_dict_1` | `qname_prefix_dict` | `value` | 13451264 | `true` | `u` | none | `qualified-name prefix interning during indexing`, `not directly used by default read workflows` |
| `idx_entities_qname` | `entities` | `qualified_name_id` | 5201920 | `false` | `c` | none | `query symbols exact qualified-name lookup`, `definitions/callers/callees/context-pack/impact seed resolution` |
| `idx_entities_name` | `entities` | `name_id` | 5136384 | `false` | `c` | `symbol_query_exact_name` | `query symbols exact-name lookup`, `definitions/callers/callees/context-pack/impact seed resolution` |
| `idx_entities_path` | `entities` | `path_id` | 4747264 | `false` | `c` | none | `list_entities_by_file during symbol FTS fallback and file-scoped expansion`, `stale cleanup and file lifecycle maintenance by path` |
| `sqlite_autoindex_path_dict_1` | `path_dict` | `value` | 139264 | `true` | `u` | none | `path dictionary lookup during indexing, file cleanup, and source-span resolution` |
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

Indexes: `sqlite_autoindex_relation_kind_dict_1`, `rowid=?`, `id_key=?`

Full scans: `SCAN e`

| ID | Parent | Detail |
| ---: | ---: | --- |
| 12 | 0 | `SCAN e` |
| 17 | 0 | `SCALAR SUBQUERY 1` |
| 21 | 17 | `SEARCH relation_kind_dict USING COVERING INDEX sqlite_autoindex_relation_kind_dict_1 (value=?)` |
| 29 | 0 | `SEARCH exactness USING INTEGER PRIMARY KEY (rowid=?)` |
| 32 | 0 | `SEARCH tail USING PRIMARY KEY (id_key=?)` |
| 37 | 0 | `SEARCH tail_name USING INTEGER PRIMARY KEY (rowid=?)` |
| 40 | 0 | `SEARCH tail_qname USING INTEGER PRIMARY KEY (rowid=?)` |
| 43 | 0 | `SEARCH tail_extractor USING INTEGER PRIMARY KEY (rowid=?)` |


## Notes

- Read-only audit: no VACUUM, ANALYZE, index drop, or storage rewrite was applied.
- dbstat byte totals include SQLite b-tree pages and FTS shadow objects when available.
