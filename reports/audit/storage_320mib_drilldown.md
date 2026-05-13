# Storage 320 MiB Drilldown

Generated: 2026-05-12 12:43:00 -05:00

Source of truth: `MVP.md`. No production behavior changed in this phase.

## Executive Summary

- Proof DB: `336,203,776` bytes / `320.63 MiB`.
- Target: `250.00 MiB`.
- Minimum required reduction: `70.63 MiB`.
- Current size is explained by template overlay tables plus dictionaries and their indexes, not by the proof edge table itself.
- Repeated `content_hash` is already effectively solved in this artifact; the remaining big payloads are local template IDs, template metadata JSON, symbol/prefix strings, and template/dictionary indexes.

## Focus Contributor Drilldown

| Object | Class | Rows | Table MiB | Avg B/row | Idx count | Idx MiB | Proof req | Derivable | Sidecar |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `template_entities` | template | 653,819 | 83.469 | 133.86 | 1 | 0.000 | no/partial | yes | no/partial |
| `template_edges` | template | 169,290 | 38.914 | 241.03 | 3 | 31.547 | no/partial | yes | no/partial |
| `symbol_dict` | dictionary | 696,373 | 28.984 | 43.64 | 1 | 13.926 | no/partial | no | yes |
| `qname_prefix_dict` | dictionary | 155,741 | 21.242 | 143.02 | 1 | 3.156 | no/partial | yes | yes |
| `qualified_name_dict` | dictionary | 727,372 | 11.609 | 16.74 | 1 | 12.152 | no/partial | yes | no/partial |
| `source_spans` | proof_required | 0 | 0.004 | n/a | 2 | 0.004 | yes | no | no/partial |
| `file_source_spans` | proof_required | 62,005 | 6.480 | 109.59 | 2 | 5.812 | yes | no | no/partial |
| `path_evidence` | derived_cache | 4,096 | 8.125 | 2080.00 | 3 | 0.605 | no/partial | yes | yes |
| `files` | proof_required | 4,975 | 1.617 | 340.85 | 3 | 0.156 | yes | no | no/partial |
| `source_content_template` | template | 2,718 | 0.191 | 73.84 | 2 | 0.188 | no/partial | no | no/partial |
| `edges` | proof_required | 31,848 | 2.008 | 66.11 | 4 | 1.613 | yes | no | no/partial |
| `entities` | proof_required | 89,407 | 6.488 | 76.10 | 4 | 2.844 | yes | no | no/partial |
| `callsites` | structural | 2,177 | 0.121 | 58.33 | 1 | 0.000 | no/partial | no | no/partial |
| `callsite_args` | structural | 27,980 | 1.469 | 55.04 | 1 | 0.000 | no/partial | no | no/partial |
| `path_dict` | dictionary | 4,975 | 0.379 | 79.86 | 1 | 0.422 | no/partial | no | no/partial |

## Contributor Column Payloads

### `template_entities`

- Proof note: partly proof-required through duplicate-file overlay; over-broad if it keeps non-proof local symbols
- Compact path: local TEXT ids and repeated metadata can become compact integer/template references
| Column | Type | Non-null | Payload MiB | Avg B | Max B |
| --- | --- | --- | --- | --- | --- |
| `local_template_entity_id` | TEXT | 653,819 | 25.565 | 41.00 | 41 |
| `metadata_json` | TEXT | 653,819 | 25.436 | 40.79 | 220 |
| `content_template_id` | INTEGER | 653,819 | 4.988 | 8.00 | 8 |
| `kind_id` | INTEGER | 653,819 | 4.988 | 8.00 | 8 |
| `name_id` | INTEGER | 653,819 | 4.988 | 8.00 | 8 |
| `qualified_name_id` | INTEGER | 653,819 | 4.988 | 8.00 | 8 |
| `start_line` | INTEGER | 653,819 | 4.988 | 8.00 | 8 |
| `start_column` | INTEGER | 653,819 | 4.988 | 8.00 | 8 |

| Index | Unique | Columns | MiB |
| --- | --- | --- | --- |
| `sqlite_autoindex_template_entities_1` | True | content_template_id, local_template_entity_id | 0.000 |

### `template_edges`

