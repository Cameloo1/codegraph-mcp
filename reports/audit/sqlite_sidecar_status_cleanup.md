# SQLite Sidecar Status Cleanup

Generated at: 2026-05-15T23:31:41-05:00

Scope: clarify WAL/SHM sidecar terminology in lifecycle preflight, CLI status, CLI doctor, and MCP status output. No lifecycle/passport gate was weakened.

## Source-Of-Truth Status

- `MVP.md` is missing from this active worktree at `C:\Users\wamin\.codex\worktrees\4160\codegraph-mcp\MVP.md`.
- The canonical sibling copy at `C:\Users\wamin\Desktop\development\codegraph-mcp\MVP.md` was read before changes.
- `reports/audit/db_lifecycle_surface_inventory.md` was read before changes.

## Problem

The shared DB preflight reported expected SQLite WAL/SHM sidecars through a field named `orphan_sidecars`. That made valid DBs with normal live WAL/SHM files look suspicious even when the main DB existed and lifecycle/passport checks passed.

## Changes

- `crates/codegraph-store/src/sqlite.rs`
  - Added `sqlite_sidecars` and `sidecar_status` to `DbPreflightReport`.
  - Kept `orphan_sidecars` as a deprecated compatibility field.
  - `orphan_sidecars` is now populated only when sidecars exist without the matching main DB.
  - Sidecar status values now include:
    - `normal`
    - `orphan_without_main_db`
    - `stale_cleanup_candidate`
    - `unexpected` reserved for future abnormal classifier cases
- `crates/codegraph-cli/src/lib.rs`
  - CLI `status` now reports `sqlite_sidecars` and top-level `sidecar_status`.
  - CLI `doctor` now reports the same sidecar object and top-level `sidecar_status`.
  - Lifecycle JSON includes `sqlite_sidecars`, `sidecar_status`, deprecated `orphan_sidecars`, and `orphan_sidecars_deprecated=true`.
- `crates/codegraph-mcp-server/src/lib.rs`
  - MCP `status` now reports `sqlite_sidecars` and top-level `sidecar_status`.
  - MCP lifecycle JSON includes the same sidecar terminology.

## Compatibility

The old `orphan_sidecars` field remains present for existing consumers, but its semantics are corrected: it no longer lists normal WAL/SHM files when the main DB exists.

## Tests

Added CLI regression tests:

- `status_and_doctor_report_normal_sqlite_sidecars_for_valid_db`
  - Builds a valid indexed DB.
  - Holds a live WAL/SHM sidecar connection open.
  - Verifies CLI `status` and `doctor` both report `sidecar_status=normal`.
  - Verifies deprecated `orphan_sidecars` is empty for the valid DB.
- `status_and_doctor_report_orphan_without_main_db_for_sidecars_only`
  - Creates WAL/SHM files without the main DB.
  - Verifies CLI `status` and `doctor` both report `sidecar_status=orphan_without_main_db`.

Existing MCP status tests continue to pass with the new fields.

## Verification

- `cargo fmt`: passed, with existing warning `could not canonicalize path C:\Users\wamin`.
- `cargo test -p codegraph-cli sidecar`: passed, 2 tests matched.
- `cargo test -p codegraph-mcp-server mcp_status`: passed, 6 tests matched.
- `cargo test --workspace`: passed.
- `git diff --check`: passed, with CRLF normalization warnings only.
- `python -m json.tool reports/audit/sqlite_sidecar_status_cleanup.json`: passed.

## Dirty State

The active worktree already contained broad uncommitted lifecycle/source/report changes before this pass. This pass added only the sidecar terminology changes, regression tests, and this report pair. No DB files, WAL/SHM files, raw logs, `target/`, benchmark payloads, or CGC artifacts were intentionally added.
