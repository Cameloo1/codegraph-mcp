# Comprehensive Benchmark Latest

Source of truth: `MVP.md`.

Execution mode: latest persisted artifact snapshot. This report does not hide unknowns and does not rerun the long Autoresearch cold build by default.

## Section 1 - Executive Verdict

- Verdict: **fail**
- Reason: failed targets: bytes_per_proof_edge, cold_proof_build_total_wall_ms, proof_db_mib, proof_db_mib_stretch
- Optimization may continue: `true`
- Comparison claims allowed: `false`

### Failed targets

- `bytes_per_proof_edge`
- `cold_proof_build_total_wall_ms`
- `proof_db_mib`
- `proof_db_mib_stretch`

### Passed targets

- `audit_debug_db_bytes`
- `bytes_per_edge_table_plus_index`
- `bytes_per_entity`
- `bytes_per_path_evidence_row`
- `bytes_per_source_span`
- `bytes_per_template_edge`
- `bytes_per_template_entity`
- `cold_proof_build_total_profile_ms`
- `context_cases_passed`
- `context_cases_total`
- `context_pack_normal`
- `critical_symbol_recall`
- `db_write_time_ms`
- `derived_without_provenance_count`
- `distractor_ratio`
- `expected_edges_matched`
- `expected_entities_matched`
- `expected_paths_matched`
- `expected_tests_recall`
- `forbidden_edges_found`
- `forbidden_paths_found`
- `generated_fallback_path_count`
- `graph_truth_cases_passed`
- `graph_truth_cases_total`
- `index_bytes`
- `integrity_check_status`
- `integrity_check_time_ms`
- `proof_db_bytes`
- `proof_path_coverage`
- `proof_path_source_span_coverage`
- `repeat_db_writes_performed`
- `repeat_files_hashed`
- `repeat_files_parsed`
- `repeat_files_read`
- `repeat_files_walked`
- `repeat_graph_fact_hash_changed`
- `repeat_integrity_check_included`
- `repeat_metadata_unchanged`
- `repeat_unchanged_total_ms`
- `repeat_validation_work_integrity_ms`
- `single_file_update_total_ms`
- `source_snippets_not_stored_redundantly`
- `source_span_failures`
- `stale_fact_failures`
- `stored_path_evidence_rows`
- `table_bytes`
- `test_mock_production_leakage_count`
- `total_artifact_bytes`
- `unresolved_calls_paginated`
- `unresolved_exact_count`
- `update_dirty_path_evidence_count`
- `update_file_walk_time_ms`
- `update_global_work_accidentally_triggered`
- `update_graph_hash_update_time_ms`
- `update_hash_time_ms`
- `update_index_maintenance_time_ms`
- `update_integrity_or_quick_check_time_ms`
- `update_integrity_remains_ok`
- `update_parse_time_ms`
- `update_path_evidence_regeneration_time_ms`
- `update_proof_edge_update_time_ms`
- `update_proof_entity_update_time_ms`
- `update_read_time_ms`
- `update_restore_time_ms`
- `update_rows_deleted`
- `update_rows_inserted`
- `update_stale_delete_time_ms`
- `update_transaction_commit_time_ms`
- `wal_bytes`
- `wal_size_bytes`

## Section 2 - Correctness Gates

| Metric | Target | Observed | Status | Notes |
| --- | --- | --- | --- | --- |
| `graph_truth_cases_total` | 11 | 11 | pass | All adversarial graph-truth fixtures must be present. |
| `graph_truth_cases_passed` | all cases | 11 | pass | Graph Truth Gate must be 100% pass. |
| `expected_entities_matched` | 29 | 29 | pass | Every expected entity must be present. |
| `expected_edges_matched` | 20 | 20 | pass | Every required edge must be present. |
| `expected_paths_matched` | 11 | 11 | pass | Every expected proof path must be present. |
| `forbidden_edges_found` | 0 | 0 | pass | Forbidden edge hits must remain zero. |
| `forbidden_paths_found` | 0 | 0 | pass | Forbidden proof paths must remain zero. |
| `source_span_failures` | 0 | 0 | pass | Proof-grade facts must have valid source spans. |
| `unresolved_exact_count` | 0 | 0 | pass | Unresolved relations must not be labeled exact. |
| `derived_without_provenance_count` | 0 | 0 | pass | Derived edges must retain provenance. |
| `test_mock_production_leakage_count` | 0 | 0 | pass | Production proof paths must not include test/mock edges. |
| `stale_fact_failures` | 0 | 0 | pass | Mutation fixtures must not leave stale current facts. |