- Proof note: proof-required only for duplicate-file overlay proof relations; structural/debug relations should not live here
- Compact path: local TEXT ids, repeated metadata, and unused directional indexes can compact
| Column | Type | Non-null | Payload MiB | Avg B | Max B |
| --- | --- | --- | --- | --- | --- |
| `local_head_entity_id` | TEXT | 169,290 | 6.619 | 41.00 | 41 |
| `local_tail_entity_id` | TEXT | 169,290 | 6.619 | 41.00 | 41 |
| `metadata_json` | TEXT | 169,290 | 6.453 | 39.97 | 43 |
| `local_template_edge_id` | TEXT | 169,290 | 6.296 | 39.00 | 39 |
| `content_template_id` | INTEGER | 169,290 | 1.292 | 8.00 | 8 |
| `relation_id` | INTEGER | 169,290 | 1.292 | 8.00 | 8 |
| `start_line` | INTEGER | 169,290 | 1.292 | 8.00 | 8 |
| `start_column` | INTEGER | 169,290 | 1.292 | 8.00 | 8 |

| Index | Unique | Columns | MiB |
| --- | --- | --- | --- |
| `idx_template_edges_tail_relation` | False | content_template_id, local_tail_entity_id, relation_id | 15.773 |
| `idx_template_edges_head_relation` | False | content_template_id, local_head_entity_id, relation_id | 15.773 |
| `sqlite_autoindex_template_edges_1` | True | content_template_id, local_template_edge_id | 0.000 |

### `symbol_dict`

- Proof note: required for human-readable names, but not all template/debug names need proof DB residency
- Compact path: split proof dictionary from audit/debug dictionary or intern local template ordinals
| Column | Type | Non-null | Payload MiB | Avg B | Max B |
| --- | --- | --- | --- | --- | --- |
| `value` | TEXT | 696,373 | 15.390 | 23.17 | 286 |
| `id` | INTEGER | 696,373 | 5.313 | 8.00 | 8 |
| `value_hash` | INTEGER | 696,373 | 5.313 | 8.00 | 8 |
| `value_len` | INTEGER | 696,373 | 5.313 | 8.00 | 8 |

| Index | Unique | Columns | MiB |
| --- | --- | --- | --- |
| `idx_symbol_dict_hash` | False | value_hash, value_len | 13.926 |

### `qname_prefix_dict`

- Proof note: required only for displayed qnames; many prefixes are derivable from hierarchy/path
- Compact path: reconstruct or sidecar long/debug prefixes
| Column | Type | Non-null | Payload MiB | Avg B | Max B |
| --- | --- | --- | --- | --- | --- |
| `value` | TEXT | 155,741 | 17.529 | 118.02 | 363 |
| `id` | INTEGER | 155,741 | 1.188 | 8.00 | 8 |
| `value_hash` | INTEGER | 155,741 | 1.188 | 8.00 | 8 |
| `value_len` | INTEGER | 155,741 | 1.188 | 8.00 | 8 |

| Index | Unique | Columns | MiB |
| --- | --- | --- | --- |
| `idx_qname_prefix_dict_hash` | False | value_hash, value_len | 3.156 |

### `qualified_name_dict`

- Proof note: required for lookup/display today; value text is already decomposed
- Compact path: further reduce indexes or template-only qnames
| Column | Type | Non-null | Payload MiB | Avg B | Max B |
| --- | --- | --- | --- | --- | --- |
| `id` | INTEGER | 727,372 | 5.549 | 8.00 | 8 |
| `prefix_id` | INTEGER | 727,372 | 5.549 | 8.00 | 8 |
| `suffix_id` | INTEGER | 727,372 | 5.549 | 8.00 | 8 |
| `value` | TEXT | 0 | 0.000 | 0.00 | 0 |

| Index | Unique | Columns | MiB |
| --- | --- | --- | --- |
| `idx_qualified_name_parts` | True | prefix_id, suffix_id | 12.152 |

### `source_spans`

- Proof note: zero physical rows in this artifact; spans are stored elsewhere
- Compact path: no target
| Column | Type | Non-null | Payload MiB | Avg B | Max B |
| --- | --- | --- | --- | --- | --- |
| `id_key` | INTEGER | 0 | 0.000 | 0.00 | 0 |
| `path_id` | INTEGER | 0 | 0.000 | 0.00 | 0 |
| `start_line` | INTEGER | 0 | 0.000 | 0.00 | 0 |
| `start_column` | INTEGER | 0 | 0.000 | 0.00 | 0 |
| `end_line` | INTEGER | 0 | 0.000 | 0.00 | 0 |
| `end_column` | INTEGER | 0 | 0.000 | 0.00 | 0 |

