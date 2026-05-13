# Storage Experiments

Original DB: `<REPO_ROOT>/Desktop/development/codegraph-mcp/reports/final/artifacts/run_20260510_232527/fixture_workers4.sqlite`

Run dir: `<REPO_ROOT>/Desktop/development/codegraph-mcp/reports/final/artifacts/run_20260510_232527/query_latency_work/run-1778473556610`

Original file family bytes: `663552`

## Experiments

### `vacuum_analyze`

Copied DB: `<REPO_ROOT>/Desktop/development/codegraph-mcp/reports/final/artifacts/run_20260510_232527/query_latency_work/run-1778473556610/vacuum_analyze/codegraph.sqlite`

Copy removed: `true`

Mutations: `ANALYZE`, `PRAGMA optimize`, `VACUUM`

Recommendation: `recommended_for_next_safe_trial` (`recommended=true`)

Reason: Final copied-DB checkpoint preserved measured core/context query behavior; run the semantic gate before applying this to production artifacts.

| DB bytes before | DB bytes after | Delta bytes | Delta percent | Graph Truth status | Context packet status |
| ---: | ---: | ---: | ---: | --- | --- |
| 663552 | 618496 | -45056 | -6.79 | `not_run` | `queried` |

Notes:

- Maintenance-only experiment; graph semantics should be unchanged, but production adoption still needs the normal semantic gate.

| Checkpoint | DB bytes | WAL bytes | Edge index bytes | Dict table bytes | Unique text index bytes | FTS bytes | Source span bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `before` | 630784 | 0 | 57344 | 114688 | 110592 | 28672 | 8192 |
| `after_analyze` | 630784 | 0 | 57344 | 114688 | 110592 | 28672 | 8192 |
| `after_vacuum` | 618496 | 0 | 57344 | 114688 | 110592 | 28672 | 8192 |

#### Core Query Delta

| Query | Before ms | After ms | Delta ms | Before status | After status |
| --- | ---: | ---: | ---: | --- | --- |
| `entity_name_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `entity_qname_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `edge_head_relation_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `edge_tail_relation_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `edge_span_path_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `relation_count_scan` | 0 | 0 | 0 | `ok` | `ok` |
| `text_query_fts` | 0 | 0 | 0 | `ok` | `ok` |
| `relation_query_calls` | 0 | 0 | 0 | `ok` | `ok` |
| `context_pack_outbound` | 0 | 0 | 0 | `ok` | `ok` |
| `impact_inbound` | 0 | 0 | 0 | `ok` | `ok` |
| `unresolved_calls_paginated` | 0 | 0 | 0 | `ok` | `ok` |

#### Query Latencies

| Checkpoint | Query | ms | Rows | Status | Indexes | Full scans |
| --- | --- | ---: | ---: | --- | --- | --- |
| `before` | `entity_name_lookup` | 0 | 4 | `ok` | `idx_entities_name` | `SCAN entities` |
| `before` | `entity_qname_lookup` | 0 | 1 | `ok` | `idx_entities_qname` | `SCAN entities` |
| `before` | `edge_head_relation_lookup` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `edge_tail_relation_lookup` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `edge_span_path_lookup` | 0 | 64 | `ok` | `idx_edges_span_path` | `SCAN edges` |
| `before` | `relation_count_scan` | 0 | 27 | `ok` | `idx_edges_tail_relation` | `SCAN edges USING COVERING INDEX idx_edges_tail_relation` |
| `before` | `text_query_fts` | 0 | 0 | `ok` | none | `SCAN stage0_fts VIRTUAL TABLE INDEX 0:M6` |
| `before` | `relation_query_calls` | 0 | 20 | `ok` | `sqlite_autoindex_relation_kind_dict_1` | `SCAN e` |
| `before` | `context_pack_outbound` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `impact_inbound` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `unresolved_calls_paginated` | 0 | 19 | `ok` | `sqlite_autoindex_relation_kind_dict_1`, `id_key=?`, `rowid=?` | `SCAN e` |
| `after_analyze` | `entity_name_lookup` | 0 | 4 | `ok` | `idx_entities_name` | `SCAN entities` |
| `after_analyze` | `entity_qname_lookup` | 0 | 1 | `ok` | `idx_entities_qname` | `SCAN entities` |
| `after_analyze` | `edge_head_relation_lookup` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `after_analyze` | `edge_tail_relation_lookup` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `after_analyze` | `edge_span_path_lookup` | 0 | 64 | `ok` | `idx_edges_span_path` | `SCAN edges` |
| `after_analyze` | `relation_count_scan` | 0 | 27 | `ok` | `idx_edges_tail_relation` | `SCAN edges USING COVERING INDEX idx_edges_tail_relation` |
| `after_analyze` | `text_query_fts` | 0 | 0 | `ok` | none | `SCAN stage0_fts VIRTUAL TABLE INDEX 0:M6` |
| `after_analyze` | `relation_query_calls` | 0 | 20 | `ok` | `sqlite_autoindex_relation_kind_dict_1` | `SCAN e` |
| `after_analyze` | `context_pack_outbound` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `after_analyze` | `impact_inbound` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `after_analyze` | `unresolved_calls_paginated` | 0 | 19 | `ok` | `sqlite_autoindex_relation_kind_dict_1`, `id_key=?`, `rowid=?` | `SCAN e` |
| `after_vacuum` | `entity_name_lookup` | 0 | 4 | `ok` | `idx_entities_name` | `SCAN entities` |
| `after_vacuum` | `entity_qname_lookup` | 0 | 1 | `ok` | `idx_entities_qname` | `SCAN entities` |
| `after_vacuum` | `edge_head_relation_lookup` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `after_vacuum` | `edge_tail_relation_lookup` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `after_vacuum` | `edge_span_path_lookup` | 0 | 64 | `ok` | `idx_edges_span_path` | `SCAN edges` |
| `after_vacuum` | `relation_count_scan` | 0 | 27 | `ok` | `idx_edges_tail_relation` | `SCAN edges USING COVERING INDEX idx_edges_tail_relation` |
| `after_vacuum` | `text_query_fts` | 0 | 0 | `ok` | none | `SCAN stage0_fts VIRTUAL TABLE INDEX 0:M6` |
| `after_vacuum` | `relation_query_calls` | 0 | 20 | `ok` | `sqlite_autoindex_relation_kind_dict_1` | `SCAN e` |
| `after_vacuum` | `context_pack_outbound` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `after_vacuum` | `impact_inbound` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `after_vacuum` | `unresolved_calls_paginated` | 0 | 19 | `ok` | `sqlite_autoindex_relation_kind_dict_1`, `id_key=?`, `rowid=?` | `SCAN e` |

#### Degradation Flags

No query degradation crossed the audit threshold.

### `drop_recreate_edge_indexes`

Copied DB: `<REPO_ROOT>/Desktop/development/codegraph-mcp/reports/final/artifacts/run_20260510_232527/query_latency_work/run-1778473556610/drop_recreate_edge_indexes/codegraph.sqlite`

Copy removed: `true`

Mutations: `DROP INDEX idx_edges_head_relation`, `DROP INDEX idx_edges_tail_relation`, `DROP INDEX idx_edges_span_path`, `VACUUM`, `recreate edge indexes`, `ANALYZE`

Recommendation: `recommended_for_next_safe_trial` (`recommended=true`)

Reason: Final copied-DB checkpoint preserved measured core/context query behavior; run the semantic gate before applying this to production artifacts.

| DB bytes before | DB bytes after | Delta bytes | Delta percent | Graph Truth status | Context packet status |
| ---: | ---: | ---: | ---: | --- | --- |
| 663552 | 618496 | -45056 | -6.79 | `not_run` | `queried` |

Notes:

- Measures index rebuild cost and final size after restoring the same edge indexes.
- Temporary checkpoint after dropping indexes is expected to change query plans; recommendation is based on the final restored checkpoint.

| Checkpoint | DB bytes | WAL bytes | Edge index bytes | Dict table bytes | Unique text index bytes | FTS bytes | Source span bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `before` | 630784 | 0 | 57344 | 114688 | 110592 | 28672 | 8192 |
| `after_drop_edge_indexes` | 548864 | 0 | 0 | 114688 | 110592 | 28672 | 8192 |
| `after_recreate_edge_indexes` | 618496 | 0 | 57344 | 114688 | 110592 | 28672 | 8192 |

#### Core Query Delta

| Query | Before ms | After ms | Delta ms | Before status | After status |
| --- | ---: | ---: | ---: | --- | --- |
| `entity_name_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `entity_qname_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `edge_head_relation_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `edge_tail_relation_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `edge_span_path_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `relation_count_scan` | 0 | 0 | 0 | `ok` | `ok` |
| `text_query_fts` | 0 | 0 | 0 | `ok` | `ok` |
| `relation_query_calls` | 0 | 0 | 0 | `ok` | `ok` |
| `context_pack_outbound` | 0 | 0 | 0 | `ok` | `ok` |
| `impact_inbound` | 0 | 0 | 0 | `ok` | `ok` |
| `unresolved_calls_paginated` | 0 | 0 | 0 | `ok` | `ok` |