## Section 3 - Context Packet Gate

| Metric | Target | Observed | Status | Notes |
| --- | --- | --- | --- | --- |
| `context_cases_total` | reported | 11 | pass | Context Packet Gate fixture count. |
| `context_cases_passed` | all cases | 11 | pass | Context Packet Gate must pass all cases. |
| `critical_symbol_recall` | 1.000 | 1.000 | pass | Critical symbol recall must be 100%. |
| `proof_path_coverage` | 1.000 | 1.000 | pass | Proof-path coverage must be 100%. |
| `proof_path_source_span_coverage` | 1.000 | 1.000 | pass | Source-span coverage for proof paths must be 100%. |
| `expected_tests_recall` | 0.900 | 1.000 | pass | Expected test recall target is >=90%. |
| `distractor_ratio` | 0.250 | 0.000 | pass | Distractor ratio target is <=25%. |
| `stored_path_evidence_rows` | reported | 4096 | pass | Stored PathEvidence rows must exist for proof paths. |
| `generated_fallback_path_count` | 0 | 0 | pass | Normal proof cases should use stored PathEvidence instead of generated fallback paths. |

## Section 4 - DB Integrity

| Metric | Target | Observed | Status | Notes |
| --- | --- | --- | --- | --- |
| `integrity_check_status` | ok | ok | pass | Cold proof DB must pass PRAGMA integrity_check. |
| `quick_check_status` | ok | unknown | unknown | Repeat/update quick_check status was not separately persisted in the compact gate. |
| `foreign_key_check_status` | ok or not_applicable | unknown | unknown | Foreign-key check is reported only when the schema/gate persists it. |
| `wal_size_bytes` | reported | 0 | pass | WAL file size is reported separately. |
| `rollback_failure_simulation_status` | rollback cleanly | not persisted in compact proof gate; update harness integrity remained ok | unknown | No explicit failed-update simulation artifact is attached to the compact proof gate; do not infer a pass. |

## Section 5 - Storage Summary

| Metric | Target | Observed | Status | Notes |
| --- | --- | --- | --- | --- |
| `proof_db_bytes` | reported | 336203776 | pass | Proof DB family size in bytes. |
| `proof_db_mib` | 250.000 | 320.630 | fail | Proof DB family must be <=250 MiB. |
| `proof_db_mib_stretch` | 150.000 | 320.630 | fail | Stretch target is <=150 MiB. |
| `audit_debug_db_bytes` | reported | 1063632896 | pass | Audit/debug DB bytes are reported separately and not counted against proof target. |
| `total_artifact_bytes` | reported | 1399836672 | pass | Proof + audit artifact bytes for operator planning. |
| `wal_bytes` | reported | 0 | pass | WAL bytes are reported separately. |
| `table_bytes` | reported | 241885184 | pass | dbstat table bytes. |
| `index_bytes` | reported | 94285824 | pass | dbstat index bytes. |
| `bytes_per_proof_edge` | 120.000 | 10557 | fail | Whole proof DB bytes divided by physical proof-edge rows; this remains a bloat signal. |
| `bytes_per_edge_table_plus_index` | 120.000 | 119.220 | pass | Physical edge table plus edge indexes per proof edge. |
| `bytes_per_entity` | reported | 76.100 | pass | Average total bytes per entity row. |
| `bytes_per_template_entity` | reported | 133.860 | pass | Average total bytes per template entity row. |
| `bytes_per_template_edge` | reported | 241.030 | pass | Average total bytes per template edge row. |
| `bytes_per_source_span` | reported | 109.590 | pass | Average total bytes per file_source_spans row. |
| `bytes_per_path_evidence_row` | reported | 2080 | pass | Average total bytes per PathEvidence row. |
| `source_snippets_not_stored_redundantly` | true | true | pass | Source snippets should be loaded from source files, not redundantly stored in SQLite. |

