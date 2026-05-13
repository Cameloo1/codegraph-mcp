# Default Query Surface Audit

Source of truth: `MVP.md`.

- Status: **passed**
- DB: `<REPO_ROOT>/Desktop\development\codegraph-mcp\reports\final\artifacts\comprehensive_proof_proof_build_fix_20260513.sqlite`
- Iterations: `20`

| Query | Status | p50 ms | p95 ms | p99 ms | rows | target p95 |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `entity_name_lookup` | `pass` | 1.006 | 1.781 | 1.781 | 20 | 250.000 |
| `symbol_lookup` | `pass` | 118.420 | 125.540 | 125.540 | 20 | 250.000 |
| `qname_lookup` | `pass` | 10.797 | 11.166 | 11.166 | 1 | 250.000 |
| `text_fts_query` | `pass` | 0.025 | 0.238 | 0.238 | 4 | 500.000 |
| `relation_query_calls` | `pass` | 149.162 | 175.895 | 175.895 | 50 | 500.000 |
| `relation_query_reads_writes` | `pass` | 148.847 | 155.593 | 155.593 | 50 | 500.000 |
| `path_evidence_lookup` | `pass` | 1.430 | 2.292 | 2.292 | 50 | 500.000 |
| `source_snippet_batch_load` | `pass` | 29.885 | 32.675 | 32.675 | 50 | 500.000 |
| `context_pack_normal` | `pass` | 76.495 | 79.331 | 79.331 | 1 | 2000 |
| `unresolved_calls_paginated` | `pass` | 50.259 | 56.198 | 56.198 | 0 | 1000 |

## Query Details

### `entity_name_lookup`

Command: `codegraph-mcp query symbols <entity-name>`

Status: `pass`

SQL:

```sql
WITH wanted_name AS (SELECT id FROM symbol_dict WHERE value = 'login' LIMIT 1) SELECT 'repo://e/' || lower(hex(e.entity_hash)) AS id, name.value AS name, qname.value AS qualified_name, path.value AS repo_relative_path FROM wanted_name JOIN entities e ON e.name_id = wanted_name.id JOIN symbol_dict name ON name.id = e.name_id JOIN qualified_name_lookup qname ON qname.id = e.qualified_name_id JOIN path_dict path ON path.id = e.path_id ORDER BY qname.value, e.entity_hash LIMIT 20
```

EXPLAIN QUERY PLAN:

| id | parent | detail |
| ---: | ---: | --- |
| 2 | 0 | `CO-ROUTINE wanted_name` |
| 5 | 2 | `SCAN symbol_dict` |
| 23 | 0 | `SCAN wanted_name` |
| 26 | 0 | `SEARCH name USING INTEGER PRIMARY KEY (rowid=?)` |
| 32 | 0 | `SEARCH e USING INDEX idx_entities_name (name_id=?)` |
| 43 | 0 | `SEARCH qname USING INTEGER PRIMARY KEY (rowid=?)` |
| 46 | 0 | `SEARCH prefix USING INTEGER PRIMARY KEY (rowid=?)` |
| 49 | 0 | `SEARCH suffix USING INTEGER PRIMARY KEY (rowid=?)` |
| 52 | 0 | `SEARCH path USING INTEGER PRIMARY KEY (rowid=?)` |
| 88 | 0 | `USE TEMP B-TREE FOR ORDER BY` |

### `symbol_lookup`

Command: `codegraph-mcp query symbols <symbol>`

Status: `pass`

SQL:

```sql
WITH candidate_keys AS ( SELECT e.id_key FROM symbol_dict wanted JOIN entities e ON e.name_id = wanted.id WHERE wanted.value = 'login' UNION SELECT e.id_key FROM qualified_name_lookup wanted_qname JOIN entities e ON e.qualified_name_id = wanted_qname.id WHERE wanted_qname.value = 'login' ) SELECT 'repo://e/' || lower(hex(e.entity_hash)) AS id, name.value AS name, qname.value AS qualified_name, path.value AS repo_relative_path FROM candidate_keys JOIN entities e ON e.id_key = candidate_keys.id_key JOIN symbol_dict name ON name.id = e.name_id JOIN qualified_name_lookup qname ON qname.id = e.qualified_name_id JOIN path_dict path ON path.id = e.path_id ORDER BY qname.value, e.entity_hash LIMIT 20
```

