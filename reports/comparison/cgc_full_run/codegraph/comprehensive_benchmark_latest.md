# Comprehensive Benchmark Latest

Source of truth: `MVP.md`.

Execution mode: `fresh_proof_build`. The benchmark built a fresh proof DB artifact before reading storage and cold-build metrics.

## Section 1 - Executive Verdict

- Verdict: **fail**
- Reason: failed targets: bytes_per_proof_edge, cold_proof_build_total_wall_ms, proof_db_mib_stretch, symbol_lookup
- Optimization may continue: `true`
- Comparison claims allowed: `false`

### Failed targets

- `bytes_per_proof_edge`
- `cold_proof_build_total_wall_ms`
- `proof_db_mib_stretch`
- `symbol_lookup`

### Passed targets

- `artifact_has_freshness_metadata`
- `artifact_integrity_ok`
- `artifact_not_stale`
- `artifact_reuse_marked`
- `artifact_schema_matches_current`
- `audit_debug_db_bytes`
- `bytes_per_edge_table_plus_index`
- `bytes_per_entity`
- `bytes_per_path_evidence_row`
- `bytes_per_source_span`
- `bytes_per_template_edge`
- `bytes_per_template_entity`
- `cold_build_result_claimable`
- `cold_proof_build_total_profile_ms`
- `context_cases_passed`
- `context_cases_total`
- `context_pack_normal`
- `critical_symbol_recall`
- `db_write_time_ms`
- `derived_without_provenance_count`
- `distractor_ratio`
- `entity_name_lookup`
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
- `path_evidence_lookup`
- `proof_db_bytes`
- `proof_db_freshly_built`
- `proof_db_mib`
- `proof_path_coverage`
- `proof_path_source_span_coverage`
- `qname_lookup`
- `relation_query_calls`
- `relation_query_reads_writes`
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
- `source_snippet_batch_load`
- `source_snippets_not_stored_redundantly`
- `source_span_failures`
- `stale_fact_failures`
- `storage_result_claimable`
- `stored_path_evidence_rows`
- `table_bytes`
- `test_mock_production_leakage_count`
- `text_fts_query`
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

## Section 4A - Proof Artifact Freshness

| Metric | Target | Observed | Status | Notes |
| --- | --- | --- | --- | --- |
| `proof_db_freshly_built` | true unless explicit artifact reuse requested | true | pass | Comprehensive benchmark builds a fresh proof DB by default; explicit reuse must be labeled. |
| `artifact_reuse_marked` | reported | false | pass | Explicit artifact reuse is visible in the report. |
| `artifact_has_freshness_metadata` | true | true | pass | Reused artifacts require freshness metadata; fresh builds write it. |
| `artifact_schema_matches_current` | true | true | pass | Artifact schema_version must match the current SQLite schema version. |
| `artifact_integrity_ok` | ok | ok | pass | Proof artifact must pass integrity before benchmark numbers are claimable. |
| `artifact_not_stale` | true | true | pass | Stale artifact reuse forces the master gate to fail or remain unknown. |
| `storage_result_claimable` | true | true | pass | Storage result is claimable because the proof artifact is fresh or freshness-validated. |
| `cold_build_result_claimable` | true | true | pass | Cold-build result is claimable because the proof artifact is fresh or freshness-validated. |

### Proof Artifact Metadata

| Field | Value |
| --- | --- |
| `artifact_path` | <REPO_ROOT>\reports\comparison\cgc_full_run\codegraph\artifacts\comprehensive_proof_cgc_full_run.sqlite |
| `artifact_metadata_path` | <REPO_ROOT>\reports\comparison\cgc_full_run\codegraph\artifacts\comprehensive_proof_cgc_full_run.artifact.json |
| `freshness_metadata_present` | true |
| `artifact_reuse` | false |
| `freshly_built` | true |
| `stale` | false |
| `freshness_status` | fresh |
| `schema_version` | 19 |
| `current_schema_version` | 19 |
| `migration_version` | 19 |
| `current_migration_version` | 19 |
| `storage_mode` | proof |
| `db_size_bytes` | 179499008 |
| `integrity_status` | ok |
| `build_duration_ms` | 224729 |
| `storage_result_claimable` | true |
| `cold_build_result_claimable` | true |