## Section 6 - Storage Contributors

| Object | Kind | Rows | MiB | Share | Previous bytes | Delta bytes | Classification |
| --- | --- | ---: | ---: | ---: | ---: | ---: | --- |
| `template_entities` | table | 653819 | 83.469 | 26.03% | 87523328 | 0 | template |
| `template_edges` | table | 169290 | 38.914 | 12.14% | 40804352 | 0 | template |
| `symbol_dict` | table | 696373 | 28.984 | 9.04% | 30392320 | 0 | dictionary |
| `qname_prefix_dict` | table | 155741 | 21.242 | 6.63% | 22274048 | 0 | dictionary |
| `idx_template_edges_head_relation` | index | unknown | 15.773 | 4.92% | 16539648 | 0 | template |
| `idx_template_edges_tail_relation` | index | unknown | 15.773 | 4.92% | 16539648 | 0 | template |
| `idx_symbol_dict_hash` | index | unknown | 13.926 | 4.34% | 14602240 | 0 | dictionary |
| `idx_qualified_name_parts` | index | unknown | 12.152 | 3.79% | 12742656 | 0 | dictionary |
| `qualified_name_dict` | table | 727372 | 11.609 | 3.62% | 12173312 | 0 | dictionary |
| `file_entities` | table | 89407 | 9.773 | 3.05% | 10248192 | 0 | proof_optional |
| `idx_file_entities_entity` | index | unknown | 8.676 | 2.71% | 9097216 | 0 | proof_optional |
| `path_evidence` | table | 4096 | 8.125 | 2.53% | 8519680 | 0 | derived_cache |
| `file_edges` | table | 62959 | 6.594 | 2.06% | 6914048 | 0 | proof_optional |
| `entities` | table | 89407 | 6.488 | 2.02% | 6803456 | 0 | proof_required |
| `file_source_spans` | table | 62005 | 6.480 | 2.02% | 6795264 | 0 | proof_required |
| `idx_file_edges_edge` | index | unknown | 5.883 | 1.83% | 6168576 | 0 | proof_optional |
| `idx_file_source_spans_span` | index | unknown | 5.812 | 1.81% | 6094848 | 0 | proof_optional |
| `idx_qname_prefix_dict_hash` | index | unknown | 3.156 | 0.98% | 3309568 | 0 | dictionary |
| `edges` | table | 31848 | 2.008 | 0.63% | 2105344 | 0 | proof_required |
| `files` | table | 4975 | 1.617 | 0.50% | 1695744 | 0 | proof_required |
| `callsite_args` | table | 27980 | 1.469 | 0.46% | 1540096 | 0 | structural |
| `idx_entities_qname` | index | unknown | 0.980 | 0.31% | 1028096 | 0 | dictionary |
| `idx_entities_name` | index | unknown | 0.969 | 0.30% | 1015808 | 0 | proof_optional |
| `path_dict` | table | 4975 | 0.379 | 0.12% | 397312 | 0 | dictionary |
| `source_content_template` | table | 2718 | 0.191 | 0.06% | 200704 | 0 | template |
| `sqlite_autoindex_source_content_template_1` | index | unknown | 0.188 | 0.06% | 196608 | 0 | template |
| `callsites` | table | 2177 | 0.121 | 0.04% | 126976 | 0 | structural |
| `edge_provenance_dict` | table | 1204 | 0.113 | 0.04% | 118784 | 0 | dictionary |
| `idx_files_content_template` | index | unknown | 0.098 | 0.03% | 102400 | 0 | template |
| `edge_class_dict` | table | 5 | 0.004 | 0.00% | 4096 | 0 | dictionary |
| `edge_context_dict` | table | 2 | 0.004 | 0.00% | 4096 | 0 | dictionary |
| `entity_kind_dict` | table | 31 | 0.004 | 0.00% | 4096 | 0 | dictionary |
| `exactness_dict` | table | 2 | 0.004 | 0.00% | 4096 | 0 | dictionary |
| `extractor_dict` | table | 6 | 0.004 | 0.00% | 4096 | 0 | dictionary |
| `language_dict` | table | 6 | 0.004 | 0.00% | 4096 | 0 | dictionary |
| `object_id_dict` | table | 0 | 0.004 | 0.00% | 4096 | 0 | dictionary |
| `relation_kind_dict` | table | 20 | 0.004 | 0.00% | 4096 | 0 | dictionary |
| `resolution_kind_dict` | table | 7 | 0.004 | 0.00% | 4096 | 0 | dictionary |
| `source_spans` | table | 0 | 0.004 | 0.00% | 4096 | 0 | proof_required |