EXPLAIN QUERY PLAN:

| id | parent | detail |
| ---: | ---: | --- |
| 2 | 0 | `CO-ROUTINE candidate_keys` |
| 3 | 2 | `COMPOUND QUERY` |
| 4 | 3 | `LEFT-MOST SUBQUERY` |
| 8 | 4 | `SCAN wanted` |
| 12 | 4 | `SEARCH e USING COVERING INDEX idx_entities_name (name_id=?)` |
| 21 | 3 | `UNION USING TEMP B-TREE` |
| 26 | 21 | `SCAN e USING COVERING INDEX idx_entities_qname` |
| 28 | 21 | `SEARCH qname USING INTEGER PRIMARY KEY (rowid=?)` |
| 31 | 21 | `SEARCH prefix USING INTEGER PRIMARY KEY (rowid=?)` |
| 34 | 21 | `SEARCH suffix USING INTEGER PRIMARY KEY (rowid=?)` |
| 73 | 0 | `SCAN e USING INDEX idx_entities_qname` |
| 77 | 0 | `SEARCH qname USING INTEGER PRIMARY KEY (rowid=?)` |
| 80 | 0 | `SEARCH suffix USING INTEGER PRIMARY KEY (rowid=?)` |
| 83 | 0 | `SEARCH name USING INTEGER PRIMARY KEY (rowid=?)` |
| 86 | 0 | `SEARCH prefix USING INTEGER PRIMARY KEY (rowid=?)` |
| 89 | 0 | `SEARCH path USING INTEGER PRIMARY KEY (rowid=?)` |
| 94 | 0 | `BLOOM FILTER ON candidate_keys (id_key=?)` |
| 105 | 0 | `SEARCH candidate_keys USING AUTOMATIC COVERING INDEX (id_key=?)` |
| 144 | 0 | `USE TEMP B-TREE FOR ORDER BY` |

### `qname_lookup`

Command: `codegraph-mcp query symbols <qualified-name>`

Status: `pass`

SQL:

```sql
WITH wanted_qname AS (SELECT id, value FROM qualified_name_lookup WHERE value = 'runtime::arl-codex::codex-rs::app-server::tests::suite::v2::account.set_auth_token_cancels_active_chatgpt_login.login' LIMIT 1) SELECT 'repo://e/' || lower(hex(e.entity_hash)) AS id, name.value AS name, wanted_qname.value AS qualified_name, path.value AS repo_relative_path FROM wanted_qname JOIN entities e ON e.qualified_name_id = wanted_qname.id JOIN symbol_dict name ON name.id = e.name_id JOIN path_dict path ON path.id = e.path_id ORDER BY e.entity_hash LIMIT 20
```

EXPLAIN QUERY PLAN:

| id | parent | detail |
| ---: | ---: | --- |
| 2 | 0 | `CO-ROUTINE wanted_qname` |
| 7 | 2 | `SCAN qname` |
| 9 | 2 | `SEARCH prefix USING INTEGER PRIMARY KEY (rowid=?)` |
| 12 | 2 | `SEARCH suffix USING INTEGER PRIMARY KEY (rowid=?)` |
| 60 | 0 | `SCAN wanted_qname` |
| 63 | 0 | `SEARCH e USING INDEX idx_entities_qname (qualified_name_id=?)` |
| 71 | 0 | `SEARCH name USING INTEGER PRIMARY KEY (rowid=?)` |
| 74 | 0 | `SEARCH path USING INTEGER PRIMARY KEY (rowid=?)` |
| 94 | 0 | `USE TEMP B-TREE FOR ORDER BY` |

### `text_fts_query`

Command: `codegraph-mcp query text <query>`

Status: `pass`

SQL:

```sql
SELECT kind, id, repo_relative_path, line, title, bm25(stage0_fts) AS rank FROM stage0_fts WHERE stage0_fts MATCH '"scripts"' ORDER BY rank, kind, id LIMIT 20
```

EXPLAIN QUERY PLAN:

| id | parent | detail |
| ---: | ---: | --- |
| 4 | 0 | `SCAN stage0_fts VIRTUAL TABLE INDEX 0:M6` |
| 24 | 0 | `USE TEMP B-TREE FOR ORDER BY` |

