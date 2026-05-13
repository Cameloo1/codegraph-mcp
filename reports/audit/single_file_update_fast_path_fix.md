# Single-File Update Fast Path Fix

Generated: 2026-05-12 12:20:42 -05:00

Source of truth: `MVP.md`.

## Verdict

Status: **pass for the single-file update target**.

The Autoresearch single-file update target is now met in `update-fast` mode:

- Previous compact baseline: `1.694s`
- Current Autoresearch update p95: `336ms`
- Target: `<=750ms p95`
- DB integrity after update: `ok`
- Graph Truth Gate: `11/11 passed`
- Context Packet Gate: `11/11 passed`
- Full workspace tests: `cargo test --workspace` passed

The comprehensive benchmark still has an overall **fail** verdict because storage and cold proof build remain over target:

- `proof_db_mib`
- `proof_db_mib_stretch`
- `bytes_per_proof_edge`
- `cold_proof_build_total_wall_ms`

## Root Causes Fixed

1. The update-integrity harness always ran a full graph fact hash and graph counts after every incremental update. `update-fast` now uses the incremental repo digest and skips full graph counts.
2. The update transaction rebuilt global repo count fields when prior repo state existed. Fast updates now preserve prior aggregate counts and mark the repo state metadata with `count_mode = preserved_previous_counts_fast_update`.
3. Rename detection scanned the full stored file manifest for every edit. It now runs only when the changed path is not already known in the manifest.
4. Old cache cleanup listed synthesized old entities even when the caller supplied an empty incremental cache. It now skips that lookup unless the cache actually has facts.
5. PathEvidence stale cleanup used JSON `LIKE` scans and global orphan cleanup despite materialized reverse maps. Reverse-map-backed DBs now delete dirty PathEvidence through indexed materialized tables.
6. Python file updates still triggered static JS/TS import impact analysis. Static resolver work now runs only when the changed/deleted path is in a static-resolver language.

## Modes

- `update-fast`: incremental digest, no full graph hash scan, no global graph counts, post-measurement quick check.
- `update-validated`: full graph hash, graph counts, full integrity check after update.
- `update-debug`: keeps validated behavior for heavier diagnostic runs.

## Autoresearch Evidence

Artifact: `reports/final/artifacts/compact_gate_update_integrity.json`

- Mode: `update-fast`
- Iterations: `12`
- Update samples ms: `312, 298, 313, 293, 313, 298, 336, 319, 310, 299, 314, 296`
- Update p95: `336ms`
- Restore p95: `273ms`
- `global_hash_check_ran`: `false`
- `graph_counts_ran`: `false`
- `graph_digest_kind`: `incremental_graph_digest`
- `integrity_check_kind`: `quick_check_post_measurement`
- `all_integrity_checks_passed`: `true`
- `changed_file_updates_graph_fact_hash`: `true`
- `restore_returns_to_repeat_graph_fact_hash`: `true`

First update profile:

| Stage | Time |
| --- | ---: |
| edge_insert | `143.852ms` |
| entity_insert | `121.490ms` |
| wal_checkpoint | `9.994ms` |
| stale_fact_delete | `9.933ms` |
| transaction_commit | `9.193ms` |
| graph_fact_hash | `7.914ms` |
| dictionary_lookup_insert | `6.661ms` |

## Correctness Gates

- Graph Truth Gate command: passed, `11/11`.
- Context Packet Gate command: passed, `11/11`.
- Stale update tests: `audit_incremental_delete_prunes_stale_target_and_retargets_live_import` and `audit_incremental_rename_prunes_old_path_and_retargets_import_call` passed.
- Full `cargo test --workspace`: passed.

## Storage Safety Rule

1. Did graph truth still pass? **Yes**, `11/11`.
2. Did context packet quality still pass? **Yes**, `11/11`.
3. Did proof DB size decrease? **No storage-size claim is made.** This phase was update-latency work, not storage compaction.
4. Did removed data move to a sidecar, become derivable, or get proven unnecessary? **No proof data was removed.** The JSON-scan cleanup path remains as the fallback for DBs without reverse maps; reverse-map-backed DBs use indexed materialized mappings.

One new lookup index was added for `path_evidence_edges(edge_id, path_id)` so dirty PathEvidence can be found without scanning path rows. The next storage gate must account for its size impact.

## Remaining Work

- Proof DB size remains above target.
- Cold proof build remains far above target.
- Template/dictionary write cost remains the dominant cold-build area.
- Manual real-repo relation precision is still unknown unless labels are supplied.
