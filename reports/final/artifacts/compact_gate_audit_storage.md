# Storage Inspection

Database: `reports/final/artifacts/compact_gate_autoresearch_audit.sqlite`

- DBSTAT available: `true`
- Database bytes: `1063632896`
- WAL bytes: `0`
- SHM bytes: `0`
- File family bytes: `1063632896`
- Page size: `4096`
- Page count: `259676`
- Freelist count: `0`

## Integrity Check

- Status: `ok`
- Checked: `true`
- Max errors captured: `20`

- `ok`

## Aggregate Metrics

- Tables: `55`
- Indexes: `54`
- Observed table rows: `3963779`
- Proof edge rows: `31848`
- Structural relation rows: `0`
- Callsite rows: `2177`
- Callsite argument rows: `27980`
- Semantic edge/fact rows: `62005`
- Average database bytes per proof edge: `33397.16`
- Average database bytes per semantic edge/fact: `17153.99`
- Average edge table bytes per proof edge: `66.11`
- Average edge table plus edge-index bytes per proof edge: `119.22`
- Source-span rows: `0`
- Average source-span table bytes per row: `0.00`

## Category Breakdown

| Category | Bytes |
| --- | ---: |
| Dictionary tables | 65241088 |
| Dictionary unique indexes | 442368 |
| Edge indexes | 1691648 |
| Source-span table/index | 8192 |
| FTS/shadow tables | 118784 |
| Snippet-like objects | 0 |

## Table and Index Sizes