#### Query Latencies

| Checkpoint | Query | ms | Rows | Status | Indexes | Full scans |
| --- | --- | ---: | ---: | --- | --- | --- |
| `before` | `entity_name_lookup` | 0 | 4 | `ok` | `idx_entities_name` | `SCAN entities` |
| `before` | `entity_qname_lookup` | 0 | 1 | `ok` | `idx_entities_qname` | `SCAN entities` |
| `before` | `edge_head_relation_lookup` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `edge_tail_relation_lookup` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `edge_span_path_lookup` | 0 | 64 | `ok` | `idx_edges_span_path` | `SCAN edges` |
| `before` | `relation_count_scan` | 0 | 27 | `ok` | `idx_edges_tail_relation` | `SCAN edges USING COVERING INDEX idx_edges_tail_relation` |
| `before` | `text_query_fts` | 0 | 0 | `ok` | none | `SCAN stage0_fts VIRTUAL TABLE INDEX 0:M6` |
| `before` | `relation_query_calls` | 0 | 20 | `ok` | `sqlite_autoindex_relation_kind_dict_1` | `SCAN e` |
| `before` | `context_pack_outbound` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `impact_inbound` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `unresolved_calls_paginated` | 0 | 19 | `ok` | `sqlite_autoindex_relation_kind_dict_1`, `id_key=?`, `rowid=?` | `SCAN e` |
| `after_drop_edge_indexes` | `entity_name_lookup` | 0 | 4 | `ok` | `idx_entities_name` | `SCAN entities` |
| `after_drop_edge_indexes` | `entity_qname_lookup` | 0 | 1 | `ok` | `idx_entities_qname` | `SCAN entities` |
| `after_drop_edge_indexes` | `edge_head_relation_lookup` | 0 | 1 | `ok` | none | `SCAN edges`, `SCAN edges`, `SCAN edges` |
| `after_drop_edge_indexes` | `edge_tail_relation_lookup` | 0 | 10 | `ok` | none | `SCAN edges`, `SCAN edges`, `SCAN edges` |
| `after_drop_edge_indexes` | `edge_span_path_lookup` | 0 | 64 | `ok` | none | `SCAN edges`, `SCAN edges` |
| `after_drop_edge_indexes` | `relation_count_scan` | 0 | 27 | `ok` | none | `SCAN edges` |
| `after_drop_edge_indexes` | `text_query_fts` | 0 | 0 | `ok` | none | `SCAN stage0_fts VIRTUAL TABLE INDEX 0:M6` |
| `after_drop_edge_indexes` | `relation_query_calls` | 0 | 20 | `ok` | `sqlite_autoindex_relation_kind_dict_1` | `SCAN e` |
| `after_drop_edge_indexes` | `context_pack_outbound` | 0 | 1 | `ok` | none | `SCAN e`, `SCAN edges`, `SCAN edges` |
| `after_drop_edge_indexes` | `impact_inbound` | 0 | 10 | `ok` | none | `SCAN e`, `SCAN edges`, `SCAN edges` |
| `after_drop_edge_indexes` | `unresolved_calls_paginated` | 0 | 19 | `ok` | `sqlite_autoindex_relation_kind_dict_1`, `id_key=?`, `rowid=?` | `SCAN e` |
| `after_recreate_edge_indexes` | `entity_name_lookup` | 0 | 4 | `ok` | `idx_entities_name` | `SCAN entities` |
| `after_recreate_edge_indexes` | `entity_qname_lookup` | 0 | 1 | `ok` | `idx_entities_qname` | `SCAN entities` |
| `after_recreate_edge_indexes` | `edge_head_relation_lookup` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `after_recreate_edge_indexes` | `edge_tail_relation_lookup` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `after_recreate_edge_indexes` | `edge_span_path_lookup` | 0 | 64 | `ok` | `idx_edges_span_path` | `SCAN edges` |
| `after_recreate_edge_indexes` | `relation_count_scan` | 0 | 27 | `ok` | `idx_edges_tail_relation` | `SCAN edges USING COVERING INDEX idx_edges_tail_relation` |
| `after_recreate_edge_indexes` | `text_query_fts` | 0 | 0 | `ok` | none | `SCAN stage0_fts VIRTUAL TABLE INDEX 0:M6` |
| `after_recreate_edge_indexes` | `relation_query_calls` | 0 | 20 | `ok` | `sqlite_autoindex_relation_kind_dict_1` | `SCAN e` |
| `after_recreate_edge_indexes` | `context_pack_outbound` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `after_recreate_edge_indexes` | `impact_inbound` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `after_recreate_edge_indexes` | `unresolved_calls_paginated` | 0 | 19 | `ok` | `sqlite_autoindex_relation_kind_dict_1`, `id_key=?`, `rowid=?` | `SCAN e` |

