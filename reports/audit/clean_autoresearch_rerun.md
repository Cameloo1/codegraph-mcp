# Clean Autoresearch Rerun

Generated: 2026-05-11 04:48:52 -05:00

This rerun was started only after the local gates passed:

| Gate | Result | Evidence |
| --- | --- | --- |
| DB integrity remediation | pass | `reports/audit/db_integrity_remediation.md` and update harness evidence |
| Autoresearch repeat/update failure fix | pass | `reports/audit/autoresearch_update_repro_fix.md` |
| Graph Truth Gate | pass | 11/11 fixtures, 0 forbidden edges, 0 forbidden paths |
| Context Packet Gate | pass | 11/11 fixtures, 100% critical symbol recall, 100% proof-path coverage |

## Verdict

Final clean rerun verdict: fail.

The failure is now performance and storage, not DB durability or graph-truth correctness. The clean Autoresearch DB passed integrity checks after cold index, repeat unchanged index, and single-file update. Repeat unchanged no longer fails on duplicate edge insertion, and the update harness did not create a malformed DB.

## Target Results

| Target | Status | Observed |
| --- | --- | --- |
| Cold index <= 60s | fail | 442,145 ms profile time; 676.4 s shell wall time |
| Repeat unchanged <= 5s | fail | 13,571 ms profile time; 0 files read/hashed/parsed |
| Single-file update <= 750 ms p95 | fail | 80,207 ms update, 73,249 ms restore |
| DB family <= 250 MiB | fail | 1,712,730,112 bytes, about 1,633.39 MiB |
| `context_pack` p95 <= 2s | fail | 30,315 ms |
| unresolved-calls page p95 <= 1s | pass | 68 ms |
| DB integrity clean | pass | `integrity_check` ok after cold/repeat/update |
| Duplicate insertion failure absent | pass | repeat/update `duplicate_edges_upserted=0` |

## Run Details

| Step | Status | Notes |
| --- | --- | --- |
| Cold index Autoresearch | completed | 4 workers, 8,561 files walked, 4,975 read/hashed/parsed |
| Integrity check | passed | storage audit reported `integrity_check: ok` |
| Storage audit | completed | DB family 1,712,730,112 bytes; 442.97 database bytes per edge |
| Repeat unchanged index | completed | 8,561 files walked, 4,975 metadata-unchanged, 0 read/hash/parse |
| Integrity check after repeat | passed | repeat storage audit reported clean integrity |
| Single-file update | completed | one Python file read/hashed/parsed; transaction committed |
| Integrity check after update | passed | update harness reported `integrity_status: ok` |
| `context_pack` p95 | measured | 5 iterations, p95 30,315 ms |
| unresolved-calls p95 | measured | 20 pages, p95 68 ms |
| Relation/source-span sampler | completed | CALLS/READS/WRITES/FLOWS_TO sampled; 20 stored PathEvidence rows sampled |

## Correctness Preconditions

Graph Truth Gate:

- Cases: 11/11 passed.
- Expected entities: 29/29.
- Expected edges: 20/20.
- Expected paths: 11/11.
- Forbidden edges found: 0.
- Forbidden paths found: 0.
- Source-span failures: 0.

Context Packet Gate:

- Cases: 11/11 passed.
- Critical symbol recall: 100%.
- Proof-path coverage: 100%.
- Proof-path source-span coverage: 100%.
- Expected tests recall: 100%.
- Distractor ratio: 0%.

## Indexing Metrics

Cold index:

| Metric | Value |
| --- | ---: |
| Files walked | 8,561 |
| Files read | 4,975 |
| Files hashed | 4,975 |
| Files parsed | 4,975 |
| Files skipped | 3,586 |
| Entities | 1,630,146 |
| Edges | 3,866,431 |
| Source spans | 5,496,577 |
| Duplicate edges upserted | 442 |
| Graph fact hash | `fnv64:bc69dbacfcb9987e` |

Repeat unchanged:

| Metric | Value |
| --- | ---: |
| Files walked | 8,561 |
| Metadata unchanged | 4,975 |
| Files read | 0 |
| Files hashed | 0 |
| Files parsed | 0 |
| Entities inserted | 0 |
| Edges inserted | 0 |
| Duplicate edges upserted | 0 |
| Graph fact hash | `fnv64:bc69dbacfcb9987e` |

Single-file update:

| Metric | Value |
| --- | ---: |
| Mutation file | `python/autoresearch_utils/metrics_tools.py` |
| Files walked | 1 |
| Files read | 1 |
| Files hashed | 1 |
| Files parsed | 1 |
| Entities inserted | 18 |
| Edges inserted | 35 |
| Duplicate edges upserted | 0 |
| Transaction status | committed |
| Integrity status | ok |
| Updated graph fact hash | `fnv64:b3e3f999e13b467f` |
| Restored graph fact hash | `fnv64:bc69dbacfcb9987e` |

## Storage

| Metric | Value |
| --- | ---: |
| DB bytes | 1,712,730,112 |
| WAL bytes at storage audit | 0 |
| Edge count | 3,866,431 |
| Average DB bytes per edge | 442.97 |
| PathEvidence rows | 4,096 |
| Source snippet storage | snippets not stored redundantly; source loaded from files |

Top storage contributors:

| Object | Type | Bytes |
| --- | --- | ---: |
| `edges` | table | 573,857,792 |
| `sqlite_autoindex_qualified_name_dict_1` | index | 218,021,888 |
| `qualified_name_dict` | table | 205,287,424 |
| `entities` | table | 117,542,912 |
| `sqlite_autoindex_object_id_dict_1` | index | 91,865,088 |
| `object_id_dict` | table | 82,673,664 |
| `idx_edges_tail_relation` | index | 73,654,272 |
| `idx_edges_head_relation` | index | 73,617,408 |
| `idx_edges_span_path` | index | 62,033,920 |
| `sqlite_autoindex_qname_prefix_dict_1` | index | 46,792,704 |

## Query Latency

| Query | Iterations | P95 | Target | Status |
| --- | ---: | ---: | ---: | --- |
| `context_pack` | 5 | 30,315 ms | 2,000 ms | fail |
| unresolved-calls paginated | 20 | 68 ms | 1,000 ms | pass |

`context_pack` is the highest-priority large-repo performance bug from this rerun. It is no longer blocked by missing PathEvidence, but it is still far too slow on the clean Autoresearch DB.

## Relation And Path Sampling

Relation counts completed on the clean DB.

| Relation | Count |
| --- | ---: |
| `CONTAINS` | 907,236 |
| `DEFINED_IN` | 897,965 |
| `CALLEE` | 524,760 |
| `CALLS` | 516,392 |
| `ARGUMENT_0` | 332,104 |
| `DECLARES` | 214,406 |
| `FLOWS_TO` | 163,481 |
| `ARGUMENT_1` | 73,913 |
| `DEFINES` | 72,897 |
| `IMPORTS` | 72,626 |

Samples written:

- `reports/audit/artifacts/clean_autoresearch_sample_CALLS.md/json`
- `reports/audit/artifacts/clean_autoresearch_sample_READS.md/json`
- `reports/audit/artifacts/clean_autoresearch_sample_WRITES.md/json`
- `reports/audit/artifacts/clean_autoresearch_sample_FLOWS_TO.md/json`
- `reports/audit/artifacts/clean_autoresearch_sample_PathEvidence.md/json`

PathEvidence sampling found 4,096 stored PathEvidence rows and sampled 20. The sampler did not need generated fallback paths.

## Artifacts

- Graph truth: `reports/audit/artifacts/clean_autoresearch_pre_graph_truth.md/json`
- Context packet gate: `reports/audit/artifacts/clean_autoresearch_pre_context_packet.md/json`
- Clean DB: `reports/audit/artifacts/clean_autoresearch_rerun.sqlite`
- Storage audit: `reports/audit/artifacts/clean_autoresearch_storage.md/json`
- Repeat storage audit: `reports/audit/artifacts/clean_autoresearch_storage_after_repeat.md/json`
- Update integrity: `reports/audit/artifacts/clean_autoresearch_update_integrity.md/json`
- Query latency: `reports/audit/artifacts/clean_autoresearch_query_latency.json`
- Relation counts: `reports/audit/artifacts/clean_autoresearch_relation_counts.md/json`

## Next Required Work

1. Profile cold index DB write time; current profile attributes most cold time to DB/write/FTS-index work.
2. Reduce large-repo `context_pack` p95 from about 30s to <=2s without weakening proof-path or source-span semantics.
3. Optimize incremental single-file update transaction cost; correctness and integrity are clean, but 80s is not usable.
4. Resume storage optimization from clean evidence only; the DB is about 1.63 GiB, far above the 250 MiB intended target.