| Object | Type | Rows | Bytes | Payload | Unused | DB % |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `unresolved_references` | `table` | 513752 | 280887296 | 226771807 | 51237573 | 26.41 |
| `heuristic_edges` | `table` | 513757 | 275083264 | 215468773 | 56753559 | 25.86 |
| `template_entities` | `table` | 653819 | 87523328 | 75801704 | 9502454 | 8.23 |
| `static_references` | `table` | 123251 | 66158592 | 52469903 | 13001865 | 6.22 |
| `template_edges` | `table` | 169290 | 40804352 | 34356421 | 5651231 | 3.84 |
| `idx_heuristic_edges_span_path` | `index` | unknown | 38621184 | 36606361 | 360408 | 3.63 |
| `idx_unresolved_references_path` | `index` | unknown | 38617088 | 36606119 | 356581 | 3.63 |
| `symbol_dict` | `table` | 696373 | 30392320 | 25916469 | 188213 | 2.86 |
| `qname_prefix_dict` | `table` | 155741 | 22274048 | 20771044 | 412051 | 2.09 |
| `idx_heuristic_edges_relation` | `index` | unknown | 18907136 | 17215454 | 95023 | 1.78 |
| `idx_template_edges_head_relation` | `index` | unknown | 16539648 | 15913260 | 70066 | 1.56 |
| `idx_template_edges_tail_relation` | `index` | unknown | 16539648 | 15913260 | 70066 | 1.56 |
| `idx_symbol_dict_hash` | `index` | unknown | 14602240 | 11109040 | 1361305 | 1.37 |
| `idx_qualified_name_parts` | `index` | unknown | 12742656 | 9161622 | 1361590 | 1.20 |
| `qualified_name_dict` | `table` | 727372 | 12173312 | 7739773 | 35402 | 1.14 |
| `file_entities` | `table` | 89407 | 10248192 | 8673290 | 1272838 | 0.96 |
| `idx_static_references_path` | `index` | unknown | 9166848 | 8683539 | 86704 | 0.86 |
| `idx_file_entities_entity` | `index` | unknown | 9097216 | 8673290 | 125234 | 0.86 |
| `path_evidence` | `table` | 4096 | 8519680 | 7115002 | 1347044 | 0.80 |
| `file_edges` | `table` | 62959 | 6914048 | 5883234 | 820531 | 0.65 |
| `entities` | `table` | 89407 | 6803456 | 5960003 | 555118 | 0.64 |
| `file_source_spans` | `table` | 62005 | 6795264 | 5808732 | 779459 | 0.64 |
| `idx_file_edges_edge` | `index` | unknown | 6168576 | 5883234 | 77243 | 0.58 |
| `idx_file_source_spans_span` | `index` | unknown | 6094848 | 5808732 | 81095 | 0.57 |
| `idx_qname_prefix_dict_hash` | `index` | unknown | 3309568 | 2515701 | 316952 | 0.31 |
| `edges` | `table` | 31848 | 2105344 | 1740018 | 262415 | 0.20 |
| `files` | `table` | 4975 | 1695744 | 1452837 | 218043 | 0.16 |
| `callsite_args` | `table` | 27980 | 1540096 | 1288777 | 162871 | 0.14 |
| `idx_entities_qname` | `index` | unknown | 1028096 | 752584 | 4283 | 0.10 |
| `idx_entities_name` | `index` | unknown | 1015808 | 740803 | 3812 | 0.10 |
| `path_evidence_edges` | `table` | 4096 | 946176 | 795314 | 131710 | 0.09 |
| `idx_entities_path` | `index` | unknown | 937984 | 663193 | 3826 | 0.09 |
| `path_evidence_symbols` | `table` | 8192 | 712704 | 606208 | 79836 | 0.07 |
| `idx_path_evidence_symbols_entity` | `index` | unknown | 638976 | 606208 | 6324 | 0.06 |
| `idx_path_evidence_symbols_path` | `index` | unknown | 638976 | 606208 | 6324 | 0.06 |
| `idx_edges_head_relation` | `index` | unknown | 593920 | 494858 | 1782 | 0.06 |
| `idx_edges_tail_relation` | `index` | unknown | 593920 | 494857 | 1783 | 0.06 |
| `path_evidence_lookup` | `table` | 4096 | 589824 | 551887 | 11200 | 0.06 |
| `idx_edges_span_path` | `index` | unknown | 503808 | 402465 | 4327 | 0.05 |
| `sqlite_autoindex_path_dict_1` | `index` | unknown | 442368 | 370188 | 55962 | 0.04 |
| `path_dict` | `table` | 4975 | 397312 | 360366 | 10660 | 0.04 |
| `idx_path_evidence_lookup_source` | `index` | unknown | 315392 | 289615 | 12569 | 0.03 |
| `idx_path_evidence_lookup_target` | `index` | unknown | 315392 | 289615 | 12569 | 0.03 |
| `file_path_evidence` | `table` | 4096 | 299008 | 247523 | 38285 | 0.03 |
| `path_evidence_files` | `table` | 4096 | 299008 | 247523 | 38285 | 0.03 |
| `idx_file_path_evidence_path` | `index` | unknown | 274432 | 247523 | 13781 | 0.03 |
| `idx_path_evidence_files_file` | `index` | unknown | 274432 | 247523 | 13781 | 0.03 |
| `idx_path_evidence_source` | `index` | unknown | 245760 | 229248 | 3508 | 0.02 |
| `idx_path_evidence_target` | `index` | unknown | 245760 | 229248 | 3508 | 0.02 |
| `source_content_template` | `table` | 2718 | 200704 | 173610 | 18356 | 0.02 |
| `sqlite_autoindex_source_content_template_1` | `index` | unknown | 196608 | 157480 | 30402 | 0.02 |
| `sqlite_stat4` | `table` | 1256 | 172032 | 154731 | 9901 | 0.02 |
| `sqlite_autoindex_path_evidence_1` | `index` | unknown | 143360 | 114560 | 16096 | 0.01 |
| `sqlite_autoindex_path_evidence_lookup_1` | `index` | unknown | 143360 | 114560 | 16096 | 0.01 |
| `sqlite_autoindex_edge_provenance_dict_1` | `index` | unknown | 131072 | 109352 | 17728 | 0.01 |
| `callsites` | `table` | 2177 | 126976 | 99947 | 20130 | 0.01 |
| `idx_path_evidence_edges_path_ordinal` | `index` | unknown | 126976 | 106496 | 7824 | 0.01 |
| `edge_provenance_dict` | `table` | 1204 | 118784 | 107072 | 5369 | 0.01 |
| `idx_path_evidence_lookup_signature` | `index` | unknown | 106496 | 88911 | 4989 | 0.01 |
| `idx_files_content_template` | `index` | unknown | 102400 | 79344 | 7835 | 0.01 |
| `sqlite_autoindex_files_1` | `index` | unknown | 61440 | 34569 | 11770 | 0.01 |
| `stage0_fts_content` | `table` | 186 | 61440 | 52348 | 7753 | 0.01 |
| `stage0_fts_data` | `table` | 17 | 45056 | 28725 | 15973 | 0.00 |
| `sqlite_schema` | `schema` | unknown | 32768 | 25688 | 6293 | 0.00 |
| `file_fts_rows` | `table` | 186 | 24576 | 17602 | 6348 | 0.00 |
| `idx_file_fts_rows_object` | `index` | unknown | 24576 | 17602 | 6348 | 0.00 |
| `bench_runs` | `table` | 0 | 4096 | 0 | 4088 | 0.00 |
| `bench_tasks` | `table` | 0 | 4096 | 0 | 4088 | 0.00 |
| `derived_edges` | `table` | 0 | 4096 | 0 | 4088 | 0.00 |
| `edge_class_dict` | `table` | 5 | 4096 | 59 | 4009 | 0.00 |
| `edge_context_dict` | `table` | 2 | 4096 | 20 | 4060 | 0.00 |
| `edge_debug_metadata` | `table` | 0 | 4096 | 0 | 4088 | 0.00 |
| `entity_id_history` | `table` | 100 | 4096 | 2200 | 1588 | 0.00 |
| `entity_kind_dict` | `table` | 31 | 4096 | 316 | 3648 | 0.00 |
| `exactness_dict` | `table` | 2 | 4096 | 48 | 4032 | 0.00 |
| `extraction_warnings` | `table` | 0 | 4096 | 0 | 4088 | 0.00 |
| `extractor_dict` | `table` | 6 | 4096 | 196 | 3868 | 0.00 |
| `file_graph_digests` | `table` | 0 | 4096 | 0 | 4088 | 0.00 |
| `idx_extraction_warnings_path` | `index` | unknown | 4096 | 0 | 4088 | 0.00 |
| `idx_object_id_dict_hash` | `index` | unknown | 4096 | 0 | 4088 | 0.00 |

