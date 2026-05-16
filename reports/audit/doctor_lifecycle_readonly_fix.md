# Doctor lifecycle read-only fix

Generated at: 2026-05-15T22:47:46-05:00

Scope: make `doctor` report DB lifecycle/passport safety from read-only preflight evidence instead of opening the SQLite graph store through the mutation-capable store path.

## Source-of-truth status

- `MVP.md` is missing from this active worktree at `C:\Users\wamin\.codex\worktrees\4160\codegraph-mcp\MVP.md`.
- The canonical sibling checkout copy at `C:\Users\wamin\Desktop\development\codegraph-mcp\MVP.md` was read first for project intent.
- `reports/audit/db_lifecycle_surface_inventory.md` and `reports/audit/lifecycle_preflight_api_design.md` were read before implementation.

## Inspection result

The reported risk was present in this checkout before the patch.

- `doctor` computed DB health with `SqliteGraphStore::open(&db_path).schema_version()`.
- That path is mutation-capable and can run store setup/migration behavior.
- It did not report the shared lifecycle/passport blockers that normal read paths use.

## Implementation

Changed `crates/codegraph-cli/src/lib.rs`:

- `run_doctor_command` now calls `inspect_db_lifecycle_surface_preflight` with:
  - `surface_name=cli.doctor`
  - `operation_kind=normal_read`
  - `allow_stale_read=false`
  - `allow_foreign_repo=false`
- Removed the mutation-capable DB-open probe from `doctor`.
- Added top-level doctor JSON fields:
  - `database_exists`
  - `safe_to_query`
  - `passport_status`
  - `repo_match`
  - `scope_match`
  - `schema_status`
  - `storage_mode`
  - `storage_mode_status`
  - `blockers`
  - `lifecycle_warnings`
  - `sqlite_sidecars`
  - `db_lifecycle_read`
- Kept missing DB as a doctor warning, preserving the existing nonfatal doctor behavior for unindexed repos.
- Existing unsafe DBs now make the database check an error, so doctor no longer labels unsafe-but-openable DBs as ok.
- Added `sqlite_sidecars_status` to report `-wal` and `-shm` state without mutating the DB.

## Regression coverage

Added or expanded tests:

- `doctor_and_config_outputs_are_structured_nonfatal`
- `doctor_valid_db_reports_readonly_lifecycle_evidence`
- `doctor_repo_mismatch_matches_status_blockers`
- `doctor_reports_scope_mismatch_without_opening_mutably`
- `doctor_reports_missing_passport_for_existing_db`
- `doctor_does_not_migrate_old_schema_db`

Covered behavior:

- Valid DB doctor result is `ok` and `safe_to_query=true`.
- Repo-mismatched DB is not ok.
- Doctor and status report matching lifecycle blockers for a mismatched DB.
- Scope mismatch is visible in doctor output.
- Existing DB with missing passport is reported as unsafe.
- Old-schema DB keeps its original `user_version`; doctor does not migrate it.
- `doctor --json` includes lifecycle evidence.

## Verification

- `cargo fmt`: passed.
- `cargo test -p codegraph-cli doctor`: passed; 7 focused doctor tests matched across lib/bin/integration targets.
- `git diff --check`: passed, with existing CRLF normalization warnings for already dirty files.
- `python -m json.tool reports/audit/doctor_lifecycle_readonly_fix.json`: passed.
- `cargo test --workspace`: failed on the same existing Windows UI socket reset test, `tests::ui_server_starts_and_status_endpoint_uses_real_index`.
- `cargo test -p codegraph-cli tests::ui_server_starts_and_status_endpoint_uses_real_index -- --exact`: passed on rerun.

## Remaining notes

This pass only fixed the `doctor` lifecycle surface. Other inventory P0 surfaces, including `bundle import`, remain for separate targeted fixes.