## Section 7 - Row Counts And Cardinality

| Metric | Target | Observed | Status | Notes |
| --- | --- | --- | --- | --- |
| `files_walked` | reported | 8561 | pass | Cold proof build files walked. |
| `files_parsed` | reported | 2555 | pass | Cold proof build files parsed. |
| `content_templates` | reported | 2718 | pass | Source content templates. |
| `file_instances` | reported | 4975 | pass | Path-specific file instances. |
| `duplicate_content_templates` | reported | 2420 | pass | Duplicate local analyses skipped by content-template dedupe. |
| `template_entities` | reported | 653819 | pass | Template entity rows. |
| `template_edges` | reported | 169290 | pass | Template edge rows. |
| `proof_entities` | reported | 89407 | pass | Proof entity rows. |
| `proof_edges` | reported | 31848 | pass | Physical proof edge rows. |
| `structural_records` | reported | 0 | pass | Generic structural relation rows. |
| `callsites` | reported | 2177 | pass | Callsite rows. |
| `callsite_args` | reported | 27980 | pass | Callsite argument rows. |
| `symbols` | reported | 696373 | pass | Symbol dictionary rows. |
| `qname_prefixes` | reported | 155741 | pass | QName prefix dictionary rows. |
| `source_spans` | reported | 62005 | pass | File/source-span mapping rows. |
| `path_evidence_rows` | reported | 4096 | pass | Stored PathEvidence rows. |
| `heuristic_debug_rows` | reported | 1150760 | pass | Audit/debug sidecar rows preserved outside compact proof facts. |

## Section 8 - Cold Proof Build Profile

