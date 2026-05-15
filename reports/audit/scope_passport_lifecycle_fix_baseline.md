# Scope/Passport Lifecycle Fix Baseline

Generated: 2026-05-15T12:41:38.6118067-05:00

Purpose: freeze the current state before changing lifecycle, scope, or passport behavior. This report is observational only; no production code behavior was changed for this baseline.

## Source-of-truth preflight

`MVP.md` was requested as the source of truth, but it is not present in this worktree.

Commands run:

```text
Test-Path MVP.md
False

.\.codex-tools\rg.exe --files | .\.codex-tools\rg.exe "(^|/)MVP\.md$"
<no output; exit 1>
```

Consequence: no task-completion timestamp could be written to `MVP.md`. The latest four-issue problem report text was taken from the user prompt for this run. A repo search for the exact issue phrases did not find an in-repo problem report file with those terms.

## Current git baseline

HEAD: `4fb6c936f37354581f16a8e0b39c7b91b81e216d`

Branch state:

```text
## HEAD (no branch)
 M crates/codegraph-cli/src/lib.rs
 M crates/codegraph-cli/tests/cli_smoke.rs
```

Required status command:

```text
git status --short
 M crates/codegraph-cli/src/lib.rs
 M crates/codegraph-cli/tests/cli_smoke.rs
```

Required diff stat:

```text
git diff --stat
 crates/codegraph-cli/src/lib.rs         | 484 +++++++++++++++++++++++++++++---
 crates/codegraph-cli/tests/cli_smoke.rs | 277 +++++++++++++++++-
 2 files changed, 713 insertions(+), 48 deletions(-)
warning: in the working copy of 'crates/codegraph-cli/src/lib.rs', LF will be replaced by CRLF the next time Git touches it
warning: in the working copy of 'crates/codegraph-cli/tests/cli_smoke.rs', LF will be replaced by CRLF the next time Git touches it
```

Required whitespace check:

```text
git diff --check
warning: in the working copy of 'crates/codegraph-cli/src/lib.rs', LF will be replaced by CRLF the next time Git touches it
warning: in the working copy of 'crates/codegraph-cli/tests/cli_smoke.rs', LF will be replaced by CRLF the next time Git touches it
```

`git diff --check` exited successfully; only CRLF normalization warnings were printed.

## Dirty and generated state

Tracked dirty files already present before this baseline:

- `crates/codegraph-cli/src/lib.rs`
- `crates/codegraph-cli/tests/cli_smoke.rs`

No untracked files were present before creating this baseline report. Ignored generated directories observed:

```text
git status --short --ignored=matching .codegraph target reports
!! .codegraph/
!! target/
```

Generated files that must stay out of commits unless intentionally committed:

- `target/`
- `.codegraph/`
- benchmark payload directories under `reports/final/artifacts/<run>/`
- SQLite DB files, WAL files, and SHM files
- raw logs and raw timing payloads
- CGC comparison artifacts unless explicitly selected as compact summary evidence

## Known issues to guard

1. Non-default scope indexes can become unreadable because read preflight expects `IndexOptions::default()` scope hash.
2. Newly ignored paths can keep stale facts because changed-file update skips existing ignored paths instead of deleting prior DB facts for paths known to be stale.
3. Supplying any `--include` disables directory pruning for all excluded directories because excluded directories are only pruned when `has_include_patterns()` is false.
4. MCP `status` bypasses the passport gate by opening the SQLite store directly after context says the DB is indexed.

## Affected crates and modules

- `crates/codegraph-index/src/scope.rs`: `IndexScopeOptions`, include/exclude evaluation, scope policy hash inputs.
- `crates/codegraph-index/src/lib.rs`: `IndexOptions`, DB lifecycle policy, manifest diff, stale cleanup, full index lifecycle, changed-file update lifecycle.
- `crates/codegraph-store/src/sqlite.rs`: DB passport schema, expected passport fields, read-only preflight validation, integrity checks.
- `crates/codegraph-cli/src/lib.rs`: CLI status/read helpers, scope option parsing, update entry points, audit/bench direct DB readers.
- `crates/codegraph-mcp-server/src/lib.rs`: MCP read helpers, `status`, `index_repo`, `update_changed_files`.
- `crates/codegraph-cli/tests/cli_smoke.rs` and crate-local tests: current regression-test surface for benchmark metadata and CLI lifecycle behavior.

## Suspected CLI read paths

- `run_status_command` calls `inspect_read_db_passport` before opening the store, so CLI status is gated.
- `inspect_read_db_passport` first checks `IndexOptions::default()` and only retries with the passport storage mode on storage-mode mismatch. It does not adapt to the stored non-default scope policy, so non-default scope DBs remain suspect.
- `read_db_lifecycle_guard` refuses invalid DBs unless `CODEGRAPH_ALLOW_STALE_READ`/`--allow-stale-read` diagnostic mode is used.
- `open_existing_store` uses `read_db_lifecycle_guard` before opening the default DB, so normal CLI read tools are gated but inherit the default-scope mismatch risk.
- Direct `SqliteGraphStore::open` calls in doctor/audit/integrity/benchmark commands should be reviewed individually so diagnostic commands stay explicit and claimable read paths stay passport-gated.

## Suspected MCP read paths

