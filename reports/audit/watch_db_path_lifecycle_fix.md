# Watch DB path lifecycle fix

Generated at: 2026-05-15T22:33:23-05:00

Scope: make long-running `watch --db <path>` use the supplied DB path consistently and preflight that exact DB before warming cache or applying updates.

## Source-of-truth status

- `MVP.md` is missing from this active worktree at `C:\Users\wamin\.codex\worktrees\4160\codegraph-mcp\MVP.md`.
- The canonical sibling checkout copy at `C:\Users\wamin\Desktop\development\codegraph-mcp\MVP.md` was read first for project intent.
- `reports/audit/db_lifecycle_surface_inventory.md` and `reports/audit/lifecycle_preflight_api_design.md` were read before implementation.

## Inspection result

The reported long-running watch bug was present in this checkout before the patch.

- `parse_watch_options` parsed `--db`.
- `watch --once --db` used the supplied DB path through `update_changed_files_to_db`.
- Long-running `watch --db` dropped `options.db` and called `watch_repo(&options.repo, options.debounce)`.
- `watch_repo` opened `default_db_path(&repo_root)` directly to warm the incremental cache.
- `watch_repo` also created `repo_root/.codegraph` before checking DB safety.

## Implementation

Changed `crates/codegraph-cli/src/lib.rs`:

- Added `WatchStartup` as the explicit startup state for long-running watch.
- Added `prepare_watch_startup(repo_path, requested_db_path, surface_name)`.
- Added `watch_requested_db_path`.
- Added `watch_lifecycle_status_json`.
- Added `watch_lifecycle_error`.
- Added `watch_update_lifecycle_metadata` for `watch --once` JSON evidence.
- Changed long-running `watch_repo` to accept the selected DB path.
- Changed long-running cache warmup to:
  - resolve the selected DB path
  - run `inspect_db_lifecycle_surface_preflight` with `operation_kind=write_update`
  - reject missing/unsafe DBs before any open
  - open only `actual_db_path_opened`
  - call `update_changed_files_with_cache_to_db` for event updates
- Removed unconditional `.codegraph` directory creation from long-running startup.
- Added startup stderr lifecycle logging with:
  - `requested_db_path`
  - `actual_db_path_opened`
  - `lifecycle_status`
  - `auto_index_enabled=false`

Changed `crates/codegraph-cli/tests/cli_smoke.rs`:

- Added coverage that `watch --once --db <external>` uses the external DB and does not create the default DB.

## Behavior

- `watch --once --db external.sqlite` uses `external.sqlite`.
- Long-running startup for `watch --db external.sqlite` uses `external.sqlite`.
- Long-running startup does not create or mutate default `.codegraph/codegraph.sqlite` when an external DB is configured.
- Unsafe external DBs are rejected before cache warmup.
- Missing DBs produce a clear index-first message and explicitly say long-running watch does not auto-index by default.
- No auto-index path was added.

## Verification

- `cargo fmt`: passed, with existing warning `could not canonicalize path C:\Users\wamin`.
- `cargo test -p codegraph-cli watch`: passed; matched 5 tests.
- `git diff --check`: passed, with existing CRLF normalization warnings.
- `python -m json.tool reports/audit/watch_db_path_lifecycle_fix.json`: passed.
- `cargo test --workspace`: failed on the known intermittent Windows UI socket reset in `tests::ui_server_starts_and_status_endpoint_uses_real_index`.
- `cargo test --workspace` rerun: failed on the same single UI socket reset test.
- `cargo test -p codegraph-cli tests::ui_server_starts_and_status_endpoint_uses_real_index -- --exact`: passed on rerun.

## Remaining notes

This pass intentionally fixed only the watch DB path lifecycle surface. Other inventory P0 surfaces, including `doctor` and `bundle import`, remain for separate targeted fixes.