| Metric | Target | Observed | Status | Notes |
| --- | --- | --- | --- | --- |
| `cold_proof_build_total_wall_ms` | 60000 | 3001445 | fail | Cold proof build intended target is <=60 seconds. |
| `cold_proof_build_total_profile_ms` | reported | 3001445 | pass | Profile total from compact proof baseline. |
| `file_walk_time_ms` | reported | unknown | unknown | Not persisted in compact proof cold-build profile. |
| `metadata_diff_time_ms` | reported | unknown | unknown | Not persisted in compact proof cold-build profile. |
| `read_time_ms` | reported | unknown | unknown | Not persisted in compact proof cold-build profile. |
| `hash_time_ms` | reported | unknown | unknown | Not persisted in compact proof cold-build profile. |
| `parse_time_ms` | reported | unknown | unknown | Not persisted in compact proof cold-build profile. |
| `local_fact_bundle_creation_time_ms` | reported | unknown | unknown | Not persisted in compact proof cold-build profile. |
| `content_template_dedupe_time_ms` | reported | unknown | unknown | Not persisted in compact proof cold-build profile. |
| `reducer_time_ms` | reported | unknown | unknown | Not persisted in compact proof cold-build profile. |
| `symbol_interning_time_ms` | reported | unknown | unknown | Not persisted in compact proof cold-build profile. |
| `qname_prefix_interning_time_ms` | reported | unknown | unknown | Not persisted separately in compact proof cold-build profile. |
| `template_entity_insert_time_ms` | reported | unknown | unknown | Not persisted separately in compact proof cold-build profile. |
| `template_edge_insert_time_ms` | reported | unknown | unknown | Not persisted separately in compact proof cold-build profile. |
| `proof_entity_insert_time_ms` | reported | unknown | unknown | Not persisted separately in compact proof cold-build profile. |
| `proof_edge_insert_time_ms` | reported | unknown | unknown | Not persisted separately in compact proof cold-build profile. |
| `source_span_insert_time_ms` | reported | unknown | unknown | Not persisted separately in compact proof cold-build profile. |
| `path_evidence_generation_time_ms` | reported | unknown | unknown | Not persisted separately in compact proof cold-build profile. |
| `db_write_time_ms` | reported | 2877316 | pass | Known dominant cold-build stage. |
| `index_creation_time_ms` | reported | unknown | unknown | Not persisted separately in compact proof cold-build profile. |
| `fts_build_time_ms` | reported | unknown | unknown | Not persisted separately in compact proof cold-build profile. |
| `vacuum_time_ms` | reported | unknown | unknown | Not persisted separately in compact proof cold-build profile. |
| `analyze_time_ms` | reported | unknown | unknown | Not persisted separately in compact proof cold-build profile. |
| `integrity_check_time_ms` | reported | 12937 | pass | Cold proof DB integrity-check duration. |
| `graph_hash_time_ms` | reported | unknown | unknown | Not persisted separately in compact proof cold-build profile. |
| `report_generation_time_ms` | reported | unknown | unknown | Not persisted separately in compact proof cold-build profile. |

### Cold Build Mode Distinction

| Mode | Observed ms | Minutes | Status | Included in 50.02 min? | Classification |
| --- | ---: | ---: | --- | --- | --- |
| `proof-build-only` | 3001445 | 50.024 | fail | true | actual production proof-mode cold build as persisted by the compact gate |
| `proof-build-plus-validation` | 3014382 | 50.240 | fail | false | proof build plus the separately persisted integrity_check duration |
| `proof-build-plus-audit` | 6501810 | 108.364 | reported | false | sequential proof build plus separate audit-sidecar build |
| `proof-build-plus-audit-plus-validation` | 6514747 | 108.579 | reported | false | operator gate bundle, not the production proof-build-only target |
| `full-gate` | unknown | unknown | unknown | false | not persisted as a single wall-clock value in the compact gate |

### Cold Build Waterfall

| Stage | Elapsed ms | Share | Included in 50.02 min? | Source | Notes |
| --- | ---: | ---: | --- | --- | --- |
| `actual_proof_db_build_total` | 3001445 | 100.00% | true | `reports/final/compact_proof_db_gate.json.autoresearch.proof_build.wall_ms` | This is the persisted cold proof build number. |
| `production_persistence_and_global_reduction_bucket` | 2877316 | 95.86% | true | `reports/final/compact_proof_db_gate.json.autoresearch.proof_build.db_write_ms` | This bucket is currently broad: it includes SQLite persistence plus post-local global reduction, PathEvidence refresh, index recreation, ANALYZE/checkpoint work, and transaction commit where applicable. |
| `source_scan_parse_extract_dedupe_reducer_residual` | 124129 | 4.14% | true | `wall_ms - db_write_ms` | The compact gate did not persist nested cold-build spans for these phases; as a combined residual they are below 5% of proof-build-only wall time. |
| `integrity_check_validation` | 12937 | 0.43% | false | `reports/final/compact_proof_db_gate.json.autoresearch.proof_build.integrity_check_ms` | Integrity checking is separately measured and is not the dominant cause. |
| `audit_sidecar_build` | 3500365 | unknown | false | `reports/final/compact_proof_db_gate.json.autoresearch.audit_build.wall_ms` | Audit/debug-sidecar build time is a separate artifact and must not be blamed for the proof-build-only failure. |