## Section 5 - Storage Summary

| Metric | Target | Observed | Status | Notes |
| --- | --- | --- | --- | --- |
| `proof_db_bytes` | reported | 179499008 | pass | Proof DB family size in bytes. |
| `proof_db_mib` | 250.000 | 171.184 | pass | Proof DB family must be <=250 MiB. |
| `proof_db_mib_stretch` | 150.000 | 171.184 | fail | Stretch target is <=150 MiB. |
| `audit_debug_db_bytes` | reported | 1063632896 | pass | Audit/debug DB bytes are reported separately and not counted against proof target. |
| `total_artifact_bytes` | reported | 1243131904 | pass | Proof + audit artifact bytes for operator planning. |
| `wal_bytes` | reported | 0 | pass | WAL bytes are reported separately. |
| `table_bytes` | reported | 139456512 | pass | dbstat table bytes. |
| `index_bytes` | reported | 40009728 | pass | dbstat index bytes. |
| `bytes_per_proof_edge` | 120.000 | 5649 | fail | Whole proof DB bytes divided by physical proof-edge rows; this remains a bloat signal. |
| `bytes_per_edge_table_plus_index` | 120.000 | 118.590 | pass | Physical edge table plus edge indexes per proof edge. |
| `bytes_per_entity` | reported | 75.910 | pass | Average total bytes per entity row. |
| `bytes_per_template_entity` | reported | 88.930 | pass | Average total bytes per template entity row. |
| `bytes_per_template_edge` | reported | 104.810 | pass | Average total bytes per template edge row. |
| `bytes_per_source_span` | reported | 109.460 | pass | Average total bytes per file_source_spans row. |
| `bytes_per_path_evidence_row` | reported | 1114 | pass | Average total bytes per PathEvidence row. |
| `source_snippets_not_stored_redundantly` | true | true | pass | Source snippets should be loaded from source files, not redundantly stored in SQLite. |

## Section 6 - Storage Contributors

