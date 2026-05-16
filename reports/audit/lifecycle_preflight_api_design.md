# Lifecycle preflight API design

Generated at: 2026-05-15T22:14:42-05:00

Scope: add one reusable DB lifecycle preflight primitive that validates the exact DB path a command intends to open. This pass does not refactor every CLI, MCP, UI, benchmark, or import command to call it.

## Source-of-truth status

- `MVP.md` is missing from this active worktree at `C:\Users\wamin\.codex\worktrees\4160\codegraph-mcp\MVP.md`.
- The canonical sibling checkout copy at `C:\Users\wamin\Desktop\development\codegraph-mcp\MVP.md` was read first for project intent.
- `reports/audit/db_lifecycle_surface_inventory.md` was read and used as the surface inventory for this design.
- The active worktree already had uncommitted source/test/report changes before this pass. This pass added only the shared primitive, its direct tests, and this audit report pair.

## API added

The new primitive lives in `crates/codegraph-index/src/lib.rs`:

- `DbLifecycleOperationKind`
- `DbLifecycleSurfacePreflightRequest`
- `DbLifecycleSurfacePreflight`
- `inspect_db_lifecycle_surface_preflight(request)`

It wraps the existing `inspect_db_lifecycle_preflight` passport/scope logic and adds command-surface semantics without changing broad user-facing command behavior.

## Request contract

`DbLifecycleSurfacePreflightRequest` records the exact surface decision:

- `repo_root`
- `db_path`
- `surface_name`
- `operation_kind`
- `allow_stale_read`
- `allow_foreign_repo`
- `required_storage_mode`
- `expected_scope`

`operation_kind` is one of:

- `normal_read`
- `diagnostic_read`
- `write_update`
- `import_replace`
- `import_merge`
- `benchmark_inspection`
- `benchmark_setup`

## Response contract

`DbLifecycleSurfacePreflight` returns:

- `safe_to_read`
- `safe_to_write`
- `claimable`
- `diagnostic_only`
- `passport_status`
- `repo_match`
- `scope_match`
- `schema_status`
- `storage_mode_match`
- `blockers`
- `warnings`
- `exact_db_path_checked`
- `repo_root_expected`
- `repo_root_observed`
- `artifact_freshness`
- `scope_source`
- `passport_scope_hash`
- `explicit_scope_hash`
- `lifecycle_preflight`

`exact_db_path_checked` is copied from the underlying read-only preflight report for the normalized path passed into this API. It is not derived from `default_db_path`.

## Operation semantics

`normal_read` fails closed unless the existing lifecycle/passport preflight is safe and any requested storage mode matches.

`diagnostic_read` is always `diagnostic_only=true` and `claimable=false`. It can downgrade repo-root blockers only when `allow_foreign_repo=true`, and stale/passport blockers only when `allow_stale_read=true`. It still blocks missing DBs, corrupt DBs, schema-incompatible DBs, explicit scope mismatches, and storage requirement mismatches.

`write_update` requires the normal lifecycle preflight to be safe. Diagnostic allowances are ignored and surfaced as warnings, so write/update remains stricter than diagnostic reads.

`import_replace` has explicit replace semantics and can be safe to write when the target DB is missing or the existing DB is lifecycle-safe. Pre-import claimability is true only when the existing DB was already safe.

`import_merge` is strict and requires a lifecycle-safe existing DB.

`benchmark_inspection` uses the same read-only lifecycle primitive. Safe artifacts are claimable; unsafe artifacts can only become diagnostic/non-claimable when explicit stale or foreign allowances downgrade the relevant blockers.

`benchmark_setup` can write a missing target or lifecycle-safe existing target, but is diagnostic/non-claimable until a post-setup artifact proves safe.

## Tests added

Focused tests were added in `codegraph-index`:

- `lifecycle_surface_preflight_valid_db_normal_read_passes_exact_path`
- `lifecycle_surface_preflight_missing_passport_normal_read_fails`
- `lifecycle_surface_preflight_mismatched_repo_normal_read_fails_closed`
- `lifecycle_surface_preflight_mismatched_repo_diagnostic_read_is_nonclaimable`
- `lifecycle_surface_preflight_write_update_is_stricter_than_diagnostic_read`

These prove:

- valid DB `normal_read` passes
- missing passport `normal_read` fails
- mismatched repo `normal_read` fails even with foreign allowance present
- mismatched repo `diagnostic_read` passes only with explicit foreign allowance and stays non-claimable
- `write_update` is stricter than diagnostic read
- `exact_db_path_checked` equals the exact custom DB path supplied to the API

## Verification

- `cargo fmt`: passed, with existing warning `could not canonicalize path C:\Users\wamin`.
- `cargo test -p codegraph-index lifecycle_surface_preflight`: passed, 5 tests matched and passed.
- `python -m json.tool reports/audit/lifecycle_preflight_api_design.json`: passed.
- `git diff --check`: passed, with existing CRLF normalization warnings for already dirty files.
- `cargo test --workspace`: failed on `tests::ui_server_starts_and_status_endpoint_uses_real_index` with Windows socket error `Os { code: 10054, kind: ConnectionReset, message: "An existing connection was forcibly closed by the remote host." }`.
- `cargo test -p codegraph-cli tests::ui_server_starts_and_status_endpoint_uses_real_index -- --exact`: passed on rerun.

The workspace failure matches the pre-existing failure documented in `db_lifecycle_surface_inventory.md`; it is recorded here rather than fixed because this prompt is scoped to the reusable primitive.

## Behavior-change boundary

No broad command behavior was refactored in this pass. Existing command surfaces will need follow-up patches to call `inspect_db_lifecycle_surface_preflight` with their actual DB path and operation kind, especially the P0 surfaces from `db_lifecycle_surface_inventory.md`.