## Section 9 - Repeat Unchanged Index

| Metric | Target | Observed | Status | Notes |
| --- | --- | --- | --- | --- |
| `repeat_unchanged_total_ms` | 5000 | 1674 | pass | Repeat unchanged index must complete within 5 seconds. |
| `repeat_files_walked` | reported | 8561 | pass | Repeat unchanged files walked. |
| `repeat_metadata_unchanged` | reported | 4975 | pass | Files skipped by metadata prefilter. |
| `repeat_files_read` | 0 | 0 | pass | Unchanged repeat should not read source files. |
| `repeat_files_hashed` | 0 | 0 | pass | Unchanged repeat should not hash source files. |
| `repeat_files_parsed` | 0 | 0 | pass | Unchanged repeat should not parse source files. |
| `repeat_entities_inserted` | 0 | unknown | unknown | Unchanged repeat should not insert proof entities. |
| `repeat_edges_inserted` | 0 | unknown | unknown | Unchanged repeat should not insert proof edges. |
| `repeat_templates_inserted` | reported | unknown | unknown | Template insert count is not persisted in compact proof repeat artifact. |
| `repeat_graph_fact_hash_changed` | false | false | pass | Unchanged repeat graph fact hash must remain stable. |
| `repeat_db_writes_performed` | no proof graph mutations | false | pass | Metadata/checkpoint writes may occur; proof graph rows must not mutate. |
| `repeat_validation_work_integrity_ms` | reported | 744 | pass | Validation work duration if the artifact persisted it. |
| `repeat_integrity_check_included` | reported | ok | pass | Repeat artifact reports integrity status separately. |

## Section 10 - Single-File Update

| Metric | Target | Observed | Status | Notes |
| --- | --- | --- | --- | --- |
| `single_file_update_total_ms` | 750.000 | 317.000 | pass | Single-file update p95 target is <=750 ms. |
| `update_file_walk_time_ms` | reported | 0.000 | pass | File walk span. |
| `update_read_time_ms` | reported | 0.043 | pass | File read span. |
| `update_hash_time_ms` | reported | 0.003 | pass | File hash span. |
| `update_parse_time_ms` | reported | 0.339 | pass | Parse span. |
| `update_stale_delete_time_ms` | reported | 14.689 | pass | Indexed stale fact cleanup span. |
| `update_template_invalidation_time_ms` | reported | unknown | unknown | Template invalidation is not persisted as a separate span. |
| `update_template_insert_update_time_ms` | reported | unknown | unknown | Template insert/update time is not persisted separately. |
| `update_proof_entity_update_time_ms` | reported | 118.688 | pass | Proof entity insertion/update span. |
| `update_proof_edge_update_time_ms` | reported | 149.831 | pass | Proof edge insertion/update span. |
| `update_dirty_path_evidence_count` | reported | 2 | pass | Dirty PathEvidence rows regenerated. |
| `update_path_evidence_regeneration_time_ms` | reported | 1.107 | pass | PathEvidence regeneration span. |
| `update_index_maintenance_time_ms` | reported | 0.000 | pass | Index maintenance span. |
| `update_transaction_commit_time_ms` | reported | 9.501 | pass | Transaction commit span. |
| `update_integrity_or_quick_check_time_ms` | reported | 785.593 | pass | Validation integrity duration captured outside the fast path. |
| `update_graph_hash_update_time_ms` | reported | 3.887 | pass | Graph digest measurement; fast mode uses the incremental digest instead of a full scan. |
| `update_restore_time_ms` | reported | 211.000 | pass | Restore update duration from update-integrity harness. |
| `update_rows_deleted` | reported | 1 | pass | Current artifact persists deleted fact files, not exact row count. |
| `update_rows_inserted` | reported | 44 | pass | Inserted proof entity + edge rows. |
| `update_indexes_touched` | reported | unknown | unknown | Indexes touched are not persisted in the artifact. |
| `update_global_work_accidentally_triggered` | false | false | pass | Fast path should avoid full graph hash scans unless validation mode explicitly requests them. |
| `update_integrity_remains_ok` | ok | ok | pass | DB integrity must remain ok after update. |