| Index | Unique | Columns | MiB |
| --- | --- | --- | --- |
| `idx_source_spans_path` | False | path_id | 0.004 |
| `sqlite_autoindex_source_spans_1` | True | id_key | 0.000 |

### `file_source_spans`

- Proof note: needed for source-span proof and stale cleanup
- Compact path: integerize file/span ids later
| Column | Type | Non-null | Payload MiB | Avg B | Max B |
| --- | --- | --- | --- | --- | --- |
| `file_id` | TEXT | 62,005 | 3.041 | 51.42 | 105 |
| `span_id` | TEXT | 62,005 | 2.306 | 39.00 | 39 |

| Index | Unique | Columns | MiB |
| --- | --- | --- | --- |
| `idx_file_source_spans_span` | False | span_id, file_id | 5.812 |
| `sqlite_autoindex_file_source_spans_1` | True | file_id, span_id | 0.000 |

### `path_evidence`

- Proof note: required for context quality, but verbose JSON is derivable from materialized path tables and proof edges/spans
- Compact path: move verbose JSON to sidecar or reconstruct
| Column | Type | Non-null | Payload MiB | Avg B | Max B |
| --- | --- | --- | --- | --- | --- |
| `metadata_json` | TEXT | 4,096 | 4.864 | 1245.17 | 1610 |
| `source_spans_json` | TEXT | 4,096 | 0.497 | 127.16 | 198 |
| `summary` | TEXT | 4,096 | 0.405 | 103.74 | 106 |
| `edges_json` | TEXT | 4,096 | 0.397 | 101.74 | 104 |
| `source` | TEXT | 4,096 | 0.160 | 41.00 | 41 |
| `target` | TEXT | 4,096 | 0.160 | 41.00 | 41 |
| `id` | TEXT | 4,096 | 0.090 | 23.00 | 23 |
| `exactness` | TEXT | 4,096 | 0.072 | 18.52 | 27 |

| Index | Unique | Columns | MiB |
| --- | --- | --- | --- |
| `idx_path_evidence_target` | False | target, length, confidence | 0.234 |
| `idx_path_evidence_source` | False | source, length, confidence | 0.234 |
| `sqlite_autoindex_path_evidence_1` | True | id | 0.137 |

### `files`

- Proof note: canonical file identity/snapshot table
- Compact path: content_hash is small but could become blob
| Column | Type | Non-null | Payload MiB | Avg B | Max B |
| --- | --- | --- | --- | --- | --- |
| `metadata_json` | TEXT | 4,975 | 1.129 | 238.00 | 238 |
| `content_hash` | TEXT | 4,975 | 0.104 | 22.00 | 22 |
| `file_id` | INTEGER | 4,975 | 0.038 | 8.00 | 8 |
| `path_id` | INTEGER | 4,975 | 0.038 | 8.00 | 8 |
| `size_bytes` | INTEGER | 4,975 | 0.038 | 8.00 | 8 |
| `language_id` | INTEGER | 4,975 | 0.038 | 8.00 | 8 |
| `indexed_at_unix_ms` | INTEGER | 4,975 | 0.038 | 8.00 | 8 |
| `content_template_id` | INTEGER | 4,975 | 0.038 | 8.00 | 8 |

| Index | Unique | Columns | MiB |
| --- | --- | --- | --- |
| `idx_files_content_template` | False | content_template_id, path_id | 0.098 |
| `sqlite_autoindex_files_1` | True | path_id | 0.059 |
| `sqlite_autoindex_files_2` | True | file_id | 0.000 |

### `source_content_template`

- Proof note: required for content-template dedupe
- Compact path: small
| Column | Type | Non-null | Payload MiB | Avg B | Max B |
| --- | --- | --- | --- | --- | --- |
| `content_hash` | TEXT | 2,718 | 0.057 | 22.00 | 22 |
| `extraction_version` | TEXT | 2,718 | 0.057 | 22.00 | 22 |
| `content_template_id` | INTEGER | 2,718 | 0.021 | 8.00 | 8 |
| `language_id` | INTEGER | 2,718 | 0.021 | 8.00 | 8 |
| `canonical_path_id` | INTEGER | 2,718 | 0.021 | 8.00 | 8 |
| `metadata_json` | TEXT | 2,718 | 0.005 | 2.00 | 2 |