#### Degradation Flags

No query degradation crossed the audit threshold.

### `drop_broad_unused_indexes`

Copied DB: `<REPO_ROOT>/Desktop/development/codegraph-mcp/reports/final/artifacts/run_20260510_232527/query_latency_work/run-1778473556610/drop_broad_unused_indexes/codegraph.sqlite`

Copy removed: `true`

Mutations: `DROP INDEX idx_edges_span_path`, `DROP INDEX idx_source_spans_path`, `DROP INDEX idx_retrieval_traces_created`, `VACUUM`, `ANALYZE`

Recommendation: `not_recommended_for_production_yet` (`recommended=false`)

Reason: The copied-DB result changes schema/query semantics and needs a reducer/query-layer design plus Graph Truth before adoption.

| DB bytes before | DB bytes after | Delta bytes | Delta percent | Graph Truth status | Context packet status |
| ---: | ---: | ---: | ---: | --- | --- |
| 663552 | 585728 | -77824 | -11.73 | `not_run` | `queried` |

Notes:

- Drops broad indexes called out by storage forensics as unused or weakly justified by default workflows.
- This is a copied-DB measurement only; source-span and retrieval-trace workflows need explicit validation before any schema change.

| Checkpoint | DB bytes | WAL bytes | Edge index bytes | Dict table bytes | Unique text index bytes | FTS bytes | Source span bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `before` | 630784 | 0 | 57344 | 114688 | 110592 | 28672 | 8192 |
| `after_drop_broad_indexes` | 585728 | 0 | 40960 | 114688 | 110592 | 28672 | 4096 |

#### Core Query Delta