## Table Row Averages

| Table | Rows | Bytes | Payload | Avg bytes/row | Avg payload/row |
| --- | ---: | ---: | ---: | ---: | ---: |
| `unresolved_references` | 513752 | 280887296 | 226771807 | 546.74 | 441.40 |
| `heuristic_edges` | 513757 | 275083264 | 215468773 | 535.43 | 419.40 |
| `template_entities` | 653819 | 87523328 | 75801704 | 133.86 | 115.94 |
| `static_references` | 123251 | 66158592 | 52469903 | 536.78 | 425.72 |
| `template_edges` | 169290 | 40804352 | 34356421 | 241.03 | 202.94 |
| `symbol_dict` | 696373 | 30392320 | 25916469 | 43.64 | 37.22 |
| `qname_prefix_dict` | 155741 | 22274048 | 20771044 | 143.02 | 133.37 |
| `qualified_name_dict` | 727372 | 12173312 | 7739773 | 16.74 | 10.64 |
| `file_entities` | 89407 | 10248192 | 8673290 | 114.62 | 97.01 |
| `path_evidence` | 4096 | 8519680 | 7115002 | 2080.00 | 1737.06 |
| `file_edges` | 62959 | 6914048 | 5883234 | 109.82 | 93.45 |
| `entities` | 89407 | 6803456 | 5960003 | 76.10 | 66.66 |
| `file_source_spans` | 62005 | 6795264 | 5808732 | 109.59 | 93.68 |
| `edges` | 31848 | 2105344 | 1740018 | 66.11 | 54.64 |
| `files` | 4975 | 1695744 | 1452837 | 340.85 | 292.03 |
| `callsite_args` | 27980 | 1540096 | 1288777 | 55.04 | 46.06 |
| `path_evidence_edges` | 4096 | 946176 | 795314 | 231.00 | 194.17 |
| `path_evidence_symbols` | 8192 | 712704 | 606208 | 87.00 | 74.00 |
| `path_evidence_lookup` | 4096 | 589824 | 551887 | 144.00 | 134.74 |
| `path_dict` | 4975 | 397312 | 360366 | 79.86 | 72.44 |
| `file_path_evidence` | 4096 | 299008 | 247523 | 73.00 | 60.43 |
| `path_evidence_files` | 4096 | 299008 | 247523 | 73.00 | 60.43 |
| `source_content_template` | 2718 | 200704 | 173610 | 73.84 | 63.87 |
| `sqlite_stat4` | 1256 | 172032 | 154731 | 136.97 | 123.19 |
| `callsites` | 2177 | 126976 | 99947 | 58.33 | 45.91 |
| `edge_provenance_dict` | 1204 | 118784 | 107072 | 98.66 | 88.93 |
| `stage0_fts_content` | 186 | 61440 | 52348 | 330.32 | 281.44 |
| `stage0_fts_data` | 17 | 45056 | 28725 | 2650.35 | 1689.71 |
| `file_fts_rows` | 186 | 24576 | 17602 | 132.13 | 94.63 |
| `bench_runs` | 0 | 4096 | 0 | 0.00 | 0.00 |
| `bench_tasks` | 0 | 4096 | 0 | 0.00 | 0.00 |
| `derived_edges` | 0 | 4096 | 0 | 0.00 | 0.00 |
| `edge_class_dict` | 5 | 4096 | 59 | 819.20 | 11.80 |
| `edge_context_dict` | 2 | 4096 | 20 | 2048.00 | 10.00 |
| `edge_debug_metadata` | 0 | 4096 | 0 | 0.00 | 0.00 |
| `entity_id_history` | 100 | 4096 | 2200 | 40.96 | 22.00 |
| `entity_kind_dict` | 31 | 4096 | 316 | 132.13 | 10.19 |
| `exactness_dict` | 2 | 4096 | 48 | 2048.00 | 24.00 |
| `extraction_warnings` | 0 | 4096 | 0 | 0.00 | 0.00 |
| `extractor_dict` | 6 | 4096 | 196 | 682.67 | 32.67 |
| `file_graph_digests` | 0 | 4096 | 0 | 0.00 | 0.00 |
| `language_dict` | 6 | 4096 | 52 | 682.67 | 8.67 |
| `object_id_dict` | 0 | 4096 | 0 | 0.00 | 0.00 |
| `path_evidence_tests` | 0 | 4096 | 0 | 0.00 | 0.00 |
| `relation_kind_dict` | 20 | 4096 | 224 | 204.80 | 11.20 |
| `repo_graph_digest` | 0 | 4096 | 0 | 0.00 | 0.00 |
| `repo_index_state` | 1 | 4096 | 156 | 4096.00 | 156.00 |
| `resolution_kind_dict` | 7 | 4096 | 164 | 585.14 | 23.43 |
| `retrieval_traces` | 0 | 4096 | 0 | 0.00 | 0.00 |
| `source_spans` | 0 | 4096 | 0 | 0.00 | 0.00 |
| `sqlite_stat1` | 70 | 4096 | 3483 | 58.51 | 49.76 |
| `stage0_fts_config` | 1 | 4096 | 11 | 4096.00 | 11.00 |
| `stage0_fts_docsize` | 186 | 4096 | 1674 | 22.02 | 9.00 |
| `stage0_fts_idx` | 15 | 4096 | 106 | 273.07 | 7.07 |
| `structural_relations` | 0 | 4096 | 0 | 0.00 | 0.00 |