## Section 11 - Query Latency

| Query | Target p95 ms | p50 ms | p95 ms | p99 ms | Status | Notes |
| --- | ---: | ---: | ---: | ---: | --- | --- |
| `entity_name_lookup` | 250.000 | unknown | unknown | unknown | unknown | Entity lookup p95 was not measured in the compact proof gate. |
| `symbol_lookup` | 250.000 | unknown | unknown | unknown | unknown | Symbol lookup p95 was not measured in the compact proof gate. |
| `qname_lookup` | 250.000 | unknown | unknown | unknown | unknown | QName lookup p95 was not measured in the compact proof gate. |
| `text_fts_query` | 500.000 | unknown | unknown | unknown | unknown | Text/FTS p95 was not measured in the compact proof gate. |
| `relation_query_calls` | 500.000 | unknown | unknown | unknown | unknown | CALLS relation p95 was not measured in the compact proof gate. |
| `relation_query_reads_writes` | 500.000 | unknown | unknown | unknown | unknown | READS/WRITES relation p95 was not measured in the compact proof gate. |
| `context_pack_normal` | 2000 | 824.677 | 852.213 | 852.213 | pass | context_pack normal p95 from compact proof latency artifact. |
| `context_pack_impact` | 5000 | unknown | unknown | unknown | unknown | Impact mode context_pack p95 was not measured in the compact proof gate. |
| `unresolved_calls_paginated` | 1000 | 154.422 | 242.777 | 262.974 | pass | Paginated unresolved-calls p95 from compact proof latency artifact. |
| `impact_file` | 5000 | unknown | unknown | unknown | unknown | impact <file> p95 was not measured in the compact proof gate. |
| `path_evidence_lookup` | 500.000 | unknown | unknown | unknown | unknown | PathEvidence lookup p95 was not measured as a standalone query. |
| `source_snippet_batch_load` | 500.000 | unknown | unknown | unknown | unknown | Source snippet batch-load p95 was not measured as a standalone query. |

## Section 12 - Manual Relation Quality

Status: `unknown`. Real-world relation precision: `unknown`.

If labels are absent, real relation precision remains unknown and no precision claim is allowed.

## Section 13 - CGC / Competitor Comparison Readiness

| Metric | Value |
| --- | --- |
| CGC available | true |
| CGC version | 0.4.7 |
| CGC completed | false |
| CGC timeout | true |
| Verdict | incomplete |

## Section 14 - Regression Summary

| Metric | Previous | Current | Delta | Status |
| --- | ---: | ---: | ---: | --- |
| `proof_db_mib_vs_clean_1_63_gib` | 1633 | 320.630 | -1313 | improved |
| `context_pack_p95_ms_vs_clean_30s` | 30315 | 834.853 | -29480 | improved |
| `single_file_update_ms_vs_clean_80s` | 80207 | 1694 | -78513 | improved |
| `repeat_unchanged_ms_vs_clean_13s` | 13571 | 1674 | -11897 | improved |
| `cold_proof_build_ms_vs_compact_baseline` | 3001445 | 3001445 | 0.000 | unchanged |
| `proof_db_mib_vs_compact_baseline` | 320.630 | 320.630 | 0.000 | unchanged |
| `proof_db_mib_vs_previous_comprehensive` | 320.630 | 320.630 | 0.000 | unchanged |

## Operating Rule

Every future storage change must answer: did Graph Truth still pass, did Context Packet quality still pass, did proof DB size decrease, and did removed data move to a sidecar, become derivable, or get proven unnecessary. If the fourth answer is unclear, the change is not safe.
