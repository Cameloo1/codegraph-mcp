# Unresolved-calls lifecycle fix

Generated at: 2026-05-15T22:25:33-05:00

Scope: ensure `query unresolved-calls --db <path>`, the Proof-Path UI unresolved-calls route, and the query-surface benchmark unresolved-calls probe preflight the exact DB path they open.

## Source-of-truth status

- `MVP.md` is missing from this active worktree at `C:\Users\wamin\.codex\worktrees\4160\codegraph-mcp\MVP.md`.
- The canonical sibling checkout copy at `C:\Users\wamin\Desktop\development\codegraph-mcp\MVP.md` was read first for project intent.
- `reports/audit/db_lifecycle_surface_inventory.md` and `reports/audit/lifecycle_preflight_api_design.md` were read before implementation.

## Inspection result

The reported bug was present in this checkout before the patch.

- The outer `query` command preflighted `default_db_path(current_repo_root)`.
- `query unresolved-calls` parsed its own `--db` argument and opened that path directly.
- `/api/unresolved-calls` called the same helper outside the top-level query guard.
- The query-surface benchmark invoked unresolved-calls with a custom DB path but did not record lifecycle status for that probe.
- The `--source-scan` branch opened `SqliteGraphStore` on the same DB path after the read-only unresolved-calls open.

## Implementation

Changed `crates/codegraph-cli/src/lib.rs`:

- Imported the shared surface preflight primitive from `codegraph-index`.
- Added exact-path unresolved-calls lifecycle helpers:
  - `unresolved_calls_lifecycle_preflight`
  - `require_unresolved_calls_lifecycle_preflight`
  - `db_lifecycle_surface_preflight_json`
- Extended `UnresolvedCallsOptions` with:
  - `allow_stale_read`
  - `explicit_scope_policy`
  - `surface_name`
  - `operation_kind`
- Changed `run_query_command` so `unresolved-calls` is guarded by its own final parsed DB path instead of the outer default DB path.
- Changed `query_unresolved_calls` to preflight before both SQLite read-only open and the optional `--source-scan` store open.
- Added unresolved-calls lifecycle metadata to output:
  - `db_lifecycle_read`
  - `claimable`
  - `diagnostic_only`
  - `exact_db_path_checked`
- Changed `/api/unresolved-calls` to inherit the same preflight through `query_unresolved_calls`.
- Changed `benchmark_unresolved_calls_surface_query` to record `db_lifecycle_read` with `operation_kind=benchmark_inspection`.

## Behavior

- `query unresolved-calls --db <valid-db>` now succeeds even when no default DB exists, because the helper preflights the explicit DB path it will open.
- `query unresolved-calls --db <mismatched-db>` fails closed in normal mode.
- `query unresolved-calls --db <mismatched-db> --allow-stale-read` succeeds only as diagnostic output with `claimable=false`, `diagnostic_only=true`, lifecycle warnings, and the exact DB path checked.
- UI unresolved-calls refuses unsafe DBs instead of reading counts from an unguarded SQLite open.
- Query-surface benchmark unresolved-calls metrics carry lifecycle evidence.

## Verification

- `cargo fmt`: passed, with existing warning `could not canonicalize path C:\Users\wamin`.
- `cargo test -p codegraph-cli unresolved_calls`: passed; matched 4 tests.
- `cargo test -p codegraph-cli unresolved_calls_custom_db_preflights_exact_path`: passed.
- `cargo test -p codegraph-cli unresolved_calls_custom_mismatched_db_fails_without_diagnostic_flag`: passed.
- `cargo test -p codegraph-cli unresolved_calls_query_is_bounded_and_instrumented`: passed.
- `cargo test -p codegraph-cli unresolved_calls_ui_endpoint_refuses_unsafe_db`: passed.
- `cargo test -p codegraph-cli bench_query_surface_measures_default_compact_proof_queries`: passed.
- `git diff --check`: passed, with existing CRLF normalization warnings.
- `cargo test --workspace`: failed on unrelated existing/dirty proof-build validation test `bench_proof_build_validated_marks_artifact_validated_only_after_integrity_gate`.
- Exact rerun of that failing test also failed; current observed `proof_build_only_ms` was nonzero while the stale assertion expected `0`.

## Remaining notes

This pass intentionally fixed only unresolved-calls exact-path lifecycle behavior. Other P0 surfaces from the inventory, such as `doctor`, persistent `watch`, and `bundle import`, remain for separate targeted fixes.