- Normal MCP read tools call `McpServer::open_store`, which uses `inspect_repo_db_passport(&repo_root, &db_path, &IndexOptions::default())` before opening the DB.
- Because MCP `open_store` uses default `IndexOptions`, it is also suspect for non-default scope DB readability.
- MCP `status` is the known bypass: when `context.indexed` is true it calls `SqliteGraphStore::open(&db_path)` directly and returns schema/counts without passport preflight.

## Suspected update paths

- `update_changed_files_to_db` and `update_changed_files_with_cache_to_db` require `require_reusable_db_passport(&repo_root, &db_path, &IndexOptions::default())`, so updates against non-default scope DBs are suspect.
- The changed-file update loop calls `should_ignore_path(&repo_root, file_path)` with default scope and immediately continues for ignored existing files; this can leave old facts for paths that newly became ignored.
- Full indexing builds `stale_cleanup_paths` by comparing existing DB files to the current scoped path set. If an explicit incompatible scope excludes a path, that path can look stale even though it is scope-excluded rather than physically stale.
- Stale deletion calls `delete_indexed_files_by_path_to_writer`, which invokes `delete_facts_for_file` for each path. Fixes must keep this precise and must not delete facts for merely excluded paths unless the DB actually contains stale facts for that path under the intended lifecycle semantics.

## Suspected status path

- CLI status is guarded by `inspect_read_db_passport` and returns `db_problem` instead of opening an unsafe DB.
- MCP status bypasses the guard at `crates/codegraph-mcp-server/src/lib.rs:871` by opening the DB directly.

## Current DB passport fields

Current stored passport fields:

- `passport_version`
- `codegraph_schema_version`
- `storage_mode`
- `index_scope_policy_hash`
- `scope_policy_json`
- `canonical_repo_root`
- `git_remote`
- `worktree_root`
- `repo_head`
- `source_discovery_policy_version`
- `codegraph_build_version`
- `last_successful_index_timestamp`
- `last_completed_run_id`
- `last_run_status`
- `integrity_gate_result`
- `files_seen`
- `files_indexed`
- `created_at_unix_ms`
- `updated_at_unix_ms`

Expected passport fields used for preflight:

- `canonical_repo_root`
- `storage_mode`
- `index_scope_policy_hash`
- `git_remote`
- `worktree_root`

Current preflight also performs read-only SQLite open, `quick_check`, foreign-key check, schema version validation, passport table/row validation, last-run completion validation, integrity result validation, and orphan-sidecar reporting.

## Current IndexOptions/default behavior

`IndexOptions::default()` currently means:

- `profile = false`
- `json = false`
- `worker_count = None`
- `storage_mode = proof`
- `build_mode = proof-build-only`
- `scope = IndexScopeOptions::default()`
- `db_lifecycle.policy = safe-auto`
- `db_lifecycle.explicit_db_path = false`

`IndexScopeOptions::default()` currently means:

- `include_ignored = false`
- `no_default_excludes = false`
- `respect_gitignore = true`
- `include_patterns = []`
- `exclude_patterns = []`
- `explain_scope = false`
- `print_included = false`
- `print_excluded = false`

CLI index parsing can set `--include-ignored`, `--include`, `--exclude`, `--no-default-excludes`, `--respect-gitignore`, `--explain-scope`, `--print-included`, and `--print-excluded`.

## Test gate result

Required command:

```text
cargo test --workspace
```

Result: failed.

Observed failure:

```text
test tests::ui_server_starts_and_status_endpoint_uses_real_index ... FAILED

thread 'tests::ui_server_starts_and_status_endpoint_uses_real_index' panicked at crates\codegraph-cli\src\lib.rs:14671:14:
read HTTP response: Os { code: 10054, kind: ConnectionReset, message: "An existing connection was forcibly closed by the remote host." }

error: test failed, to rerun pass `-p codegraph-cli --lib`
```

Prior test binary in the same workspace run completed with:

```text
55 passed; 0 failed; 1 ignored
```

The failing CLI test binary completed with:

```text
42 passed; 1 failed; 0 ignored
```

Targeted rerun command:

```text
cargo test -p codegraph-cli tests::ui_server_starts_and_status_endpoint_uses_real_index -- --exact
```

Targeted rerun result: passed.

```text
test tests::ui_server_starts_and_status_endpoint_uses_real_index ... ok
1 passed; 0 failed; 0 ignored; 42 filtered out
```

Baseline interpretation: the required full workspace gate is not green because the exact `cargo test --workspace` invocation failed once. The isolated rerun suggests the failing UI-server test may be flaky on this machine, but this report does not hide the full-gate failure.

## Guardrails for the next fix

- Do not broaden the Rust refactor beyond lifecycle/scope/passport surfaces.
- Do not remove PowerShell scripts as cleanup.
- Do not change benchmark thresholds.
- Do not hide benchmark or test failures.
- Do not weaken Graph Truth, Context Packet, DB integrity, source-span, provenance, stale-update, or context-quality gates.
- Do not disable passport checks or add unsafe read acceptance.
- Do not bypass repo-root, schema, storage, integrity, or passport safety checks.
- Preserve scope semantics explicitly; any incompatibility must be reported, not silently changed.
- Debug/diagnostic outputs can be recorded, but claimable production verdicts must remain strict.