| Index | Unique | Columns | MiB |
| --- | --- | --- | --- |
| `sqlite_autoindex_source_content_template_1` | True | content_hash, language_id, extraction_version | 0.188 |
| `sqlite_autoindex_source_content_template_2` | True | content_template_id | 0.000 |

### `edges`

- Proof note: proof edge table
- Compact path: already compact
| Column | Type | Non-null | Payload MiB | Avg B | Max B |
| --- | --- | --- | --- | --- | --- |
| `id_key` | INTEGER | 31,848 | 0.243 | 8.00 | 8 |
| `head_id_key` | INTEGER | 31,848 | 0.243 | 8.00 | 8 |
| `relation_id` | INTEGER | 31,848 | 0.243 | 8.00 | 8 |
| `tail_id_key` | INTEGER | 31,848 | 0.243 | 8.00 | 8 |
| `span_path_id` | INTEGER | 31,848 | 0.243 | 8.00 | 8 |
| `start_line` | INTEGER | 31,848 | 0.243 | 8.00 | 8 |
| `start_column` | INTEGER | 31,848 | 0.243 | 8.00 | 8 |
| `end_line` | INTEGER | 31,848 | 0.243 | 8.00 | 8 |

| Index | Unique | Columns | MiB |
| --- | --- | --- | --- |
| `idx_edges_tail_relation` | False | tail_id_key, relation_id | 0.566 |
| `idx_edges_head_relation` | False | head_id_key, relation_id | 0.566 |
| `idx_edges_span_path` | False | span_path_id | 0.480 |
| `sqlite_autoindex_edges_1` | True | id_key | 0.000 |

### `entities`

- Proof note: proof entity table
- Compact path: already compact except small metadata/hash fields
| Column | Type | Non-null | Payload MiB | Avg B | Max B |
| --- | --- | --- | --- | --- | --- |
| `entity_hash` | BLOB | 89,407 | 1.364 | 16.00 | 16 |
| `id_key` | INTEGER | 89,407 | 0.682 | 8.00 | 8 |
| `kind_id` | INTEGER | 89,407 | 0.682 | 8.00 | 8 |
| `name_id` | INTEGER | 89,407 | 0.682 | 8.00 | 8 |
| `qualified_name_id` | INTEGER | 89,407 | 0.682 | 8.00 | 8 |
| `path_id` | INTEGER | 89,407 | 0.682 | 8.00 | 8 |
| `span_path_id` | INTEGER | 89,407 | 0.682 | 8.00 | 8 |
| `start_line` | INTEGER | 89,407 | 0.682 | 8.00 | 8 |

| Index | Unique | Columns | MiB |
| --- | --- | --- | --- |
| `idx_entities_qname` | False | qualified_name_id | 0.980 |
| `idx_entities_name` | False | name_id | 0.969 |
| `idx_entities_path` | False | path_id | 0.895 |
| `sqlite_autoindex_entities_1` | True | id_key | 0.000 |

### `callsites`

- Proof note: typed structural records; queryable but not generic proof edges
- Compact path: small
| Column | Type | Non-null | Payload MiB | Avg B | Max B |
| --- | --- | --- | --- | --- | --- |
| `id_key` | INTEGER | 2,177 | 0.017 | 8.00 | 8 |
| `callsite_id_key` | INTEGER | 2,177 | 0.017 | 8.00 | 8 |
| `relation_id` | INTEGER | 2,177 | 0.017 | 8.00 | 8 |
| `callee_id_key` | INTEGER | 2,177 | 0.017 | 8.00 | 8 |
| `span_path_id` | INTEGER | 2,177 | 0.017 | 8.00 | 8 |
| `start_line` | INTEGER | 2,177 | 0.017 | 8.00 | 8 |
| `start_column` | INTEGER | 2,177 | 0.017 | 8.00 | 8 |
| `end_line` | INTEGER | 2,177 | 0.017 | 8.00 | 8 |

| Index | Unique | Columns | MiB |
| --- | --- | --- | --- |
| `sqlite_autoindex_callsites_1` | True | id_key | 0.000 |

### `callsite_args`