| Object | Kind | Rows | MiB | Share | Previous bytes | Delta bytes | Classification |
| --- | --- | ---: | ---: | ---: | ---: | ---: | --- |
| `template_entities` | table | 653819 | 55.453 | 32.39% | 58146816 | 0 | template |
| `template_edges` | table | 169290 | 16.922 | 9.89% | 17743872 | 0 | template |
| `file_entities` | table | 89407 | 9.773 | 5.71% | 10248192 | 0 | proof_optional |
| `idx_file_entities_entity` | index | unknown | 8.676 | 5.07% | 9097216 | 0 | proof_optional |
| `symbol_dict` | table | 176558 | 7.703 | 4.50% | 8077312 | 0 | dictionary |
| `file_edges` | table | 62886 | 6.578 | 3.84% | 6897664 | 0 | proof_optional |
| `entities` | table | 89407 | 6.473 | 3.78% | 6787072 | 0 | proof_required |
| `file_source_spans` | table | 61932 | 6.465 | 3.78% | 6778880 | 0 | proof_required |
| `idx_file_edges_edge` | index | unknown | 5.875 | 3.43% | 6160384 | 0 | proof_optional |
| `qname_prefix_dict` | table | 48603 | 5.863 | 3.43% | 6148096 | 0 | dictionary |
| `idx_file_source_spans_span` | index | unknown | 5.797 | 3.39% | 6078464 | 0 | proof_optional |
| `idx_qualified_name_parts` | index | unknown | 4.410 | 2.58% | 4624384 | 0 | dictionary |
| `path_evidence` | table | 4096 | 4.352 | 2.54% | 4562944 | 0 | derived_cache |
| `qualified_name_dict` | table | 275831 | 4.199 | 2.45% | 4403200 | 0 | dictionary |
| `idx_symbol_dict_hash` | index | unknown | 3.504 | 2.05% | 3674112 | 0 | dictionary |
| `edges` | table | 31775 | 1.980 | 1.16% | 2076672 | 0 | proof_required |
| `files` | table | 4975 | 1.617 | 0.94% | 1695744 | 0 | proof_required |
| `callsite_args` | table | 27980 | 1.469 | 0.86% | 1540096 | 0 | structural |
| `path_evidence_edges` | table | 4096 | 1.273 | 0.74% | 1335296 | 0 | derived_cache |
| `idx_entities_qname` | index | unknown | 0.980 | 0.57% | 1028096 | 0 | dictionary |
| `idx_entities_name` | index | unknown | 0.961 | 0.56% | 1007616 | 0 | proof_optional |
| `idx_qname_prefix_dict_hash` | index | unknown | 0.961 | 0.56% | 1007616 | 0 | dictionary |
| `path_dict` | table | 4975 | 0.379 | 0.22% | 397312 | 0 | dictionary |
| `source_content_template` | table | 2715 | 0.191 | 0.11% | 200704 | 0 | template |
| `sqlite_autoindex_source_content_template_1` | index | unknown | 0.188 | 0.11% | 196608 | 0 | template |
| `callsites` | table | 2177 | 0.121 | 0.07% | 126976 | 0 | structural |
| `edge_provenance_dict` | table | 1131 | 0.109 | 0.06% | 114688 | 0 | dictionary |
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
| `files_walked` | reported | 8564 | pass | Cold proof build files walked. |
| `files_parsed` | reported | 2555 | pass | Cold proof build files parsed. |
| `content_templates` | reported | 2715 | pass | Source content templates. |
| `file_instances` | reported | 4975 | pass | Path-specific file instances. |
| `duplicate_content_templates` | reported | 3589 | pass | Duplicate local analyses skipped by content-template dedupe. |
| `template_entities` | reported | 653819 | pass | Template entity rows. |
| `template_edges` | reported | 169290 | pass | Template edge rows. |
| `proof_entities` | reported | 89407 | pass | Proof entity rows. |
| `proof_edges` | reported | 31775 | pass | Physical proof edge rows. |
| `structural_records` | reported | 0 | pass | Generic structural relation rows. |
| `callsites` | reported | 2177 | pass | Callsite rows. |
| `callsite_args` | reported | 27980 | pass | Callsite argument rows. |
| `symbols` | reported | 176558 | pass | Symbol dictionary rows. |
| `qname_prefixes` | reported | 48603 | pass | QName prefix dictionary rows. |
| `source_spans` | reported | 61932 | pass | File/source-span mapping rows. |
| `path_evidence_rows` | reported | 4096 | pass | Stored PathEvidence rows. |
| `heuristic_debug_rows` | reported | 1150760 | pass | Audit/debug sidecar rows preserved outside compact proof facts. |

## Section 8 - Cold Proof Build Profile

| Metric | Target | Observed | Status | Notes |
| --- | --- | --- | --- | --- |
| `cold_proof_build_total_wall_ms` | 60000 | 224729 | fail | Cold proof build intended target is <=60 seconds. |
| `cold_proof_build_total_profile_ms` | reported | 224729 | pass | Profile total from compact proof baseline. |
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
| `db_write_time_ms` | reported | 122264 | pass | Known dominant cold-build stage. |
| `index_creation_time_ms` | reported | unknown | unknown | Not persisted separately in compact proof cold-build profile. |
| `fts_build_time_ms` | reported | unknown | unknown | Not persisted separately in compact proof cold-build profile. |
| `vacuum_time_ms` | reported | unknown | unknown | Not persisted separately in compact proof cold-build profile. |
| `analyze_time_ms` | reported | unknown | unknown | Not persisted separately in compact proof cold-build profile. |
| `integrity_check_time_ms` | reported | 464.988 | pass | Cold proof DB integrity-check duration. |
| `graph_hash_time_ms` | reported | unknown | unknown | Not persisted separately in compact proof cold-build profile. |
| `report_generation_time_ms` | reported | unknown | unknown | Not persisted separately in compact proof cold-build profile. |

