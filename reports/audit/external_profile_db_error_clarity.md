# External Profile DB Error Clarity

Created: 2026-05-16T00:12:24-05:00

## Summary

Implemented clearer lifecycle/preflight error classification for configured DB paths, especially profile DBs outside the current workspace such as LocalAppData-backed production profiles.

The active worktree does not contain `MVP.md`; the canonical sibling `C:\Users\wamin\Desktop\development\codegraph-mcp\MVP.md` was read first and treated as source of truth. No production graph semantics, benchmark thresholds, passport checks, or read safety gates were weakened.

## Behavior Added

- `DbPreflightReport` now carries:
  - `db_problem_kind`
  - `path_access_status`
  - `path_access_error`
- Lifecycle preflight now carries:
  - `exact_db_path_checked`
  - `repo_root_expected`
  - `db_path_outside_workspace`
  - `outside_workspace_note`
- Status, doctor, read lifecycle JSON, and MCP lifecycle JSON expose the DB path/access fields.
- If a DB path is outside the workspace, outputs include:
  - `This profile DB is outside the workspace; grant access or choose a workspace-local DB.`
- Inaccessible or permission-denied paths are no longer labeled corrupt unless SQLite/passport corruption is actually proven.
- MCP read failures now preserve specific machine-readable kinds such as `repo_root_mismatch`, `scope_mismatch`, `schema_mismatch`, `db_missing`, `permission_denied`, and `filesystem_inaccessible`.

## Error Kind Separation

Covered classifications:

- `filesystem_inaccessible`
- `permission_denied`
- `db_missing`
- `passport_missing`
- `passport_corrupt`
- `repo_root_mismatch`
- `schema_mismatch`

Additional precise kinds retained where useful:

- `scope_mismatch`
- `storage_mismatch`
- `db_locked`
- `sqlite_corrupt`

## Source Changes

- `crates/codegraph-store/src/sqlite.rs`
  - Added path-access inspection before SQLite open.
  - Added structured problem/access fields to DB preflight reports.
  - Added tests for missing DB, missing passport, corrupt passport row, and permission-denied classification.
- `crates/codegraph-index/src/lib.rs`
  - Threaded problem/access fields through shared lifecycle and surface preflight.
  - Added outside-workspace detection and external profile DB note.
  - Added regression coverage for external profile DB paths.
- `crates/codegraph-cli/src/lib.rs`
  - Updated status/doctor/read lifecycle JSON.
  - Updated read error messages to include machine-readable kind and outside-workspace note.
- `crates/codegraph-mcp-server/src/lib.rs`
  - Updated status and guarded read paths to use specific lifecycle error kinds.
  - Added outside-workspace metadata to repo context and status outputs.
- `crates/codegraph-cli/tests/cli_smoke.rs`
  - Updated lifecycle integration expectation from generic `db_lifecycle_blocked` to specific `repo_root_mismatch`.

## Status/Profile Scripts

No standalone PowerShell profile script in `scripts/` was found that owns profile DB status output. The status-bearing surfaces now print the configured DB path and outside-workspace status directly:

- CLI `status`
- CLI `doctor --json`
- CLI read lifecycle output
- MCP `codegraph.status`
- MCP guarded read errors

## Verification

Passed:

- `cargo test -p codegraph-store preflight -- --test-threads=1`
- `cargo test -p codegraph-index lifecycle_surface_preflight -- --test-threads=1`
- `cargo test -p codegraph-cli doctor -- --test-threads=1`
- `cargo test -p codegraph-mcp-server mcp_status -- --test-threads=1`
- `cargo test -p codegraph-cli lifecycle_integration_unsafe_db_rejected_consistently_by_cli_mcp_and_status -- --test-threads=1`
- `cargo test --workspace`
- `git diff --check`

Notes:

- First `cargo test --workspace` run failed because one integration test still expected the old generic MCP error code. The code now returns `repo_root_mismatch`, which is the intended machine-readable clarity, and the focused rerun plus final workspace run passed.
- `git diff --check` passed with line-ending warnings only.
- The workspace already had many dirty source/report changes from prior lifecycle work; they were not reverted.