- Proof note: typed callsite argument records
- Compact path: small
| Column | Type | Non-null | Payload MiB | Avg B | Max B |
| --- | --- | --- | --- | --- | --- |
| `id_key` | INTEGER | 27,980 | 0.213 | 8.00 | 8 |
| `callsite_id_key` | INTEGER | 27,980 | 0.213 | 8.00 | 8 |
| `ordinal` | INTEGER | 27,980 | 0.213 | 8.00 | 8 |
| `relation_id` | INTEGER | 27,980 | 0.213 | 8.00 | 8 |
| `argument_id_key` | INTEGER | 27,980 | 0.213 | 8.00 | 8 |
| `span_path_id` | INTEGER | 27,980 | 0.213 | 8.00 | 8 |
| `start_line` | INTEGER | 27,980 | 0.213 | 8.00 | 8 |
| `start_column` | INTEGER | 27,980 | 0.213 | 8.00 | 8 |

| Index | Unique | Columns | MiB |
| --- | --- | --- | --- |
| `sqlite_autoindex_callsite_args_1` | True | id_key | 0.000 |

### `path_dict`

- Proof note: canonical paths are required for snippets/file identity
- Compact path: small
| Column | Type | Non-null | Payload MiB | Avg B | Max B |
| --- | --- | --- | --- | --- | --- |
| `value` | TEXT | 4,975 | 0.326 | 68.74 | 122 |
| `id` | INTEGER | 4,975 | 0.038 | 8.00 | 8 |

| Index | Unique | Columns | MiB |
| --- | --- | --- | --- |
| `sqlite_autoindex_path_dict_1` | True | value | 0.422 |

## Top 20 Columns By Estimated Payload

| Rank | Table | Column | Type | Payload MiB | Non-null | Avg B | Max B |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | `template_entities` | `local_template_entity_id` | TEXT | 25.565 | 653,819 | 41.00 | 41 |
| 2 | `template_entities` | `metadata_json` | TEXT | 25.436 | 653,819 | 40.79 | 220 |
| 3 | `qname_prefix_dict` | `value` | TEXT | 17.529 | 155,741 | 118.02 | 363 |
| 4 | `symbol_dict` | `value` | TEXT | 15.390 | 696,373 | 23.17 | 286 |
| 5 | `template_edges` | `local_head_entity_id` | TEXT | 6.619 | 169,290 | 41.00 | 41 |
| 6 | `template_edges` | `local_tail_entity_id` | TEXT | 6.619 | 169,290 | 41.00 | 41 |
| 7 | `template_edges` | `metadata_json` | TEXT | 6.453 | 169,290 | 39.97 | 43 |
| 8 | `template_edges` | `local_template_edge_id` | TEXT | 6.296 | 169,290 | 39.00 | 39 |
| 9 | `qualified_name_dict` | `id` | INTEGER | 5.549 | 727,372 | 8.00 | 8 |
| 10 | `qualified_name_dict` | `prefix_id` | INTEGER | 5.549 | 727,372 | 8.00 | 8 |
| 11 | `qualified_name_dict` | `suffix_id` | INTEGER | 5.549 | 727,372 | 8.00 | 8 |
| 12 | `symbol_dict` | `id` | INTEGER | 5.313 | 696,373 | 8.00 | 8 |
| 13 | `symbol_dict` | `value_hash` | INTEGER | 5.313 | 696,373 | 8.00 | 8 |
| 14 | `symbol_dict` | `value_len` | INTEGER | 5.313 | 696,373 | 8.00 | 8 |
| 15 | `template_entities` | `content_template_id` | INTEGER | 4.988 | 653,819 | 8.00 | 8 |
| 16 | `template_entities` | `kind_id` | INTEGER | 4.988 | 653,819 | 8.00 | 8 |
| 17 | `template_entities` | `name_id` | INTEGER | 4.988 | 653,819 | 8.00 | 8 |
| 18 | `template_entities` | `qualified_name_id` | INTEGER | 4.988 | 653,819 | 8.00 | 8 |
| 19 | `template_entities` | `start_line` | INTEGER | 4.988 | 653,819 | 8.00 | 8 |
| 20 | `template_entities` | `start_column` | INTEGER | 4.988 | 653,819 | 8.00 | 8 |

## Top 20 Indexes By Bytes