### Cold Build Mode Distinction

| Mode | Observed ms | Minutes | Status | Included in 50.02 min? | Classification |
| --- | ---: | ---: | --- | --- | --- |
| `proof-build-only` | 224729 | 3.745 | fail | true | actual production proof-mode cold build as persisted by the compact gate |
| `proof-build-plus-validation` | 225194 | 3.753 | fail | false | proof build plus the separately persisted integrity_check duration |
| `proof-build-plus-audit` | 3725094 | 62.085 | reported | false | sequential proof build plus separate audit-sidecar build |
| `proof-build-plus-audit-plus-validation` | 3725559 | 62.093 | reported | false | operator gate bundle, not the production proof-build-only target |
| `full-gate` | unknown | unknown | unknown | false | not persisted as a single wall-clock value in the compact gate |

### Cold Build Waterfall

| Stage | Elapsed ms | Share | Included in 50.02 min? | Source | Notes |
| --- | ---: | ---: | --- | --- | --- |
| `actual_proof_db_build_total` | 224729 | 100.00% | true | `reports/final/compact_proof_db_gate.json.autoresearch.proof_build.wall_ms` | This is the persisted cold proof build number. |
| `production_persistence_and_global_reduction_bucket` | 122264 | 54.41% | true | `reports/final/compact_proof_db_gate.json.autoresearch.proof_build.db_write_ms` | This bucket is currently broad: it includes SQLite persistence plus post-local global reduction, PathEvidence refresh, index recreation, ANALYZE/checkpoint work, and transaction commit where applicable. |
| `source_scan_parse_extract_dedupe_reducer_residual` | 102465 | 45.59% | true | `wall_ms - db_write_ms` | The compact gate did not persist nested cold-build spans for these phases; as a combined residual they are below 5% of proof-build-only wall time. |
| `integrity_check_validation` | 464.988 | 0.21% | false | `reports/final/compact_proof_db_gate.json.autoresearch.proof_build.integrity_check_ms` | Integrity checking is separately measured and is not the dominant cause. |
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
| `single_file_update_total_ms` | 750.000 | 336.000 | pass | Single-file update p95 target is <=750 ms. |
| `update_file_walk_time_ms` | reported | 0.000 | pass | File walk span. |
| `update_read_time_ms` | reported | 0.073 | pass | File read span. |
| `update_hash_time_ms` | reported | 0.003 | pass | File hash span. |
| `update_parse_time_ms` | reported | 0.340 | pass | Parse span. |
| `update_stale_delete_time_ms` | reported | 9.933 | pass | Indexed stale fact cleanup span. |
| `update_template_invalidation_time_ms` | reported | unknown | unknown | Template invalidation is not persisted as a separate span. |
| `update_template_insert_update_time_ms` | reported | unknown | unknown | Template insert/update time is not persisted separately. |
| `update_proof_entity_update_time_ms` | reported | 121.490 | pass | Proof entity insertion/update span. |
| `update_proof_edge_update_time_ms` | reported | 143.852 | pass | Proof edge insertion/update span. |
| `update_dirty_path_evidence_count` | reported | 2 | pass | Dirty PathEvidence rows regenerated. |
| `update_path_evidence_regeneration_time_ms` | reported | 1.159 | pass | PathEvidence regeneration span. |
| `update_index_maintenance_time_ms` | reported | 0.000 | pass | Index maintenance span. |
| `update_transaction_commit_time_ms` | reported | 9.193 | pass | Transaction commit span. |
| `update_integrity_or_quick_check_time_ms` | reported | 778.821 | pass | Validation integrity duration captured outside the fast path. |
| `update_graph_hash_update_time_ms` | reported | 4.841 | pass | Graph digest measurement; fast mode uses the incremental digest instead of a full scan. |
| `update_restore_time_ms` | reported | 223.000 | pass | Restore update duration from update-integrity harness. |
| `update_rows_deleted` | reported | 1 | pass | Current artifact persists deleted fact files, not exact row count. |
| `update_rows_inserted` | reported | 44 | pass | Inserted proof entity + edge rows. |
| `update_indexes_touched` | reported | unknown | unknown | Indexes touched are not persisted in the artifact. |
| `update_global_work_accidentally_triggered` | false | false | pass | Fast path should avoid full graph hash scans unless validation mode explicitly requests them. |
| `update_integrity_remains_ok` | ok | ok | pass | DB integrity must remain ok after update. |