## Dictionary Metrics

| Dictionary | Rows | Value bytes | Unique index bytes |
| --- | ---: | ---: | ---: |
| `qname_prefix_dict` | 155741 | 18380648 | 0 |
| `symbol_dict` | 696373 | 16137884 | 0 |
| `path_dict` | 4975 | 341970 | 442368 |
| `object_id_dict` | 0 | 0 | 0 |
| `qualified_name_dict` | 727372 | 0 | 0 |

## FTS And Snippet Storage

- FTS total bytes: `118784`
- FTS rows: `186`
- FTS payload bytes: `49264`
- Stores source snippets: `false`

| Kind | Rows |
| --- | ---: |
| `entity` | 186 |

## Edge Fact Mix

- Total edges: `31848`
- Derived edges: `1203`
- Heuristic/unknown edge labels observed: `180`

### Exactness Counts

| Exactness | Edges |
| --- | ---: |
| `derived_from_verified_edges` | 1203 |
| `parser_verified` | 30645 |

### Edge Class Counts

| Edge class | Edges |
| --- | ---: |
| `base_exact` | 27754 |
| `derived` | 1203 |
| `reified_callsite` | 447 |
| `test` | 2264 |
| `unknown` | 180 |

### Edge Context Counts

| Context | Edges |
| --- | ---: |
| `production` | 29584 |
| `test` | 2264 |

