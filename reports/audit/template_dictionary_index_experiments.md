# Template/Dictionary Index Experiments
Source DB: `reports\audit\artifacts\symbol_qname_compaction_probe_fast.sqlite`

Experiments used copied DBs only. The source DB was not mutated.

## Decisions
| Index | Bytes | Decision | Regressions | Notes |
| --- | ---: | --- | ---: | --- |
| `idx_symbol_dict_hash` | 3334144 | `keep` | 2 | retained; required lookup path or measured regression risk |
| `idx_qname_prefix_dict_hash` | 929792 | `keep` | 1 | retained; required lookup path or measured regression risk |
| `idx_qualified_name_parts` | 4255744 | `keep` | 1 | retained; required lookup path or measured regression risk |
| `idx_files_content_template` | 102400 | `drop` | 0 | safe low-value content-template helper index |

## P95 Latency Summary

### `idx_symbol_dict_hash`

| Query | With index p95 ms | Without index p95 ms |
| --- | ---: | ---: |
| `content_template_instances` | 0.0062 | 0.0074 |
| `duplicate_template_instances_scan` | 0.1022 | 0.1266 |
| `path_evidence_lookup` | 0.0063 | 0.0046 |
| `qname_lookup` | 0.0277 | 4.9642 |
| `relation_lookup` | 0.0045 | 0.0043 |
| `repeat_manifest_lookup` | 0.0046 | 0.0048 |
| `single_file_update_stale_lookup` | 0.0120 | 0.0111 |
| `symbol_lookup` | 0.0059 | 4.0570 |
| `template_edges_by_template` | 0.0858 | 0.0953 |
| `template_entities_by_template` | 0.0820 | 0.1034 |

Regressions:
- `symbol_lookup`: 0.0059 -> 4.0570 ms
- `qname_lookup`: 0.0277 -> 4.9642 ms

### `idx_qname_prefix_dict_hash`

| Query | With index p95 ms | Without index p95 ms |
| --- | ---: | ---: |
| `content_template_instances` | 0.0062 | 0.0125 |
| `duplicate_template_instances_scan` | 0.1022 | 0.2217 |
| `path_evidence_lookup` | 0.0063 | 0.0097 |
| `qname_lookup` | 0.0277 | 3.7755 |
| `relation_lookup` | 0.0045 | 0.0095 |
| `repeat_manifest_lookup` | 0.0046 | 0.0101 |
| `single_file_update_stale_lookup` | 0.0120 | 0.0231 |
| `symbol_lookup` | 0.0059 | 0.0074 |
| `template_edges_by_template` | 0.0858 | 0.1679 |
| `template_entities_by_template` | 0.0820 | 0.1512 |

Regressions:
- `qname_lookup`: 0.0277 -> 3.7755 ms

### `idx_qualified_name_parts`

| Query | With index p95 ms | Without index p95 ms |
| --- | ---: | ---: |
| `content_template_instances` | 0.0062 | 0.0131 |
| `duplicate_template_instances_scan` | 0.1022 | 0.2953 |
| `path_evidence_lookup` | 0.0063 | 0.0136 |
| `qname_lookup` | 0.0277 | 8.0735 |
| `relation_lookup` | 0.0045 | 0.0072 |
| `repeat_manifest_lookup` | 0.0046 | 0.0089 |
| `single_file_update_stale_lookup` | 0.0120 | 0.0409 |
| `symbol_lookup` | 0.0059 | 0.0056 |
| `template_edges_by_template` | 0.0858 | 0.2260 |
| `template_entities_by_template` | 0.0820 | 0.1624 |

Regressions:
- `qname_lookup`: 0.0277 -> 8.0735 ms

### `idx_files_content_template`

| Query | With index p95 ms | Without index p95 ms |
| --- | ---: | ---: |
| `content_template_instances` | 0.0062 | 0.9938 |
| `duplicate_template_instances_scan` | 0.1022 | 0.2757 |
| `path_evidence_lookup` | 0.0063 | 0.0054 |
| `qname_lookup` | 0.0277 | 0.0359 |
| `relation_lookup` | 0.0045 | 0.0055 |
| `repeat_manifest_lookup` | 0.0046 | 0.0065 |
| `single_file_update_stale_lookup` | 0.0120 | 0.0221 |
| `symbol_lookup` | 0.0059 | 0.0061 |
| `template_edges_by_template` | 0.0858 | 0.1576 |
| `template_entities_by_template` | 0.0820 | 0.0915 |

## Query Plans

### `idx_symbol_dict_hash`

`symbol_lookup` without index:

```text
2 | 0 | 216 | SCAN symbol_dict
```

`qname_prefix_lookup` without index:

```text
3 | 0 | 43 | SEARCH qname_prefix_dict USING INDEX idx_qname_prefix_dict_hash (value_hash=? AND value_len=?)
```

`qname_parts_lookup` without index:

```text
2 | 0 | 39 | SEARCH qualified_name_dict USING COVERING INDEX idx_qualified_name_parts (prefix_id=? AND suffix_id=?)
```

`content_template_instances` without index:

```text
3 | 0 | 40 | SEARCH f USING COVERING INDEX idx_files_content_template (content_template_id=?)
```

`template_entities_by_template` without index:

```text
4 | 0 | 95 | SEARCH template_entities USING PRIMARY KEY (content_template_id=?)
```

`template_edges_by_template` without index:

```text
4 | 0 | 76 | SEARCH template_edges USING PRIMARY KEY (content_template_id=?)
```

`relation_lookup` without index:

```text
4 | 0 | 46 | SEARCH edges USING INDEX idx_edges_head_relation (head_id_key=? AND relation_id=?)
```

`path_evidence_lookup` without index:

```text
5 | 0 | 49 | SEARCH path_evidence_lookup USING INDEX idx_path_evidence_lookup_source (source_id=? AND task_class=? AND relation_signature=?)
```

`single_file_update_stale_lookup` without index:

```text
3 | 0 | 100 | SEARCH file_edges USING PRIMARY KEY (file_id=?)
```

`repeat_manifest_lookup` without index:

```text
3 | 0 | 39 | SEARCH files USING INDEX sqlite_autoindex_files_1 (path_id=?)
```

### `idx_qname_prefix_dict_hash`

`symbol_lookup` without index:

```text
3 | 0 | 44 | SEARCH symbol_dict USING INDEX idx_symbol_dict_hash (value_hash=? AND value_len=?)
```

`qname_prefix_lookup` without index:

```text
2 | 0 | 216 | SCAN qname_prefix_dict
```

`qname_parts_lookup` without index:

```text
2 | 0 | 39 | SEARCH qualified_name_dict USING COVERING INDEX idx_qualified_name_parts (prefix_id=? AND suffix_id=?)
```

`content_template_instances` without index:

```text
3 | 0 | 40 | SEARCH f USING COVERING INDEX idx_files_content_template (content_template_id=?)
```

`template_entities_by_template` without index:

```text
4 | 0 | 95 | SEARCH template_entities USING PRIMARY KEY (content_template_id=?)
```

`template_edges_by_template` without index:

```text
4 | 0 | 76 | SEARCH template_edges USING PRIMARY KEY (content_template_id=?)
```

`relation_lookup` without index:

```text
4 | 0 | 46 | SEARCH edges USING INDEX idx_edges_head_relation (head_id_key=? AND relation_id=?)
```

`path_evidence_lookup` without index:

```text
5 | 0 | 49 | SEARCH path_evidence_lookup USING INDEX idx_path_evidence_lookup_source (source_id=? AND task_class=? AND relation_signature=?)
```

`single_file_update_stale_lookup` without index:

```text
3 | 0 | 100 | SEARCH file_edges USING PRIMARY KEY (file_id=?)
```

`repeat_manifest_lookup` without index:

```text
3 | 0 | 39 | SEARCH files USING INDEX sqlite_autoindex_files_1 (path_id=?)
```

### `idx_qualified_name_parts`

`symbol_lookup` without index:

```text
3 | 0 | 44 | SEARCH symbol_dict USING INDEX idx_symbol_dict_hash (value_hash=? AND value_len=?)
```

`qname_prefix_lookup` without index:

```text
3 | 0 | 43 | SEARCH qname_prefix_dict USING INDEX idx_qname_prefix_dict_hash (value_hash=? AND value_len=?)
```

`qname_parts_lookup` without index:

```text
2 | 0 | 216 | SCAN qualified_name_dict
```

`content_template_instances` without index:

```text
3 | 0 | 40 | SEARCH f USING COVERING INDEX idx_files_content_template (content_template_id=?)
```

`template_entities_by_template` without index:

```text
4 | 0 | 95 | SEARCH template_entities USING PRIMARY KEY (content_template_id=?)
```

`template_edges_by_template` without index:

```text
4 | 0 | 76 | SEARCH template_edges USING PRIMARY KEY (content_template_id=?)
```

`relation_lookup` without index:

```text
4 | 0 | 46 | SEARCH edges USING INDEX idx_edges_head_relation (head_id_key=? AND relation_id=?)
```

`path_evidence_lookup` without index:

```text
5 | 0 | 49 | SEARCH path_evidence_lookup USING INDEX idx_path_evidence_lookup_source (source_id=? AND task_class=? AND relation_signature=?)
```

`single_file_update_stale_lookup` without index:

```text
3 | 0 | 100 | SEARCH file_edges USING PRIMARY KEY (file_id=?)
```

`repeat_manifest_lookup` without index:

```text
3 | 0 | 39 | SEARCH files USING INDEX sqlite_autoindex_files_1 (path_id=?)
```

### `idx_files_content_template`

`symbol_lookup` without index:

```text
3 | 0 | 44 | SEARCH symbol_dict USING INDEX idx_symbol_dict_hash (value_hash=? AND value_len=?)
```

`qname_prefix_lookup` without index:

```text
3 | 0 | 43 | SEARCH qname_prefix_dict USING INDEX idx_qname_prefix_dict_hash (value_hash=? AND value_len=?)
```

`qname_parts_lookup` without index:

```text
2 | 0 | 39 | SEARCH qualified_name_dict USING COVERING INDEX idx_qualified_name_parts (prefix_id=? AND suffix_id=?)
```

`content_template_instances` without index:

```text
4 | 0 | 145 | SCAN f USING INDEX sqlite_autoindex_files_1
```

`template_entities_by_template` without index:

```text
4 | 0 | 95 | SEARCH template_entities USING PRIMARY KEY (content_template_id=?)
```

`template_edges_by_template` without index:

```text
4 | 0 | 76 | SEARCH template_edges USING PRIMARY KEY (content_template_id=?)
```

`relation_lookup` without index:

```text
4 | 0 | 46 | SEARCH edges USING INDEX idx_edges_head_relation (head_id_key=? AND relation_id=?)
```

`path_evidence_lookup` without index:

```text
5 | 0 | 49 | SEARCH path_evidence_lookup USING INDEX idx_path_evidence_lookup_source (source_id=? AND task_class=? AND relation_signature=?)
```

`single_file_update_stale_lookup` without index:

```text
3 | 0 | 100 | SEARCH file_edges USING PRIMARY KEY (file_id=?)
```

`repeat_manifest_lookup` without index:

```text
3 | 0 | 39 | SEARCH files USING INDEX sqlite_autoindex_files_1 (path_id=?)
```