| Query | Before ms | After ms | Delta ms | Before status | After status |
| --- | ---: | ---: | ---: | --- | --- |
| `entity_name_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `entity_qname_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `edge_head_relation_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `edge_tail_relation_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `edge_span_path_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `relation_count_scan` | 0 | 0 | 0 | `ok` | `ok` |
| `text_query_fts` | 0 | 0 | 0 | `ok` | `ok` |
| `relation_query_calls` | 0 | 0 | 0 | `ok` | `ok` |
| `context_pack_outbound` | 0 | 0 | 0 | `ok` | `ok` |
| `impact_inbound` | 0 | 0 | 0 | `ok` | `ok` |
| `unresolved_calls_paginated` | 0 | 0 | 0 | `ok` | `ok` |

#### Query Latencies

| Checkpoint | Query | ms | Rows | Status | Indexes | Full scans |
| --- | --- | ---: | ---: | --- | --- | --- |
| `before` | `entity_name_lookup` | 0 | 4 | `ok` | `idx_entities_name` | `SCAN entities` |
| `before` | `entity_qname_lookup` | 0 | 1 | `ok` | `idx_entities_qname` | `SCAN entities` |
| `before` | `edge_head_relation_lookup` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `edge_tail_relation_lookup` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `edge_span_path_lookup` | 0 | 64 | `ok` | `idx_edges_span_path` | `SCAN edges` |
| `before` | `relation_count_scan` | 0 | 27 | `ok` | `idx_edges_tail_relation` | `SCAN edges USING COVERING INDEX idx_edges_tail_relation` |
| `before` | `text_query_fts` | 0 | 0 | `ok` | none | `SCAN stage0_fts VIRTUAL TABLE INDEX 0:M6` |
| `before` | `relation_query_calls` | 0 | 20 | `ok` | `sqlite_autoindex_relation_kind_dict_1` | `SCAN e` |
| `before` | `context_pack_outbound` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `impact_inbound` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `unresolved_calls_paginated` | 0 | 19 | `ok` | `sqlite_autoindex_relation_kind_dict_1`, `id_key=?`, `rowid=?` | `SCAN e` |
| `after_drop_broad_indexes` | `entity_name_lookup` | 0 | 4 | `ok` | `idx_entities_name` | `SCAN entities` |
| `after_drop_broad_indexes` | `entity_qname_lookup` | 0 | 1 | `ok` | `idx_entities_qname` | `SCAN entities` |
| `after_drop_broad_indexes` | `edge_head_relation_lookup` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `after_drop_broad_indexes` | `edge_tail_relation_lookup` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `after_drop_broad_indexes` | `edge_span_path_lookup` | 0 | 64 | `ok` | none | `SCAN edges`, `SCAN edges` |
| `after_drop_broad_indexes` | `relation_count_scan` | 0 | 27 | `ok` | `idx_edges_tail_relation` | `SCAN edges USING COVERING INDEX idx_edges_tail_relation` |
| `after_drop_broad_indexes` | `text_query_fts` | 0 | 0 | `ok` | none | `SCAN stage0_fts VIRTUAL TABLE INDEX 0:M6` |
| `after_drop_broad_indexes` | `relation_query_calls` | 0 | 20 | `ok` | `sqlite_autoindex_relation_kind_dict_1` | `SCAN e` |
| `after_drop_broad_indexes` | `context_pack_outbound` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `after_drop_broad_indexes` | `impact_inbound` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `after_drop_broad_indexes` | `unresolved_calls_paginated` | 0 | 19 | `ok` | `sqlite_autoindex_relation_kind_dict_1`, `id_key=?`, `rowid=?` | `SCAN e` |

#### Degradation Flags

No query degradation crossed the audit threshold.

### `replace_broad_with_partial_index`

Copied DB: `<REPO_ROOT>/Desktop/development/codegraph-mcp/reports/final/artifacts/run_20260510_232527/query_latency_work/run-1778473556610/replace_broad_with_partial_index/codegraph.sqlite`

Copy removed: `true`

Mutations: `DROP INDEX idx_edges_span_path`, `CREATE PARTIAL INDEX idx_edges_calls_partial_heuristic`, `ANALYZE`, `VACUUM`

Recommendation: `not_recommended_for_production_yet` (`recommended=false`)

Reason: The copied-DB result changes schema/query semantics and needs a reducer/query-layer design plus Graph Truth before adoption.

| DB bytes before | DB bytes after | Delta bytes | Delta percent | Graph Truth status | Context packet status |
| ---: | ---: | ---: | ---: | --- | --- |
| 663552 | 602112 | -61440 | -9.26 | `not_run` | `queried` |

Notes:

- Replaces the broad span-path edge index with a CALLS-focused partial index when the CALLS relation id is present.
- The partial index is experimental; it is only useful if EXPLAIN shows default relation/unresolved-call queries stop scanning the whole edge table.

| Checkpoint | DB bytes | WAL bytes | Edge index bytes | Dict table bytes | Unique text index bytes | FTS bytes | Source span bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `before` | 630784 | 0 | 57344 | 114688 | 110592 | 28672 | 8192 |
| `after_partial_calls_index` | 602112 | 0 | 45056 | 114688 | 110592 | 28672 | 8192 |

#### Core Query Delta

| Query | Before ms | After ms | Delta ms | Before status | After status |
| --- | ---: | ---: | ---: | --- | --- |
| `entity_name_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `entity_qname_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `edge_head_relation_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `edge_tail_relation_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `edge_span_path_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `relation_count_scan` | 0 | 0 | 0 | `ok` | `ok` |
| `text_query_fts` | 0 | 0 | 0 | `ok` | `ok` |
| `relation_query_calls` | 0 | 0 | 0 | `ok` | `ok` |
| `context_pack_outbound` | 0 | 0 | 0 | `ok` | `ok` |
| `impact_inbound` | 0 | 0 | 0 | `ok` | `ok` |
| `unresolved_calls_paginated` | 0 | 0 | 0 | `ok` | `ok` |

#### Query Latencies

| Checkpoint | Query | ms | Rows | Status | Indexes | Full scans |
| --- | --- | ---: | ---: | --- | --- | --- |
| `before` | `entity_name_lookup` | 0 | 4 | `ok` | `idx_entities_name` | `SCAN entities` |
| `before` | `entity_qname_lookup` | 0 | 1 | `ok` | `idx_entities_qname` | `SCAN entities` |
| `before` | `edge_head_relation_lookup` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `edge_tail_relation_lookup` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `edge_span_path_lookup` | 0 | 64 | `ok` | `idx_edges_span_path` | `SCAN edges` |
| `before` | `relation_count_scan` | 0 | 27 | `ok` | `idx_edges_tail_relation` | `SCAN edges USING COVERING INDEX idx_edges_tail_relation` |
| `before` | `text_query_fts` | 0 | 0 | `ok` | none | `SCAN stage0_fts VIRTUAL TABLE INDEX 0:M6` |
| `before` | `relation_query_calls` | 0 | 20 | `ok` | `sqlite_autoindex_relation_kind_dict_1` | `SCAN e` |
| `before` | `context_pack_outbound` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `impact_inbound` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `unresolved_calls_paginated` | 0 | 19 | `ok` | `sqlite_autoindex_relation_kind_dict_1`, `id_key=?`, `rowid=?` | `SCAN e` |
| `after_partial_calls_index` | `entity_name_lookup` | 0 | 4 | `ok` | `idx_entities_name` | `SCAN entities` |
| `after_partial_calls_index` | `entity_qname_lookup` | 0 | 1 | `ok` | `idx_entities_qname` | `SCAN entities` |
| `after_partial_calls_index` | `edge_head_relation_lookup` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `after_partial_calls_index` | `edge_tail_relation_lookup` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `after_partial_calls_index` | `edge_span_path_lookup` | 0 | 64 | `ok` | none | `SCAN edges`, `SCAN edges` |
| `after_partial_calls_index` | `relation_count_scan` | 0 | 27 | `ok` | `idx_edges_tail_relation` | `SCAN edges USING COVERING INDEX idx_edges_tail_relation` |
| `after_partial_calls_index` | `text_query_fts` | 0 | 0 | `ok` | none | `SCAN stage0_fts VIRTUAL TABLE INDEX 0:M6` |
| `after_partial_calls_index` | `relation_query_calls` | 0 | 20 | `ok` | `sqlite_autoindex_relation_kind_dict_1` | `SCAN e` |
| `after_partial_calls_index` | `context_pack_outbound` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `after_partial_calls_index` | `impact_inbound` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `after_partial_calls_index` | `unresolved_calls_paginated` | 0 | 19 | `ok` | `sqlite_autoindex_relation_kind_dict_1`, `id_key=?`, `rowid=?` | `SCAN e` |