### `relation_query_calls`

Command: `codegraph-mcp query callers/callees <symbol>`

Status: `pass`

SQL:

```sql
SELECT e.id_key, head.value AS head_id, tail.value AS tail_id, relation.value AS relation, span_path.value AS source_span_path, e.start_line, e.end_line FROM edges_compat e JOIN relation_kind_dict relation ON relation.id = e.relation_id JOIN object_id_lookup head ON head.id = e.head_id_key JOIN object_id_lookup tail ON tail.id = e.tail_id_key JOIN path_dict span_path ON span_path.id = e.span_path_id WHERE relation.value = 'CALLS' ORDER BY e.id_key LIMIT 50
```

EXPLAIN QUERY PLAN:

| id | parent | detail |
| ---: | ---: | --- |
| 3 | 0 | `MATERIALIZE object_id_lookup` |
| 5 | 3 | `COMPOUND QUERY` |
| 6 | 5 | `LEFT-MOST SUBQUERY` |
| 10 | 6 | `SCAN e USING INDEX idx_entities_qname` |
| 14 | 6 | `SEARCH debug USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN` |
| 33 | 5 | `UNION ALL` |
| 35 | 33 | `SCAN debug` |
| 38 | 33 | `CORRELATED SCALAR SUBQUERY 7` |
| 42 | 38 | `SEARCH e USING PRIMARY KEY (id_key=?)` |
| 56 | 5 | `UNION ALL` |
| 58 | 56 | `SCAN history` |
| 61 | 56 | `CORRELATED SCALAR SUBQUERY 4` |
| 65 | 61 | `SEARCH e USING PRIMARY KEY (id_key=?)` |
| 75 | 56 | `CORRELATED SCALAR SUBQUERY 5` |
| 79 | 75 | `SEARCH debug USING INTEGER PRIMARY KEY (rowid=?)` |
| 103 | 0 | `SCAN e` |
| 105 | 0 | `SEARCH span_path USING INTEGER PRIMARY KEY (rowid=?)` |
| 108 | 0 | `SEARCH relation USING INTEGER PRIMARY KEY (rowid=?)` |
| 117 | 0 | `BLOOM FILTER ON head (id=?)` |
| 127 | 0 | `SEARCH head USING AUTOMATIC COVERING INDEX (id=?)` |
| 135 | 0 | `BLOOM FILTER ON tail (id=?)` |
| 145 | 0 | `SEARCH tail USING AUTOMATIC COVERING INDEX (id=?)` |

### `relation_query_reads_writes`

Command: `codegraph-mcp query impact/proof relation READS|WRITES`

Status: `pass`

SQL:

```sql
SELECT e.id_key, head.value AS head_id, tail.value AS tail_id, relation.value AS relation, span_path.value AS source_span_path, e.start_line, e.end_line FROM edges_compat e JOIN relation_kind_dict relation ON relation.id = e.relation_id JOIN object_id_lookup head ON head.id = e.head_id_key JOIN object_id_lookup tail ON tail.id = e.tail_id_key JOIN path_dict span_path ON span_path.id = e.span_path_id WHERE relation.value IN ('READS', 'WRITES') ORDER BY e.id_key LIMIT 50
```

EXPLAIN QUERY PLAN:

| id | parent | detail |
| ---: | ---: | --- |
| 3 | 0 | `MATERIALIZE object_id_lookup` |
| 5 | 3 | `COMPOUND QUERY` |
| 6 | 5 | `LEFT-MOST SUBQUERY` |
| 10 | 6 | `SCAN e USING INDEX idx_entities_qname` |
| 14 | 6 | `SEARCH debug USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN` |
| 33 | 5 | `UNION ALL` |
| 35 | 33 | `SCAN debug` |
| 38 | 33 | `CORRELATED SCALAR SUBQUERY 7` |
| 42 | 38 | `SEARCH e USING PRIMARY KEY (id_key=?)` |
| 56 | 5 | `UNION ALL` |
| 58 | 56 | `SCAN history` |
| 61 | 56 | `CORRELATED SCALAR SUBQUERY 4` |
| 65 | 61 | `SEARCH e USING PRIMARY KEY (id_key=?)` |
| 75 | 56 | `CORRELATED SCALAR SUBQUERY 5` |
| 79 | 75 | `SEARCH debug USING INTEGER PRIMARY KEY (rowid=?)` |
| 103 | 0 | `SCAN e` |
| 105 | 0 | `SEARCH span_path USING INTEGER PRIMARY KEY (rowid=?)` |
| 108 | 0 | `SEARCH relation USING INTEGER PRIMARY KEY (rowid=?)` |
| 118 | 0 | `BLOOM FILTER ON head (id=?)` |
| 128 | 0 | `SEARCH head USING AUTOMATIC COVERING INDEX (id=?)` |
| 136 | 0 | `BLOOM FILTER ON tail (id=?)` |
| 146 | 0 | `SEARCH tail USING AUTOMATIC COVERING INDEX (id=?)` |