| Rank | Index | MiB | Bytes | Payload MiB |
| --- | --- | --- | --- | --- |
| 1 | `idx_template_edges_head_relation` | 15.773 | 16,539,648 | 15.176 |
| 2 | `idx_template_edges_tail_relation` | 15.773 | 16,539,648 | 15.176 |
| 3 | `idx_symbol_dict_hash` | 13.926 | 14,602,240 | 10.594 |
| 4 | `idx_qualified_name_parts` | 12.152 | 12,742,656 | 8.737 |
| 5 | `idx_file_entities_entity` | 8.676 | 9,097,216 | 8.271 |
| 6 | `idx_file_edges_edge` | 5.883 | 6,168,576 | 5.611 |
| 7 | `idx_file_source_spans_span` | 5.812 | 6,094,848 | 5.540 |
| 8 | `idx_qname_prefix_dict_hash` | 3.156 | 3,309,568 | 2.399 |
| 9 | `idx_entities_qname` | 0.980 | 1,028,096 | 0.718 |
| 10 | `idx_entities_name` | 0.969 | 1,015,808 | 0.706 |
| 11 | `idx_entities_path` | 0.895 | 937,984 | 0.632 |
| 12 | `idx_path_evidence_symbols_entity` | 0.609 | 638,976 | 0.578 |
| 13 | `idx_path_evidence_symbols_path` | 0.609 | 638,976 | 0.578 |
| 14 | `idx_edges_head_relation` | 0.566 | 593,920 | 0.472 |
| 15 | `idx_edges_tail_relation` | 0.566 | 593,920 | 0.472 |
| 16 | `idx_edges_span_path` | 0.480 | 503,808 | 0.384 |
| 17 | `sqlite_autoindex_path_dict_1` | 0.422 | 442,368 | 0.353 |
| 18 | `idx_path_evidence_lookup_source` | 0.301 | 315,392 | 0.276 |
| 19 | `idx_path_evidence_lookup_target` | 0.301 | 315,392 | 0.276 |
| 20 | `idx_file_path_evidence_path` | 0.262 | 274,432 | 0.236 |

## Template And Dictionary Indexes

### Template indexes
| Index | MiB |
| --- | --- |
| `idx_template_edges_head_relation` | 15.773 |
| `idx_template_edges_tail_relation` | 15.773 |
| `sqlite_autoindex_source_content_template_1` | 0.188 |
| `idx_files_content_template` | 0.098 |
| `sqlite_autoindex_source_content_template_2` | 0.000 |
| `sqlite_autoindex_template_edges_1` | 0.000 |
| `sqlite_autoindex_template_entities_1` | 0.000 |

### Dictionary indexes
| Index | MiB |
| --- | --- |
| `idx_symbol_dict_hash` | 13.926 |
| `idx_qualified_name_parts` | 12.152 |
| `idx_qname_prefix_dict_hash` | 3.156 |
| `sqlite_autoindex_path_dict_1` | 0.422 |
| `sqlite_autoindex_edge_provenance_dict_1` | 0.125 |
| `idx_object_id_dict_hash` | 0.004 |
| `sqlite_autoindex_edge_class_dict_1` | 0.004 |
| `sqlite_autoindex_edge_context_dict_1` | 0.004 |
| `sqlite_autoindex_entity_kind_dict_1` | 0.004 |
| `sqlite_autoindex_exactness_dict_1` | 0.004 |
| `sqlite_autoindex_extractor_dict_1` | 0.004 |
| `sqlite_autoindex_language_dict_1` | 0.004 |
| `sqlite_autoindex_relation_kind_dict_1` | 0.004 |
| `sqlite_autoindex_resolution_kind_dict_1` | 0.004 |

## Symbol And Prefix Duplicates

| Metric | symbol_dict | qname_prefix_dict |
| --- | --- | --- |
| rows | 696,373 | 155,741 |
| unique count | 696,373 | 155,741 |
| duplicate count | 0 | 0 |
| avg length | 23.17 | 118.02 |
| p95 length | 54 | 180 |
| max length | 286 | 363 |
| duplicate/hash collision groups | 0 | 0 |

## Template Density

| Metric | Value |
| --- | --- |
| content templates | 2,718 |
| file instances | 4,975 |
| template entities | 653,819 |
| template edges | 169,290 |
| template_entities per template | 240.55 |
| template_edges per template | 62.28 |
| duplicate templates reused by multiple files | 2404 |
| duplicate file instances | 2420 |
| max file instances per template | 14 |

