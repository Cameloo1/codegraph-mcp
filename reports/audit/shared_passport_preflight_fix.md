# Shared Passport Preflight Fix

Generated: 2026-05-15T13:03:50.5164924-05:00

Purpose: route CLI reads, MCP reads, MCP status, and update paths through one DB lifecycle/passport preflight path that understands stored passport scope policy.

## Source preflight

`MVP.md` is still absent in this worktree:

```text
Test-Path MVP.md
False

.\.codex-tools\rg.exe --files | .\.codex-tools\rg.exe "(^|/)MVP\.md$"
<no output; exit 1>
```

Read: `reports/audit/scope_passport_regression_tests.md`.

## Shared component

Added shared component in `crates/codegraph-index/src/lib.rs`:

- `DbLifecyclePreflight`
- `inspect_db_lifecycle_preflight`
- `require_db_lifecycle_preflight`

The preflight returns structured fields:

- `safe`
- `blockers`
- `warnings`
- `repo_root_status`
- `schema_status`
- `storage_mode_status`
- `scope_status`
- `scope_source`
- `passport_scope_policy`
- `explicit_scope_policy`
- `effective_scope_policy`
- `db_health`

Read-side default behavior now uses the stored passport scope policy:

- no explicit scope flags: `scope_source = "passport"`
- explicit scope flags: compare explicit policy to passport policy
- old/missing `scope_policy_json`: compatibility default only when the stored hash matches default scope; otherwise unsafe/rebuild required

## Refactored paths

CLI:

- `status` uses `inspect_db_lifecycle_preflight`
- `query` parses read scope flags and uses shared preflight before opening
- `context-pack` uses shared preflight
- `open_existing_store` uses shared preflight
- read evidence now surfaces `scope_source`, status fields, blockers, warnings, and scope policies

MCP:

- normal read tools use shared preflight through `open_store`
- `status` uses the same preflight
- unsafe MCP status returns `db_problem` and blockers instead of direct counts

Update:

- `update_changed_files_with_cache_to_db` uses `require_db_lifecycle_preflight`
- update baseline scope comes from `effective_scope_policy`
- newly ignored existing paths delete stale DB facts before the ignore skip is counted
- repo-root mismatch remains a hard safety failure through the shared preflight

Scope traversal:

- added include-aware directory pruning so an include pattern only descends into an excluded directory when it can plausibly match inside that directory

## Regression outcomes

Previously ignored regression scaffolds are now enabled and passing:

- `passport_non_default_scope_read_preflight_uses_stored_scope_policy`
- `passport_non_default_scope_query_symbols_uses_passport_scope`
- `passport_non_default_scope_context_pack_uses_passport_scope`
- `passport_explicit_incompatible_scope_read_flags_reject_mismatch`
- `update_newly_ignored_file_deletes_stale_facts_before_ignore_skip`
- `scope_include_pattern_does_not_descend_unrelated_excluded_directories`
- `mcp_status_passport_gate_reports_db_problem_for_mismatched_db`

## Focused test results

```text
cargo test -p codegraph-index passport
ok: 2 passed; 0 failed; 0 ignored

cargo test -p codegraph-index scope
ok: 20 passed; 0 failed; 0 ignored

cargo test -p codegraph-index db_lifecycle
ok: 5 passed; 0 failed; 0 ignored

cargo test -p codegraph-index update
ok: 3 passed; 0 failed; 0 ignored

cargo test -p codegraph-mcp-server mcp_status
ok: 1 passed; 0 failed; 0 ignored

cargo test -p codegraph-cli passport
ok: 4 passed; 0 failed; 0 ignored
```

The requested command below does not match a package in this workspace:

```text
cargo test -p codegraph-mcp mcp_status
error: package ID specification `codegraph-mcp` did not match any packages
help: a package with a similar name exists: `codegraph-cli`
```

The MCP package is `codegraph-mcp-server`, and that focused command passed.

## Workspace result

Required command:

```text
cargo test --workspace
```

Result: failed at the same UI-server connection reset seen in the baseline and regression scaffold reports.

```text
test tests::ui_server_starts_and_status_endpoint_uses_real_index ... FAILED

read HTTP response: Os { code: 10054, kind: ConnectionReset, message: "An existing connection was forcibly closed by the remote host." }
```

The workspace run reached:

```text
codegraph_bench: 55 passed; 0 failed; 1 ignored
codegraph_cli lib: 42 passed; 1 failed; 0 ignored
```

Cargo stopped at `codegraph-cli`, so later workspace crates were not reached in that exact full command.

Targeted rerun:

```text
cargo test -p codegraph-cli tests::ui_server_starts_and_status_endpoint_uses_real_index -- --exact
ok: 1 passed; 0 failed; 0 ignored
```

Interpretation: the full workspace command is still not green, but the failure is the same isolated UI-server connection-reset behavior documented before this implementation.

## Dirty/generated state

Content diff stat currently reports the intended files:

```text
crates/codegraph-cli/src/lib.rs
crates/codegraph-cli/tests/cli_smoke.rs
crates/codegraph-index/src/lib.rs
crates/codegraph-index/src/scope.rs
crates/codegraph-mcp-server/src/lib.rs
```

`git status --short` also lists `crates/codegraph-bench/src/lib.rs`, `crates/codegraph-store/src/lib.rs`, and `crates/codegraph-store/src/sqlite.rs`, but `git diff` for those files is empty after removing formatting churn. `git update-index --refresh` could not refresh stat-only state because the linked worktree gitdir index lock is outside the writable sandbox.

`git diff --check` exited successfully with only CRLF normalization warnings.