#### Degradation Flags

No query degradation crossed the audit threshold.

### `simulate_compact_qualified_names`

Copied DB: `<REPO_ROOT>/Desktop/development/codegraph-mcp/reports/final/artifacts/run_20260510_232527/query_latency_work/run-1778473556610/simulate_compact_qualified_names/codegraph.sqlite`

Copy removed: `true`

Mutations: `UPDATE qualified_name_dict.value to compact tuple surrogate`, `VACUUM`, `ANALYZE`

Recommendation: `not_recommended_for_production_yet` (`recommended=false`)

Reason: The copied-DB result changes schema/query semantics and needs a reducer/query-layer design plus Graph Truth before adoption.

| DB bytes before | DB bytes after | Delta bytes | Delta percent | Graph Truth status | Context packet status |
| ---: | ---: | ---: | ---: | --- | --- |
| 663552 | 565248 | -98304 | -14.81 | `not_run` | `queried` |

Notes:

- Simulates replacing full qualified-name text with compact tuple text on a copied DB.
- This intentionally breaks human-readable qualified-name query output and is not a production schema change.

| Checkpoint | DB bytes | WAL bytes | Edge index bytes | Dict table bytes | Unique text index bytes | FTS bytes | Source span bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `before` | 630784 | 0 | 57344 | 114688 | 110592 | 28672 | 8192 |
| `after_compact_qname_simulation` | 565248 | 0 | 57344 | 90112 | 86016 | 28672 | 8192 |

#### Core Query Delta

| Query | Before ms | After ms | Delta ms | Before status | After status |
| --- | ---: | ---: | ---: | --- | --- |
| `entity_name_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `entity_qname_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `edge_head_relation_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `edge_tail_relation_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `edge_span_path_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `relation_count_scan` | 0 | 0 | 0 | `ok` | `ok` |
| `text_query_fts` | 0 | 0 | 0 | `ok` | `ok` |
| `relation_query_calls` | 0 | 0 | 0 | `ok` | `ok` |
| `context_pack_outbound` | 0 | 0 | 0 | `ok` | `ok` |
| `impact_inbound` | 0 | 0 | 0 | `ok` | `ok` |
| `unresolved_calls_paginated` | 0 | 0 | 0 | `ok` | `ok` |

#### Query Latencies

| Checkpoint | Query | ms | Rows | Status | Indexes | Full scans |
| --- | --- | ---: | ---: | --- | --- | --- |
| `before` | `entity_name_lookup` | 0 | 4 | `ok` | `idx_entities_name` | `SCAN entities` |
| `before` | `entity_qname_lookup` | 0 | 1 | `ok` | `idx_entities_qname` | `SCAN entities` |
| `before` | `edge_head_relation_lookup` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `edge_tail_relation_lookup` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `edge_span_path_lookup` | 0 | 64 | `ok` | `idx_edges_span_path` | `SCAN edges` |
| `before` | `relation_count_scan` | 0 | 27 | `ok` | `idx_edges_tail_relation` | `SCAN edges USING COVERING INDEX idx_edges_tail_relation` |
| `before` | `text_query_fts` | 0 | 0 | `ok` | none | `SCAN stage0_fts VIRTUAL TABLE INDEX 0:M6` |
| `before` | `relation_query_calls` | 0 | 20 | `ok` | `sqlite_autoindex_relation_kind_dict_1` | `SCAN e` |
| `before` | `context_pack_outbound` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `impact_inbound` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `unresolved_calls_paginated` | 0 | 19 | `ok` | `sqlite_autoindex_relation_kind_dict_1`, `id_key=?`, `rowid=?` | `SCAN e` |
| `after_compact_qname_simulation` | `entity_name_lookup` | 0 | 4 | `ok` | `idx_entities_name` | `SCAN entities` |
| `after_compact_qname_simulation` | `entity_qname_lookup` | 0 | 1 | `ok` | `idx_entities_qname` | `SCAN entities` |
| `after_compact_qname_simulation` | `edge_head_relation_lookup` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `after_compact_qname_simulation` | `edge_tail_relation_lookup` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `after_compact_qname_simulation` | `edge_span_path_lookup` | 0 | 64 | `ok` | `idx_edges_span_path` | `SCAN edges` |
| `after_compact_qname_simulation` | `relation_count_scan` | 0 | 27 | `ok` | `idx_edges_tail_relation` | `SCAN edges USING COVERING INDEX idx_edges_tail_relation` |
| `after_compact_qname_simulation` | `text_query_fts` | 0 | 0 | `ok` | none | `SCAN stage0_fts VIRTUAL TABLE INDEX 0:M6` |
| `after_compact_qname_simulation` | `relation_query_calls` | 0 | 20 | `ok` | `sqlite_autoindex_relation_kind_dict_1` | `SCAN e` |
| `after_compact_qname_simulation` | `context_pack_outbound` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `after_compact_qname_simulation` | `impact_inbound` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `after_compact_qname_simulation` | `unresolved_calls_paginated` | 0 | 19 | `ok` | `sqlite_autoindex_relation_kind_dict_1`, `id_key=?`, `rowid=?` | `SCAN e` |

#### Degradation Flags

No query degradation crossed the audit threshold.

### `simulate_exact_base_partition`

Copied DB: `<REPO_ROOT>/Desktop/development/codegraph-mcp/reports/final/artifacts/run_20260510_232527/query_latency_work/run-1778473556610/simulate_exact_base_partition/codegraph.sqlite`

Copy removed: `true`

Mutations: `CREATE TABLE edges_non_proof_sim AS non-proof edges`, `DELETE non-proof rows from edges`, `VACUUM`, `ANALYZE`

Recommendation: `not_recommended_for_production_yet` (`recommended=false`)

Reason: The copied-DB result changes schema/query semantics and needs a reducer/query-layer design plus Graph Truth before adoption.

| DB bytes before | DB bytes after | Delta bytes | Delta percent | Graph Truth status | Context packet status |
| ---: | ---: | ---: | ---: | --- | --- |
| 663552 | 610304 | -53248 | -8.02 | `not_run` | `queried` |

Notes:

- Simulates separating proof-grade base graph rows from heuristic/debug/test rows by moving non-proof edges into a side table.
- The simulation changes graph query answers, so it is a storage-forensics candidate only and cannot be recommended without reducer/query-layer changes.

| Checkpoint | DB bytes | WAL bytes | Edge index bytes | Dict table bytes | Unique text index bytes | FTS bytes | Source span bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `before` | 630784 | 0 | 57344 | 114688 | 110592 | 28672 | 8192 |
| `after_exact_base_partition_simulation` | 610304 | 0 | 45056 | 114688 | 110592 | 28672 | 8192 |

#### Core Query Delta

| Query | Before ms | After ms | Delta ms | Before status | After status |
| --- | ---: | ---: | ---: | --- | --- |
| `entity_name_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `entity_qname_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `edge_head_relation_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `edge_tail_relation_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `edge_span_path_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `relation_count_scan` | 0 | 0 | 0 | `ok` | `ok` |
| `text_query_fts` | 0 | 0 | 0 | `ok` | `ok` |
| `relation_query_calls` | 0 | 0 | 0 | `ok` | `ok` |
| `context_pack_outbound` | 0 | 0 | 0 | `ok` | `ok` |
| `impact_inbound` | 0 | 0 | 0 | `ok` | `ok` |
| `unresolved_calls_paginated` | 0 | 0 | 0 | `ok` | `ok` |