### Largest templates by entity count
| Template | Entities | Files | Canonical path |
| --- | --- | --- | --- |
| 2345684228067928891 | 9749 | 2 | `runtime/arl-codex/codex-rs/tui/src/bottom_pane/chat_composer.rs` |
| 6177352509006711462 | 8113 | 2 | `runtime/arl-codex/codex-rs/tui/src/chatwidget.rs` |
| 6807434422762731922 | 7186 | 2 | `runtime/arl-codex/codex-rs/core/src/config/config_tests.rs` |
| 4907633544993687481 | 6860 | 2 | `runtime/arl-codex/sdk/python/src/codex_app_server/generated/v2_all.py` |
| 8615013886084405762 | 6084 | 2 | `runtime/arl-codex/codex-rs/tui/src/resume_picker.rs` |
| 4911029311906625827 | 5368 | 2 | `runtime/arl-codex/codex-rs/tui/src/history_cell.rs` |
| 4747138561753734429 | 4493 | 2 | `runtime/arl-codex/codex-rs/tui/src/app/tests.rs` |
| 4027941902125741481 | 4272 | 2 | `runtime/arl-codex/codex-rs/state/src/runtime/memories.rs` |
| 6770099863928595956 | 3594 | 2 | `runtime/arl-codex/codex-rs/core/src/tools/handlers/multi_agents_tests.rs` |
| 8063347016472880194 | 3488 | 2 | `runtime/arl-codex/codex-rs/tui/src/bottom_pane/textarea.rs` |

### Largest templates by edge count
| Template | Edges | Files | Canonical path |
| --- | --- | --- | --- |
| 2345684228067928891 | 2744 | 2 | `runtime/arl-codex/codex-rs/tui/src/bottom_pane/chat_composer.rs` |
| 6177352509006711462 | 2106 | 2 | `runtime/arl-codex/codex-rs/tui/src/chatwidget.rs` |
| 8615013886084405762 | 1463 | 2 | `runtime/arl-codex/codex-rs/tui/src/resume_picker.rs` |
| 8063347016472880194 | 1330 | 2 | `runtime/arl-codex/codex-rs/tui/src/bottom_pane/textarea.rs` |
| 4911029311906625827 | 1170 | 2 | `runtime/arl-codex/codex-rs/tui/src/history_cell.rs` |
| 6807434422762731922 | 1129 | 2 | `runtime/arl-codex/codex-rs/core/src/config/config_tests.rs` |
| 6770099863928595956 | 837 | 2 | `runtime/arl-codex/codex-rs/core/src/tools/handlers/multi_agents_tests.rs` |
| 6156521540213617262 | 835 | 2 | `runtime/arl-codex/codex-rs/tui/src/bottom_pane/mod.rs` |
| 2418721836340623652 | 789 | 2 | `runtime/arl-codex/codex-rs/tui/src/bottom_pane/request_user_input/mod.rs` |
| 4027941902125741481 | 750 | 2 | `runtime/arl-codex/codex-rs/state/src/runtime/memories.rs` |

### Duplicate templates reused by multiple files
| Template | Files | Canonical path |
| --- | --- | --- |
| 6204208347898340492 | 14 | `runtime/arl-codex/codex-rs/app-server/tests/all.rs` |
| 1148835255747160619 | 4 | `runtime/arl-codex/codex-rs/app-server-protocol/schema/typescript/NetworkPolicyRuleAction.ts` |
| 5991931316774985196 | 4 | `runtime/arl-codex/codex-rs/app-server-protocol/schema/typescript/NetworkPolicyAmendment.ts` |
| 5569694767409187 | 2 | `runtime/arl-codex/codex-rs/app-server-protocol/schema/typescript/ResourceTemplate.ts` |
| 9047096784330040 | 2 | `runtime/arl-codex/codex-rs/app-server-protocol/schema/typescript/v2/TurnCompletedNotification.ts` |
| 9169186481476394 | 2 | `runtime/arl-codex/codex-rs/app-server-protocol/schema/typescript/v2/ModelUpgradeInfo.ts` |
| 10904410468168358 | 2 | `runtime/arl-codex/codex-rs/tui/src/tui.rs` |
| 16591736894929484 | 2 | `runtime/arl-codex/codex-rs/execpolicy/tests/basic.rs` |
| 23019605660490135 | 2 | `runtime/arl-codex/codex-rs/execpolicy/src/executable_name.rs` |
| 29245242018810397 | 2 | `runtime/arl-codex/codex-rs/tui/src/shimmer.rs` |