### `path_evidence_lookup`

Command: `context_pack stored PathEvidence lookup`

Status: `pass`

SQL:

```sql
SELECT l.path_id, p.source, p.target, l.relation_signature, l.length, l.confidence FROM path_evidence_lookup l JOIN path_evidence p ON p.id = l.path_id ORDER BY l.confidence DESC, l.length ASC, l.path_id LIMIT 50
```

EXPLAIN QUERY PLAN:

| id | parent | detail |
| ---: | ---: | --- |
| 7 | 0 | `SCAN l USING INDEX sqlite_autoindex_path_evidence_lookup_1` |
| 10 | 0 | `SEARCH p USING INDEX sqlite_autoindex_path_evidence_1 (id=?)` |
| 32 | 0 | `USE TEMP B-TREE FOR ORDER BY` |

### `source_snippet_batch_load`

Command: `context_pack source snippet batch load`

Status: `pass`

SQL:

```sql
SELECT DISTINCT repo_relative_path, start_line, start_column, end_line, end_column FROM ( SELECT path.value AS repo_relative_path, e.start_line, e.start_column, e.end_line, e.end_column FROM edges_compat e JOIN path_dict path ON path.id = e.span_path_id WHERE e.start_line > 0 UNION ALL SELECT path.value AS repo_relative_path, s.start_line, s.start_column, s.end_line, s.end_column FROM source_spans s JOIN path_dict path ON path.id = s.path_id WHERE s.start_line > 0 ) ORDER BY repo_relative_path, start_line, end_line LIMIT 50
```

EXPLAIN QUERY PLAN:

| id | parent | detail |
| ---: | ---: | --- |
| 2 | 0 | `CO-ROUTINE (subquery-2)` |
| 3 | 2 | `COMPOUND QUERY` |
| 4 | 3 | `LEFT-MOST SUBQUERY` |
| 7 | 4 | `SCAN e` |
| 11 | 4 | `SEARCH path USING INTEGER PRIMARY KEY (rowid=?)` |
| 21 | 3 | `UNION ALL` |
| 24 | 21 | `SCAN s` |
| 28 | 21 | `SEARCH path USING INTEGER PRIMARY KEY (rowid=?)` |
| 42 | 0 | `SCAN (subquery-2)` |
| 64 | 0 | `USE TEMP B-TREE FOR DISTINCT` |
| 65 | 0 | `USE TEMP B-TREE FOR ORDER BY` |

### `context_pack_normal`

Command: `codegraph-mcp context-pack --mode normal`

Status: `pass`

SQL:

```sql
context_pack normal stored-PathEvidence query path
```

EXPLAIN QUERY PLAN:

| id | parent | detail |
| ---: | ---: | --- |
| unknown | unknown | `` |
| unknown | unknown | `` |

### `unresolved_calls_paginated`

Command: `codegraph-mcp query unresolved-calls --limit 20`

Status: `pass`

SQL:

```sql
SELECT e.edge_id AS edge_id, e.head_id AS head_id, e.relation AS relation, e.tail_id AS tail_id, e.source_span_path AS span_repo_relative_path, e.start_line, e.start_column, e.end_line, e.end_column, e.repo_commit, e.file_hash AS file_hash, e.extractor AS extractor, e.confidence, e.exactness AS exactness, e.derived, e.provenance_edges_json, e.metadata_json AS edge_metadata_json, COALESCE(head_kind.value, head_static.kind, 'Function') AS head_kind, COALESCE(head_name.value, head_static.name, e.head_id) AS head_name, COALESCE(head_qname.value, head_static.qualified_name, e.head_id) AS head_qualified_name, COALESCE(head_path.value, head_static.repo_relative_path, e.source_span_path) AS head_repo_relative_path, COALESCE(head_span_path.value, head_static.source_span_path) AS head_span_repo_relative_path, head.start_line AS head_start_line, head.start_column AS head_start_column, head.end_line AS head_end_line, head.end_column AS head_end_column, COALESCE(head_extractor.value, head_static.created_from, 'heuristic_sidecar') AS head_created_from, COALESCE(head.confidence, head_static.confidence, e.confidence) AS head_confidence, COALESCE(head.metadata_json, head_static.metadata_json, '{}') AS head_metadata_json, COALESCE(tail_kind.value, tail_static.kind, 'Function') AS tail_kind, COALESCE(tail_name.value, tail_static.name, e.tail_id) AS tail_name, COALESCE(tail_qname.value, tail_static.qualified_name, e.tail_id) AS tail_qualified_name, COALESCE(tail_path.value, tail_static.repo_relative_path, e.source_span_path) AS tail_repo_relative_path, COALESCE(tail_span_path.value, tail_static.source_span_path) AS tail_span_repo_relative_path, COALESCE(tail.start_line, tail_static.start_line) AS tail_start_line, COALESCE(tail.start_column, tail_static.start_column) AS tail_start_column, COALESCE(tail.end_line, tail_static.end_line) AS tail_end_line, COALESCE(tail.end_column, tail_static.end_column) AS tail_end_column, COALESCE(tail_extractor.value, tail_static.created_from, 'heuristic_sidecar') AS tail_created_from, COALESCE(tail.confidence, tail_static.confidence, e.confidence) AS tail_confidence, COALESCE(tail.metadata_json, tail_static.metadata_json, '{}') AS tail_metadata_json FROM heuristic_edges e LEFT JOIN object_id_lookup head_oid ON head_oid.value = e.head_id LEFT JOIN object_id_lookup tail_oid ON tail_oid.value = e.tail_id LEFT JOIN entities head ON head.id_key = head_oid.id LEFT JOIN entity_kind_dict head_kind ON head_kind.id = head.kind_id LEFT JOIN symbol_dict head_name ON head_name.id = head.name_id LEFT JOIN qualified_name_lookup head_qname ON head_qname.id = head.qualified_name_id LEFT JOIN path_dict head_path ON head_path.id = head.path_id LEFT JOIN path_dict head_span_path ON head_span_path.id = head.span_path_id LEFT JOIN extractor_dict head_extractor ON head_extractor.id = head.created_from_id LEFT JOIN static_references head_static ON head_static.entity_id = e.head_id LEFT JOIN entities tail ON tail.id_key = tail_oid.id LEFT JOIN entity_kind_dict tail_kind ON tail_kind.id = tail.kind_id LEFT JOIN symbol_dict tail_name ON tail_name.id = tail.name_id LEFT JOIN qualified_name_lookup tail_qname ON tail_qname.id = tail.qualified_name_id LEFT JOIN path_dict tail_path ON tail_path.id = tail.path_id LEFT JOIN path_dict tail_span_path ON tail_span_path.id = tail.span_path_id LEFT JOIN extractor_dict tail_extractor ON tail_extractor.id = tail.created_from_id LEFT JOIN static_references tail_static ON tail_static.entity_id = e.tail_id WHERE e.relation = 'CALLS' AND ( e.exactness = 'static_heuristic' OR lower(COALESCE(e.metadata_json, '')) LIKE '%unresolved%' OR lower(COALESCE(tail_static.metadata_json, tail.metadata_json, '')) LIKE '%unresolved%' OR lower(COALESCE(tail_static.created_from, tail_extractor.value, '')) LIKE '%heuristic%' OR lower(COALESCE(tail_static.name, tail_name.value, '')) LIKE '%unknown_callee%' OR COALESCE(tail_static.qualified_name, tail_qname.value, '') LIKE 'static_reference:%' ) ORDER BY e.id_key LIMIT ?1 OFFSET ?2
```

EXPLAIN QUERY PLAN:

| id | parent | detail |
| ---: | ---: | --- |
| 3 | 0 | `MATERIALIZE object_id_lookup` |
| 5 | 3 | `COMPOUND QUERY` |
| 6 | 5 | `LEFT-MOST SUBQUERY` |
| 10 | 6 | `SCAN e USING INDEX idx_entities_qname` |
| 14 | 6 | `SEARCH debug USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN` |
| 33 | 5 | `UNION ALL` |
| 35 | 33 | `SCAN debug` |
| 38 | 33 | `CORRELATED SCALAR SUBQUERY 6` |
| 42 | 38 | `SEARCH e USING PRIMARY KEY (id_key=?)` |
| 56 | 5 | `UNION ALL` |
| 58 | 56 | `SCAN history` |
| 61 | 56 | `CORRELATED SCALAR SUBQUERY 3` |
| 65 | 61 | `SEARCH e USING PRIMARY KEY (id_key=?)` |
| 75 | 56 | `CORRELATED SCALAR SUBQUERY 4` |
| 79 | 75 | `SEARCH debug USING INTEGER PRIMARY KEY (rowid=?)` |
| 100 | 0 | `MATERIALIZE qualified_name_lookup` |
| 105 | 100 | `SCAN qname` |
| 107 | 100 | `SEARCH prefix USING INTEGER PRIMARY KEY (rowid=?)` |
| 110 | 100 | `SEARCH suffix USING INTEGER PRIMARY KEY (rowid=?)` |
| 139 | 0 | `MATERIALIZE qualified_name_lookup` |
| 144 | 139 | `SCAN qname` |
| 146 | 139 | `SEARCH prefix USING INTEGER PRIMARY KEY (rowid=?)` |
| 149 | 139 | `SEARCH suffix USING INTEGER PRIMARY KEY (rowid=?)` |
| 201 | 0 | `SEARCH e USING INDEX idx_heuristic_edges_relation (relation=?)` |
| 211 | 0 | `BLOOM FILTER ON head_oid (value=?)` |
| 221 | 0 | `SEARCH head_oid USING AUTOMATIC COVERING INDEX (value=?) LEFT-JOIN` |
| 230 | 0 | `BLOOM FILTER ON tail_oid (value=?)` |
| 240 | 0 | `SEARCH tail_oid USING AUTOMATIC COVERING INDEX (value=?) LEFT-JOIN` |
| 247 | 0 | `SEARCH head USING PRIMARY KEY (id_key=?) LEFT-JOIN` |
| 255 | 0 | `SEARCH head_kind USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN` |
| 260 | 0 | `SEARCH head_name USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN` |
| 269 | 0 | `BLOOM FILTER ON head_qname (id=?)` |
| 281 | 0 | `SEARCH head_qname USING AUTOMATIC COVERING INDEX (id=?) LEFT-JOIN` |
| 290 | 0 | `SEARCH head_path USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN` |
| 295 | 0 | `SEARCH head_span_path USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN` |
| 300 | 0 | `SEARCH head_extractor USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN` |
| 305 | 0 | `SCAN head_static USING INDEX idx_static_references_path LEFT-JOIN` |
| 314 | 0 | `SEARCH tail USING PRIMARY KEY (id_key=?) LEFT-JOIN` |
| 322 | 0 | `SEARCH tail_kind USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN` |
| 327 | 0 | `SEARCH tail_name USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN` |
| 336 | 0 | `BLOOM FILTER ON tail_qname (id=?)` |
| 348 | 0 | `SEARCH tail_qname USING AUTOMATIC COVERING INDEX (id=?) LEFT-JOIN` |
| 357 | 0 | `SEARCH tail_path USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN` |
| 362 | 0 | `SEARCH tail_span_path USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN` |
| 367 | 0 | `SEARCH tail_extractor USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN` |
| 372 | 0 | `SCAN tail_static USING INDEX idx_static_references_path LEFT-JOIN` |
| 607 | 0 | `USE TEMP B-TREE FOR ORDER BY` |


## Storage Safety Questions

1. Did Graph Truth still pass? Measured by the surrounding gate; this query audit does not change graph truth facts.
2. Did Context Packet quality still pass? Measured by the surrounding gate; context_pack normal is probed here.
3. Did proof DB size decrease? No storage optimization is performed by this audit.
4. Did removed data move to a sidecar, become derivable, or get proven unnecessary? No data is removed by this audit.