## Qualified Name Redundancy

- Stores full qualified-name text: `false`
- Rows: `727372`
- Full value bytes: `0`
- Prefix value bytes: `72860733`
- Suffix value bytes: `14906134`
- Unique index bytes: `0`

## Index Usage Report

| Index | Table | Columns | Bytes | Unique | Origin | Used by core plans | Default workflow usage |
| --- | --- | --- | ---: | --- | --- | --- | --- |
| `idx_heuristic_edges_span_path` | `heuristic_edges` | `source_span_path` | 38621184 | `false` | `c` | none | `no mapped default workflow observed; verify with query plans before keeping` |
| `idx_unresolved_references_path` | `unresolved_references` | `source_span_path` | 38617088 | `false` | `c` | none | `no mapped default workflow observed; verify with query plans before keeping` |
| `idx_heuristic_edges_relation` | `heuristic_edges` | `relation`, `exactness` | 18907136 | `false` | `c` | none | `no mapped default workflow observed; verify with query plans before keeping` |
| `idx_template_edges_head_relation` | `template_edges` | `content_template_id`, `local_head_entity_id`, `relation_id` | 16539648 | `false` | `c` | none | `no mapped default workflow observed; verify with query plans before keeping` |
| `idx_template_edges_tail_relation` | `template_edges` | `content_template_id`, `local_tail_entity_id`, `relation_id` | 16539648 | `false` | `c` | none | `no mapped default workflow observed; verify with query plans before keeping` |
| `idx_symbol_dict_hash` | `symbol_dict` | `value_hash`, `value_len` | 14602240 | `false` | `c` | none | `compact symbol dictionary lookup by stable hash/length with exact string verification`, `supports exact-name resolution and qualified-name suffix reconstruction` |
| `idx_qualified_name_parts` | `qualified_name_dict` | `prefix_id`, `suffix_id` | 12742656 | `true` | `c` | none | `qualified-name lookup by prefix_id/suffix_id tuple`, `replaces redundant full qualified-name text storage and UNIQUE text index` |
| `idx_static_references_path` | `static_references` | `repo_relative_path` | 9166848 | `false` | `c` | none | `no mapped default workflow observed; verify with query plans before keeping` |
| `idx_file_entities_entity` | `file_entities` | `entity_id`, `file_id` | 9097216 | `false` | `c` | none | `no mapped default workflow observed; verify with query plans before keeping` |
| `idx_file_edges_edge` | `file_edges` | `edge_id`, `file_id` | 6168576 | `false` | `c` | none | `no mapped default workflow observed; verify with query plans before keeping` |
| `idx_file_source_spans_span` | `file_source_spans` | `span_id`, `file_id` | 6094848 | `false` | `c` | none | `no mapped default workflow observed; verify with query plans before keeping` |
| `idx_qname_prefix_dict_hash` | `qname_prefix_dict` | `value_hash`, `value_len` | 3309568 | `false` | `c` | none | `compact qualified-name prefix lookup by stable hash/length with exact string verification`, `supports qualified-name interning without a full-prefix UNIQUE text index` |
| `idx_entities_qname` | `entities` | `qualified_name_id` | 1028096 | `false` | `c` | none | `query symbols exact qualified-name lookup`, `definitions/callers/callees/context-pack/impact seed resolution` |
| `idx_entities_name` | `entities` | `name_id` | 1015808 | `false` | `c` | `symbol_query_exact_name` | `query symbols exact-name lookup`, `definitions/callers/callees/context-pack/impact seed resolution` |
| `idx_entities_path` | `entities` | `path_id` | 937984 | `false` | `c` | none | `list_entities_by_file during symbol FTS fallback and file-scoped expansion`, `stale cleanup and file lifecycle maintenance by path` |
| `idx_path_evidence_symbols_entity` | `path_evidence_symbols` | `entity_id`, `path_id` | 638976 | `false` | `c` | none | `no mapped default workflow observed; verify with query plans before keeping` |
| `idx_path_evidence_symbols_path` | `path_evidence_symbols` | `path_id`, `entity_id` | 638976 | `false` | `c` | none | `no mapped default workflow observed; verify with query plans before keeping` |
| `idx_edges_head_relation` | `edges` | `head_id_key`, `relation_id` | 593920 | `false` | `c` | `context_pack_outbound` | `context-pack outbound proof expansion`, `impact/callees/path traversal from a seed entity` |
| `idx_edges_tail_relation` | `edges` | `tail_id_key`, `relation_id` | 593920 | `false` | `c` | `impact_inbound` | `impact/callers/test-impact reverse traversal`, `reverse proof expansion by target entity` |
| `idx_edges_span_path` | `edges` | `span_path_id` | 503808 | `false` | `c` | none | `edge lookup by source-span file for audit/UI/source-span workflows`, `not observed in the main symbol/text/context/impact query plans unless path-scoped edge lookup is requested` |
| `sqlite_autoindex_path_dict_1` | `path_dict` | `value` | 442368 | `true` | `u` | none | `path dictionary lookup during indexing, file cleanup, and source-span resolution` |
| `idx_path_evidence_lookup_source` | `path_evidence_lookup` | `source_id`, `task_class`, `relation_signature`, `confidence` | 315392 | `false` | `c` | none | `no mapped default workflow observed; verify with query plans before keeping` |
| `idx_path_evidence_lookup_target` | `path_evidence_lookup` | `target_id`, `task_class`, `relation_signature`, `confidence` | 315392 | `false` | `c` | none | `no mapped default workflow observed; verify with query plans before keeping` |
| `idx_file_path_evidence_path` | `file_path_evidence` | `path_id`, `file_id` | 274432 | `false` | `c` | none | `no mapped default workflow observed; verify with query plans before keeping` |
| `idx_path_evidence_files_file` | `path_evidence_files` | `file_id`, `path_id` | 274432 | `false` | `c` | none | `no mapped default workflow observed; verify with query plans before keeping` |
| `idx_path_evidence_source` | `path_evidence` | `source`, `length`, `confidence` | 245760 | `false` | `c` | none | `no mapped default workflow observed; verify with query plans before keeping` |
| `idx_path_evidence_target` | `path_evidence` | `target`, `length`, `confidence` | 245760 | `false` | `c` | none | `no mapped default workflow observed; verify with query plans before keeping` |
| `sqlite_autoindex_source_content_template_1` | `source_content_template` | `content_hash`, `language_id`, `extraction_version` | 196608 | `true` | `u` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_path_evidence_1` | `path_evidence` | `id` | 143360 | `true` | `pk` | none | `stored PathEvidence lookup by id when persisted` |
| `sqlite_autoindex_path_evidence_lookup_1` | `path_evidence_lookup` | `path_id` | 143360 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_edge_provenance_dict_1` | `edge_provenance_dict` | `value` | 131072 | `true` | `u` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `idx_path_evidence_edges_path_ordinal` | `path_evidence_edges` | `path_id`, `ordinal` | 126976 | `false` | `c` | none | `no mapped default workflow observed; verify with query plans before keeping` |
| `idx_path_evidence_lookup_signature` | `path_evidence_lookup` | `relation_signature`, `confidence` | 106496 | `false` | `c` | none | `no mapped default workflow observed; verify with query plans before keeping` |
| `idx_files_content_template` | `files` | `content_template_id`, `path_id` | 102400 | `false` | `c` | none | `no mapped default workflow observed; verify with query plans before keeping` |
| `sqlite_autoindex_files_1` | `files` | `path_id` | 61440 | `true` | `u` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `idx_file_fts_rows_object` | `file_fts_rows` | `object_id`, `kind`, `file_id` | 24576 | `false` | `c` | none | `no mapped default workflow observed; verify with query plans before keeping` |
| `idx_extraction_warnings_path` | `extraction_warnings` | `repo_relative_path` | 4096 | `false` | `c` | none | `no mapped default workflow observed; verify with query plans before keeping` |
| `idx_object_id_dict_hash` | `object_id_dict` | `value_hash`, `value_len` | 4096 | `false` | `c` | none | `compact object-id dictionary lookup by stable hash/length with exact string verification`, `replaces the former full-text UNIQUE autoindex on object_id_dict.value` |
| `idx_path_evidence_tests_test` | `path_evidence_tests` | `test_id`, `path_id` | 4096 | `false` | `c` | none | `no mapped default workflow observed; verify with query plans before keeping` |
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
| `sqlite_autoindex_relation_kind_dict_1` | `relation_kind_dict` | `value` | 4096 | `true` | `u` | `relation_query_calls`, `unresolved_calls_paginated` | `relation name lookup for relation filters such as CALLS and IMPORTS` |
| `sqlite_autoindex_repo_index_state_1` | `repo_index_state` | `repo_id` | 4096 | `true` | `pk` | none | `repo status lookup by repo id` |
| `sqlite_autoindex_resolution_kind_dict_1` | `resolution_kind_dict` | `value` | 4096 | `true` | `u` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_retrieval_traces_1` | `retrieval_traces` | `id` | 4096 | `true` | `pk` | none | `trace lookup by id` |
| `sqlite_autoindex_callsite_args_1` | `callsite_args` | `id_key` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_callsites_1` | `callsites` | `id_key` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_edge_debug_metadata_1` | `edge_debug_metadata` | `edge_id` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_edges_1` | `edges` | `id_key` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_entities_1` | `entities` | `id_key` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_entity_id_history_1` | `entity_id_history` | `entity_hash` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_extraction_warnings_1` | `extraction_warnings` | `id_key` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_file_edges_1` | `file_edges` | `file_id`, `edge_id` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_file_entities_1` | `file_entities` | `file_id`, `entity_id` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_file_fts_rows_1` | `file_fts_rows` | `file_id`, `rowid` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_file_graph_digests_1` | `file_graph_digests` | `file_id` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_file_path_evidence_1` | `file_path_evidence` | `file_id`, `path_id` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_file_source_spans_1` | `file_source_spans` | `file_id`, `span_id` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_files_2` | `files` | `file_id` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_heuristic_edges_1` | `heuristic_edges` | `id_key` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_path_evidence_edges_1` | `path_evidence_edges` | `path_id`, `ordinal` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_path_evidence_files_1` | `path_evidence_files` | `file_id`, `path_id` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_path_evidence_symbols_1` | `path_evidence_symbols` | `entity_id`, `path_id`, `role` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_path_evidence_tests_1` | `path_evidence_tests` | `path_id`, `test_id`, `relation` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_repo_graph_digest_1` | `repo_graph_digest` | `id` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_source_content_template_2` | `source_content_template` | `content_template_id` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_source_spans_1` | `source_spans` | `id_key` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_stage0_fts_config_1` | `stage0_fts_config` | `k` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_stage0_fts_idx_1` | `stage0_fts_idx` | `segid`, `term` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_static_references_1` | `static_references` | `id_key` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_structural_relations_1` | `structural_relations` | `id_key` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_template_edges_1` | `template_edges` | `content_template_id`, `local_template_edge_id` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_template_entities_1` | `template_entities` | `content_template_id`, `local_template_entity_id` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |
| `sqlite_autoindex_unresolved_references_1` | `unresolved_references` | `id_key` | 0 | `true` | `pk` | none | `automatic unique/primary-key constraint; usage requires case-by-case verification` |

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
| 14 | 0 | `SCAN e` |
| 19 | 0 | `SCALAR SUBQUERY 1` |
| 23 | 19 | `SEARCH relation_kind_dict USING COVERING INDEX sqlite_autoindex_relation_kind_dict_1 (value=?)` |
| 31 | 0 | `SEARCH tail USING PRIMARY KEY (id_key=?)` |
| 36 | 0 | `SEARCH qname USING INTEGER PRIMARY KEY (rowid=?)` |
| 39 | 0 | `SEARCH exactness USING INTEGER PRIMARY KEY (rowid=?)` |
| 42 | 0 | `SEARCH prefix USING INTEGER PRIMARY KEY (rowid=?)` |
| 45 | 0 | `SEARCH tail_name USING INTEGER PRIMARY KEY (rowid=?)` |
| 48 | 0 | `SEARCH tail_extractor USING INTEGER PRIMARY KEY (rowid=?)` |
| 51 | 0 | `SEARCH suffix USING INTEGER PRIMARY KEY (rowid=?)` |


## Notes

- Read-only audit: no VACUUM, ANALYZE, index drop, or storage rewrite was applied.
- dbstat byte totals include SQLite b-tree pages and FTS shadow objects when available.