#### Query Latencies

| Checkpoint | Query | ms | Rows | Status | Indexes | Full scans |
| --- | --- | ---: | ---: | --- | --- | --- |
| `before` | `entity_name_lookup` | 0 | 4 | `ok` | `idx_entities_name` | `SCAN entities` |
| `before` | `entity_qname_lookup` | 0 | 1 | `ok` | `idx_entities_qname` | `SCAN entities` |
| `before` | `edge_head_relation_lookup` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `edge_tail_relation_lookup` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `edge_span_path_lookup` | 0 | 64 | `ok` | `idx_edges_span_path` | `SCAN edges` |
| `before` | `relation_count_scan` | 0 | 27 | `ok` | `idx_edges_tail_relation` | `SCAN edges USING COVERING INDEX idx_edges_tail_relation` |
| `before` | `text_query_fts` | 0 | 0 | `ok` | none | `SCAN stage0_fts VIRTUAL TABLE INDEX 0:M6` |
| `before` | `relation_query_calls` | 0 | 20 | `ok` | `sqlite_autoindex_relation_kind_dict_1` | `SCAN e` |
| `before` | `context_pack_outbound` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `impact_inbound` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `unresolved_calls_paginated` | 0 | 19 | `ok` | `sqlite_autoindex_relation_kind_dict_1`, `id_key=?`, `rowid=?` | `SCAN e` |
| `after_exact_base_partition_simulation` | `entity_name_lookup` | 0 | 4 | `ok` | `idx_entities_name` | `SCAN entities` |
| `after_exact_base_partition_simulation` | `entity_qname_lookup` | 0 | 1 | `ok` | `idx_entities_qname` | `SCAN entities` |
| `after_exact_base_partition_simulation` | `edge_head_relation_lookup` | 0 | 3 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `after_exact_base_partition_simulation` | `edge_tail_relation_lookup` | 0 | 1 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `after_exact_base_partition_simulation` | `edge_span_path_lookup` | 0 | 54 | `ok` | `idx_edges_span_path` | `SCAN edges` |
| `after_exact_base_partition_simulation` | `relation_count_scan` | 0 | 18 | `ok` | `idx_edges_tail_relation` | `SCAN edges USING COVERING INDEX idx_edges_tail_relation` |
| `after_exact_base_partition_simulation` | `text_query_fts` | 0 | 0 | `ok` | none | `SCAN stage0_fts VIRTUAL TABLE INDEX 0:M6` |
| `after_exact_base_partition_simulation` | `relation_query_calls` | 0 | 11 | `ok` | `sqlite_autoindex_relation_kind_dict_1` | `SCAN e` |
| `after_exact_base_partition_simulation` | `context_pack_outbound` | 0 | 3 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `after_exact_base_partition_simulation` | `impact_inbound` | 0 | 1 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `after_exact_base_partition_simulation` | `unresolved_calls_paginated` | 0 | 0 | `ok` | `sqlite_autoindex_relation_kind_dict_1`, `id_key=?`, `rowid=?` | `SCAN e` |

#### Degradation Flags

No query degradation crossed the audit threshold.

### `disable_fts_snippets_simulation`

Copied DB: `<REPO_ROOT>/Desktop/development/codegraph-mcp/reports/final/artifacts/run_20260510_232527/query_latency_work/run-1778473556610/disable_fts_snippets_simulation/codegraph.sqlite`

Copy removed: `true`

Mutations: `DELETE FROM stage0_fts`, `VACUUM`, `ANALYZE`

Recommendation: `not_recommended_for_production_yet` (`recommended=false`)

Reason: The copied-DB result changes schema/query semantics and needs a reducer/query-layer design plus Graph Truth before adoption.

| DB bytes before | DB bytes after | Delta bytes | Delta percent | Graph Truth status | Context packet status |
| ---: | ---: | ---: | ---: | --- | --- |
| 663552 | 618496 | -45056 | -6.79 | `not_run` | `queried` |

Notes:

- Simulates compact mode with no FTS/snippet payload in SQLite by clearing stage0_fts on a copy.
- This is expected to break text-query workflows unless a replacement text index exists outside SQLite.