## Section 11 - Query Latency

| Query | Target p95 ms | p50 ms | p95 ms | p99 ms | Status | Notes |
| --- | ---: | ---: | ---: | ---: | --- | --- |
| `entity_name_lookup` | 250.000 | 1.984 | 2.637 | 2.637 | pass | Default entity-name lookup must compile and use compact proof entity dictionaries. |
| `symbol_lookup` | 250.000 | 195.801 | 268.788 | 268.788 | fail | Default symbol lookup must resolve object ID, name, and qualified-name paths without audit/debug sidecars. |
| `qname_lookup` | 250.000 | 18.899 | 28.357 | 28.357 | pass | Default qname lookup must use compact qualified-name reconstruction. |
| `text_fts_query` | 500.000 | 0.057 | 0.182 | 0.182 | pass | Proof-mode text query must use compact FTS or fail explicitly. |
| `relation_query_calls` | 500.000 | 248.031 | 307.594 | 307.594 | pass | Bounded CALLS relation lookup must use proof edges and compact compatibility views. |
| `relation_query_reads_writes` | 500.000 | 249.227 | 272.977 | 272.977 | pass | Bounded READS/WRITES lookup must compile against compact proof edges. |
| `path_evidence_lookup` | 500.000 | 2.974 | 4.483 | 4.483 | pass | Stored PathEvidence lookup must use proof-mode materialized lookup tables. |
| `source_snippet_batch_load` | 500.000 | 51.413 | 73.698 | 73.698 | pass | Source snippets must load from source files using proof-edge/source-span rows, not redundant SQLite snippet storage. |
| `context_pack_normal` | 2000 | 141.578 | 198.410 | 198.410 | pass | Normal context_pack must use proof-mode stored PathEvidence first and keep source-span labels. |
| `unresolved_calls_paginated` | 1000 | 93.722 | 119.457 | 119.457 | pass | Unresolved-calls pagination may read the heuristic/debug sidecar by design, but it must remain explicit and bounded. |
| `context_pack_impact` | 5000 | unknown | unknown | unknown | unknown | Impact mode context_pack p95 was not measured in the compact proof gate. |
| `impact_file` | 5000 | unknown | unknown | unknown | unknown | impact <file> p95 was not measured in the compact proof gate. |

## Section 12 - Manual Relation Quality

Status: `reported`. Real-world relation precision: `reported_for_labeled_relations_no_claim_for_absent_relations`.

Labeled samples: `320`.

