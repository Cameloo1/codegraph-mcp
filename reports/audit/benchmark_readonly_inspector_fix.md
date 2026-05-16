# Benchmark Read-Only Inspector Fix

Generated at: 2026-05-15T23:22:58-05:00

Scope: benchmark and audit helper inspection paths. This pass keeps mutation-capable work limited to explicitly named setup/update operations and makes inspection claimability explicit.

## Source-Of-Truth Status

- `MVP.md` is missing from this active worktree at `C:\Users\wamin\.codex\worktrees\4160\codegraph-mcp\MVP.md`.
- The canonical sibling copy at `C:\Users\wamin\Desktop\development\codegraph-mcp\MVP.md` was read before changes.
- `reports/audit/db_lifecycle_surface_inventory.md` and `reports/audit/lifecycle_preflight_api_design.md` were read and used as the surface inventory and lifecycle API contract.

## Confirmed Risk

Benchmark/update-integrity helper reads were still able to use mutation-capable `SqliteGraphStore::open` for inspection-only work such as schema version, quick/full integrity checks, graph counts, graph fact hash, and incremental graph digest. Since `open` configures and migrates the store, those inspections could make old or mismatched artifacts less honest.

## Inventory Outcome

- `crates/codegraph-cli/src/lib.rs`
  - `quick_integrity_status`, `full_integrity_status`, `graph_counts_for_db`, `graph_fact_hash_for_db`, and `incremental_graph_digest_for_db` now use `SqliteGraphStore::open_read_only`.
  - `bench query-surface` preflights the exact DB path, opens read-only, and emits `inspection_read_only`, `artifact_mutated_during_inspection`, `lifecycle_status`, and `claimable`.
  - `bench comprehensive` fresh and existing artifact paths emit lifecycle status plus read-only inspection metadata, and claim storage/cold-build results only when lifecycle metadata is claimable.
  - `bench update-integrity` repo/step reports emit read-only inspection metadata, lifecycle status, claimability, and explicit mutation-capable operation names.
  - The remaining mutation-capable helper opens are setup/mutation helpers: digest priming, repo digest replacement, staged update repo state adjustment, indexing, and update.
- `crates/codegraph-cli/src/audit.rs`
  - Audit DB opens already used `OpenFlags::SQLITE_OPEN_READ_ONLY`; the helper now also sets `PRAGMA query_only = ON`.
  - Storage audit JSON/markdown now reports `inspection_read_only=true` and `artifact_mutated_during_inspection=false`.
  - Storage experiments still mutate copied DBs only.
- `crates/codegraph-store/src/sqlite.rs`
  - Added `SqliteGraphStore::open_read_only`, which opens with read-only flags, enables `query_only`, registers SQLite helper functions, and intentionally does not call configure or migrate.
- `crates/codegraph-bench`
  - Direct opens remain generated-fixture benchmark setup/evaluation surfaces, not arbitrary user artifact inspectors.

## Behavior

- Old-schema DB inspection does not migrate or rewrite the DB.
- Graph counts, graph fact hash, quick check, full integrity check, and incremental digest are available through the read-only store path.
- Mismatched benchmark artifacts are reported blocked/non-claimable through lifecycle status instead of being repaired during inspection.
- Benchmark reports now expose whether inspection was read-only and whether an artifact was mutated during inspection.

## Tests Added Or Updated

- Store tests:
  - `read_only_open_does_not_migrate_old_schema_db`
  - `read_only_open_supports_counts_hash_and_integrity_checks`
- CLI tests:
  - `benchmark_inspection_lifecycle_blocks_mismatched_db_as_nonclaimable`
  - `update_integrity_report_marks_inspection_read_only_and_setup_mutations`
- Existing smoke tests were aligned with current benchmark honesty:
  - proof-build-validated reports the proof-build portion instead of `0`
  - explicit artifact reuse is claimable only with a valid passported exact-repo DB
  - UI HTTP test helper tolerates Windows connection reset only after a complete HTTP response is already present

## Verification

- `cargo fmt`: passed, with existing warning `could not canonicalize path C:\Users\wamin`.
- `cargo test -p codegraph-store read_only_open`: passed, 2 tests matched.
- `cargo test -p codegraph-cli benchmark_inspection`: passed, 1 test matched.
- `cargo test -p codegraph-cli update_integrity_report_marks_inspection_read_only_and_setup_mutations`: passed, 1 test matched.
- `cargo test -p codegraph-cli --test cli_smoke bench_comprehensive_explicit_reuse_is_marked_with_claimable_metadata`: passed.
- `cargo test -p codegraph-cli --test cli_smoke bench_proof_build_validated_marks_artifact_validated_only_after_integrity_gate`: passed.
- `cargo test -p codegraph-cli tests::ui_server_starts_and_status_endpoint_uses_real_index -- --exact`: passed.
- `cargo test --workspace`: passed on final rerun.
- `git diff --check`: passed, with CRLF normalization warnings only.

## Dirty State

The worktree already contained broad uncommitted source/test/report changes before this pass. This pass added benchmark/audit read-only inspection changes and this report pair. Existing untracked audit reports remain untracked; no DBs, WAL/SHM files, raw logs, `target/`, or CGC payloads were intentionally added.