| Checkpoint | DB bytes | WAL bytes | Edge index bytes | Dict table bytes | Unique text index bytes | FTS bytes | Source span bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `before` | 630784 | 0 | 57344 | 114688 | 110592 | 28672 | 8192 |
| `after_disable_fts_snippets_simulation` | 618496 | 0 | 57344 | 114688 | 110592 | 28672 | 8192 |

#### Core Query Delta

| Query | Before ms | After ms | Delta ms | Before status | After status |
| --- | ---: | ---: | ---: | --- | --- |
| `entity_name_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `entity_qname_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `edge_head_relation_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `edge_tail_relation_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `edge_span_path_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `relation_count_scan` | 0 | 0 | 0 | `ok` | `ok` |
| `text_query_fts` | 0 | 0 | 0 | `ok` | `ok` |
| `relation_query_calls` | 0 | 0 | 0 | `ok` | `ok` |
| `context_pack_outbound` | 0 | 0 | 0 | `ok` | `ok` |
| `impact_inbound` | 0 | 0 | 0 | `ok` | `ok` |
| `unresolved_calls_paginated` | 0 | 0 | 0 | `ok` | `ok` |

#### Query Latencies

| Checkpoint | Query | ms | Rows | Status | Indexes | Full scans |
| --- | --- | ---: | ---: | --- | --- | --- |
| `before` | `entity_name_lookup` | 0 | 4 | `ok` | `idx_entities_name` | `SCAN entities` |
| `before` | `entity_qname_lookup` | 0 | 1 | `ok` | `idx_entities_qname` | `SCAN entities` |
| `before` | `edge_head_relation_lookup` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `edge_tail_relation_lookup` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `edge_span_path_lookup` | 0 | 64 | `ok` | `idx_edges_span_path` | `SCAN edges` |
| `before` | `relation_count_scan` | 0 | 27 | `ok` | `idx_edges_tail_relation` | `SCAN edges USING COVERING INDEX idx_edges_tail_relation` |
| `before` | `text_query_fts` | 0 | 0 | `ok` | none | `SCAN stage0_fts VIRTUAL TABLE INDEX 0:M6` |
| `before` | `relation_query_calls` | 0 | 20 | `ok` | `sqlite_autoindex_relation_kind_dict_1` | `SCAN e` |
| `before` | `context_pack_outbound` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `impact_inbound` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `unresolved_calls_paginated` | 0 | 19 | `ok` | `sqlite_autoindex_relation_kind_dict_1`, `id_key=?`, `rowid=?` | `SCAN e` |
| `after_disable_fts_snippets_simulation` | `entity_name_lookup` | 0 | 4 | `ok` | `idx_entities_name` | `SCAN entities` |
| `after_disable_fts_snippets_simulation` | `entity_qname_lookup` | 0 | 1 | `ok` | `idx_entities_qname` | `SCAN entities` |
| `after_disable_fts_snippets_simulation` | `edge_head_relation_lookup` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `after_disable_fts_snippets_simulation` | `edge_tail_relation_lookup` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `after_disable_fts_snippets_simulation` | `edge_span_path_lookup` | 0 | 64 | `ok` | `idx_edges_span_path` | `SCAN edges` |
| `after_disable_fts_snippets_simulation` | `relation_count_scan` | 0 | 27 | `ok` | `idx_edges_tail_relation` | `SCAN edges USING COVERING INDEX idx_edges_tail_relation` |
| `after_disable_fts_snippets_simulation` | `text_query_fts` | 0 | 0 | `ok` | none | `SCAN stage0_fts VIRTUAL TABLE INDEX 0:M6` |
| `after_disable_fts_snippets_simulation` | `relation_query_calls` | 0 | 20 | `ok` | `sqlite_autoindex_relation_kind_dict_1` | `SCAN e` |
| `after_disable_fts_snippets_simulation` | `context_pack_outbound` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `after_disable_fts_snippets_simulation` | `impact_inbound` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `after_disable_fts_snippets_simulation` | `unresolved_calls_paginated` | 0 | 19 | `ok` | `sqlite_autoindex_relation_kind_dict_1`, `id_key=?`, `rowid=?` | `SCAN e` |

#### Degradation Flags

No query degradation crossed the audit threshold.

### `bulk_load_secondary_indexes`

Copied DB: `<REPO_ROOT>/Desktop/development/codegraph-mcp/reports/final/artifacts/run_20260510_232527/query_latency_work/run-1778473556610/bulk_load_secondary_indexes/codegraph.sqlite`

Copy removed: `true`

Mutations: `DROP all secondary non-auto indexes`, `VACUUM`, `recreate secondary indexes`, `ANALYZE`

Recommendation: `recommended_for_next_safe_trial` (`recommended=true`)

Reason: Final copied-DB checkpoint preserved measured core/context query behavior; run the semantic gate before applying this to production artifacts.

| DB bytes before | DB bytes after | Delta bytes | Delta percent | Graph Truth status | Context packet status |
| ---: | ---: | ---: | ---: | --- | --- |
| 663552 | 618496 | -45056 | -6.79 | `not_run` | `queried` |

Notes:

- Simulates the final storage shape after bulk loading data first and recreating secondary indexes after insertion.
- This does not measure insertion throughput directly; it measures final size/query impact after the rebuild.

| Checkpoint | DB bytes | WAL bytes | Edge index bytes | Dict table bytes | Unique text index bytes | FTS bytes | Source span bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `before` | 630784 | 0 | 57344 | 114688 | 110592 | 28672 | 8192 |
| `after_drop_secondary_indexes` | 524288 | 0 | 0 | 114688 | 110592 | 28672 | 4096 |
| `after_recreate_secondary_indexes` | 618496 | 0 | 57344 | 114688 | 110592 | 28672 | 8192 |

#### Core Query Delta

| Query | Before ms | After ms | Delta ms | Before status | After status |
| --- | ---: | ---: | ---: | --- | --- |
| `entity_name_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `entity_qname_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `edge_head_relation_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `edge_tail_relation_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `edge_span_path_lookup` | 0 | 0 | 0 | `ok` | `ok` |
| `relation_count_scan` | 0 | 0 | 0 | `ok` | `ok` |
| `text_query_fts` | 0 | 0 | 0 | `ok` | `ok` |
| `relation_query_calls` | 0 | 0 | 0 | `ok` | `ok` |
| `context_pack_outbound` | 0 | 0 | 0 | `ok` | `ok` |
| `impact_inbound` | 0 | 0 | 0 | `ok` | `ok` |
| `unresolved_calls_paginated` | 0 | 0 | 0 | `ok` | `ok` |

#### Query Latencies

| Checkpoint | Query | ms | Rows | Status | Indexes | Full scans |
| --- | --- | ---: | ---: | --- | --- | --- |
| `before` | `entity_name_lookup` | 0 | 4 | `ok` | `idx_entities_name` | `SCAN entities` |
| `before` | `entity_qname_lookup` | 0 | 1 | `ok` | `idx_entities_qname` | `SCAN entities` |
| `before` | `edge_head_relation_lookup` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `edge_tail_relation_lookup` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `edge_span_path_lookup` | 0 | 64 | `ok` | `idx_edges_span_path` | `SCAN edges` |
| `before` | `relation_count_scan` | 0 | 27 | `ok` | `idx_edges_tail_relation` | `SCAN edges USING COVERING INDEX idx_edges_tail_relation` |
| `before` | `text_query_fts` | 0 | 0 | `ok` | none | `SCAN stage0_fts VIRTUAL TABLE INDEX 0:M6` |
| `before` | `relation_query_calls` | 0 | 20 | `ok` | `sqlite_autoindex_relation_kind_dict_1` | `SCAN e` |
| `before` | `context_pack_outbound` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `impact_inbound` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `before` | `unresolved_calls_paginated` | 0 | 19 | `ok` | `sqlite_autoindex_relation_kind_dict_1`, `id_key=?`, `rowid=?` | `SCAN e` |
| `after_drop_secondary_indexes` | `entity_name_lookup` | 0 | 4 | `ok` | none | `SCAN entities`, `SCAN entities` |
| `after_drop_secondary_indexes` | `entity_qname_lookup` | 0 | 1 | `ok` | none | `SCAN entities`, `SCAN entities` |
| `after_drop_secondary_indexes` | `edge_head_relation_lookup` | 0 | 1 | `ok` | none | `SCAN edges`, `SCAN edges`, `SCAN edges` |
| `after_drop_secondary_indexes` | `edge_tail_relation_lookup` | 0 | 10 | `ok` | none | `SCAN edges`, `SCAN edges`, `SCAN edges` |
| `after_drop_secondary_indexes` | `edge_span_path_lookup` | 0 | 64 | `ok` | none | `SCAN edges`, `SCAN edges` |
| `after_drop_secondary_indexes` | `relation_count_scan` | 0 | 27 | `ok` | none | `SCAN edges` |
| `after_drop_secondary_indexes` | `text_query_fts` | 0 | 0 | `ok` | none | `SCAN stage0_fts VIRTUAL TABLE INDEX 0:M6` |
| `after_drop_secondary_indexes` | `relation_query_calls` | 0 | 20 | `ok` | `sqlite_autoindex_relation_kind_dict_1` | `SCAN e` |
| `after_drop_secondary_indexes` | `context_pack_outbound` | 0 | 1 | `ok` | none | `SCAN e`, `SCAN edges`, `SCAN edges` |
| `after_drop_secondary_indexes` | `impact_inbound` | 0 | 10 | `ok` | none | `SCAN e`, `SCAN edges`, `SCAN edges` |
| `after_drop_secondary_indexes` | `unresolved_calls_paginated` | 0 | 19 | `ok` | `sqlite_autoindex_relation_kind_dict_1`, `id_key=?`, `rowid=?` | `SCAN e` |
| `after_recreate_secondary_indexes` | `entity_name_lookup` | 0 | 4 | `ok` | `idx_entities_name` | `SCAN entities` |
| `after_recreate_secondary_indexes` | `entity_qname_lookup` | 0 | 1 | `ok` | `idx_entities_qname` | `SCAN entities` |
| `after_recreate_secondary_indexes` | `edge_head_relation_lookup` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `after_recreate_secondary_indexes` | `edge_tail_relation_lookup` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `after_recreate_secondary_indexes` | `edge_span_path_lookup` | 0 | 64 | `ok` | `idx_edges_span_path` | `SCAN edges` |
| `after_recreate_secondary_indexes` | `relation_count_scan` | 0 | 27 | `ok` | `idx_edges_tail_relation` | `SCAN edges USING COVERING INDEX idx_edges_tail_relation` |
| `after_recreate_secondary_indexes` | `text_query_fts` | 0 | 0 | `ok` | none | `SCAN stage0_fts VIRTUAL TABLE INDEX 0:M6` |
| `after_recreate_secondary_indexes` | `relation_query_calls` | 0 | 20 | `ok` | `sqlite_autoindex_relation_kind_dict_1` | `SCAN e` |
| `after_recreate_secondary_indexes` | `context_pack_outbound` | 0 | 1 | `ok` | `idx_edges_head_relation` | `SCAN edges`, `SCAN edges` |
| `after_recreate_secondary_indexes` | `impact_inbound` | 0 | 10 | `ok` | `idx_edges_tail_relation` | `SCAN edges`, `SCAN edges` |
| `after_recreate_secondary_indexes` | `unresolved_calls_paginated` | 0 | 19 | `ok` | `sqlite_autoindex_relation_kind_dict_1`, `id_key=?`, `rowid=?` | `SCAN e` |

#### Degradation Flags

No query degradation crossed the audit threshold.

## Notes

- Every experiment is run against a copied SQLite DB; the original path is opened read-only or copied from disk only.
- Index-removal experiments are measurement-only and must not be translated into production schema changes without graph-truth and query-plan review.
- Graph Truth is marked not applicable here because graph-truth cases reindex fixture repositories instead of consuming an already-copied benchmark DB.