| Relation | Proof DB edges | Labeled | Precision | Target | Status | Claim |
| --- | ---: | ---: | ---: | ---: | --- | --- |
| `CALLS` | 2363 | 50 | 100.00% | 95.00% | pass | sampled_precision_estimate |
| `READS` | 1970 | 50 | 100.00% | 90.00% | pass | sampled_precision_estimate |
| `WRITES` | 1104 | 50 | 100.00% | 90.00% | pass | sampled_precision_estimate |
| `FLOWS_TO` | 14801 | 50 | 100.00% | 85.00% | pass | sampled_precision_estimate |
| `MUTATES` | 177 | 50 | 100.00% | unknown | reported_no_target | sampled_precision_estimate |
| `AUTHORIZES` | 0 | 0 | unknown | 95.00% | no_claim_absent_in_proof_db | no_precision_claim |
| `CHECKS_ROLE` | 0 | 0 | unknown | 95.00% | no_claim_absent_in_proof_db | no_precision_claim |
| `SANITIZES` | 0 | 0 | unknown | 95.00% | no_claim_absent_in_proof_db | no_precision_claim |
| `EXPOSES` | 0 | 0 | unknown | unknown | no_claim_absent_in_proof_db | no_precision_claim |
| `TESTS` | 0 | 0 | unknown | 90.00% | no_claim_absent_in_proof_db | no_precision_claim |
| `ASSERTS` | 0 | 0 | unknown | 90.00% | no_claim_absent_in_proof_db | no_precision_claim |
| `MOCKS` | 0 | 0 | unknown | 90.00% | no_claim_absent_in_proof_db | no_precision_claim |
| `STUBS` | 0 | 0 | unknown | 90.00% | no_claim_absent_in_proof_db | no_precision_claim |
| `PathEvidence` | 20 | 20 | 100.00% | 95.00% | pass | sampled_precision_estimate |
| `MAY_MUTATE` | 1130 | 50 | 100.00% | unknown | reported_no_target | sampled_precision_estimate |

| Labeled relation | Samples | Precision | Source-span precision | False positives | Unsure |
| --- | ---: | ---: | ---: | ---: | ---: |
| `CALLS` | 50 | 100.00% | 100.00% | 0 | 0 |
| `FLOWS_TO` | 50 | 100.00% | 100.00% | 0 | 0 |
| `MAY_MUTATE` | 50 | 100.00% | 100.00% | 0 | 0 |
| `MUTATES` | 50 | 100.00% | 100.00% | 0 | 0 |
| `PathEvidence` | 20 | 100.00% | 100.00% | 0 | 0 |
| `READS` | 50 | 100.00% | 100.00% | 0 | 0 |
| `WRITES` | 50 | 100.00% | 100.00% | 0 | 0 |

Absent proof-mode relations with no precision claim: AUTHORIZES, CHECKS_ROLE, SANITIZES, EXPOSES, TESTS, ASSERTS, MOCKS, STUBS.

Precision claims are limited to labeled samples; recall remains unknown without a false-negative gold denominator.

## Section 13 - CGC / Competitor Comparison Readiness

| Metric | Value |
| --- | --- |
| CGC available | false |
| CGC version | unknown |
| CGC completed | false |
| CGC timeout | false |
| Verdict | incomplete |

## Section 14 - Regression Summary

| Metric | Previous | Current | Delta | Status |
| --- | ---: | ---: | ---: | --- |
| `proof_db_mib_vs_clean_1_63_gib` | 1633 | 171.184 | -1462 | improved |
| `context_pack_p95_ms_vs_clean_30s` | 30315 | 834.853 | -29480 | improved |
| `single_file_update_ms_vs_clean_80s` | 80207 | 1694 | -78513 | improved |
| `repeat_unchanged_ms_vs_clean_13s` | 13571 | 1674 | -11897 | improved |
| `cold_proof_build_ms_vs_compact_baseline` | 224729 | 224729 | 0.000 | unchanged |
| `proof_db_mib_vs_compact_baseline` | 320.630 | 171.184 | -149.446 | improved |
| `proof_db_mib_vs_previous_comprehensive` | 171.184 | 171.184 | 0.000 | unchanged |

## Operating Rule

Every future storage change must answer: did Graph Truth still pass, did Context Packet quality still pass, did proof DB size decrease, and did removed data move to a sidecar, become derivable, or get proven unnecessary. If the fourth answer is unclear, the change is not safe.