### Template entity kind distribution
| Kind | Rows |
| --- | --- |
| `CallSite` | 251,037 |
| `Expression` | 220,823 |
| `LocalVariable` | 72,184 |
| `Import` | 34,643 |
| `Function` | 27,093 |
| `Parameter` | 27,039 |
| `Export` | 7,258 |
| `Module` | 4,410 |
| `Class` | 4,019 |
| `File` | 2,404 |
| `Enum` | 1,007 |
| `Migration` | 960 |
| `Type` | 234 |
| `Method` | 213 |
| `Promise` | 133 |
| `Task` | 118 |
| `Trait` | 88 |
| `ReturnSite` | 72 |
| `TestCase` | 37 |
| `Topic` | 15 |

### Template edge relation distribution
| Relation | Rows |
| --- | --- |
| `FLOWS_TO` | 73,702 |
| `DEFINES` | 34,825 |
| `IMPORTS` | 34,643 |
| `CALLS` | 17,967 |
| `EXPORTS` | 7,258 |
| `ASSIGNED_FROM` | 421 |
| `READS` | 248 |
| `WRITES` | 135 |
| `RETURNS` | 69 |
| `MUTATES` | 22 |

## Minimal Safe Candidate Changes

Minimum needed: `70.63 MiB`. The minimal measured three-change package is `73.99 MiB` lower-bound savings, giving `3.36 MiB` margin.

### `drop_unused_template_edge_directional_indexes`

- Estimate: `31.55 MiB` lower bound.
- Change: Drop idx_template_edges_head_relation and idx_template_edges_tail_relation from proof mode unless pushed-down template endpoint lookup is implemented. Current synthesized template endpoint lookup materializes then filters, so these indexes are not buying current default semantics.
- Safety disposition: proven unnecessary for current code path; can be audit/debug/build-only if future endpoint lookup needs it
- Risk: low if query latency gates pass

### `integerize_template_local_ids`

- Estimate: `36.24 MiB` lower bound.
- Change: Replace local template entity/edge/head/tail TEXT ids with compact per-template integer ordinals and reconstruct external ids at overlay time.
- Safety disposition: derivable from content_template_id plus local ordinal/hash; debug local strings can sidecar
- Risk: medium: touches overlay identity/import resolution; duplicate identity fixtures must pass

### `normalize_path_evidence_json_payloads`

- Estimate: `6.21 MiB` lower bound.
- Change: Move verbose PathEvidence JSON arrays/metadata to materialized path_evidence_edges/files/symbols/tests or debug sidecar, keeping compact path headers in proof mode.
- Safety disposition: derivable from materialized lookup tables and proof edges/spans; debug metadata can sidecar
- Risk: medium: context packet reader must reconstruct exactly

### `compact_template_metadata_json_defaults`

- Estimate: `32.21 MiB` lower bound.
- Change: Replace repeated template metadata/provenance JSON with compact flags/defaults and optional debug sidecar.
- Safety disposition: default metadata is derivable from schema; non-empty debug/provenance details can sidecar while preserving derived provenance fields
- Risk: low-medium: must preserve non-empty provenance/debug semantics

### `split_template_only_symbol_prefix_dictionaries`

- Estimate: `67.31 MiB` upper area, not direct claim.
- Change: Split proof-visible symbol/qname-prefix dictionaries from template-only/debug display strings, or reconstruct qname prefixes from entity hierarchy where possible.
- Safety disposition: proof lookup names remain; debug/template-only strings move to sidecar or become derivable
- Risk: medium-high: needs careful lookup compatibility and manual relation/context gates

### `content_hash_payload_status`

- Estimate: `0.00 MiB` lower bound.
- Change: Repeated entity/template content_hash is not a meaningful remaining lever in this artifact. The payload is already effectively zero in template_entities/entities.
- Safety disposition: already solved; do not count this toward the 70 MiB plan
- Risk: n/a

## Recommended Minimal Package

1. Drop `idx_template_edges_head_relation` and `idx_template_edges_tail_relation` from proof mode or make them build/debug-only: `31.55 MiB`.
2. Integerize template local entity/edge/head/tail IDs with reconstructable overlay IDs: `36.24 MiB` lower bound.
3. Normalize verbose PathEvidence JSON into existing materialized rows plus optional debug sidecar: `6.21 MiB` lower bound.

Combined lower-bound savings: `73.99 MiB`, enough to clear the `70.63 MiB` gap by `3.36 MiB`.

No data is removed in this phase. When implemented, each change must answer: Graph Truth still passes, Context Packet still passes, proof DB size decreases, and removed data is derivable, sidecarred, or proven unnecessary.
