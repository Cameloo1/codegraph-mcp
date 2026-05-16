# Scope/Passport Regression Test Scaffold

Generated: 2026-05-15T12:51:41.9020333-05:00

Purpose: add regression tests for the four known lifecycle/scope/passport issues before implementation changes. This pass changed test scaffolding and audit reports only.

## Source preflight

`MVP.md` is still absent in this worktree:

```text
Test-Path MVP.md
False

.\.codex-tools\rg.exe --files | .\.codex-tools\rg.exe "(^|/)MVP\.md$"
<no output; exit 1>
```

Read baseline: `reports/audit/scope_passport_lifecycle_fix_baseline.md`.

Latest scope/passport lifecycle problem report: no separate in-repo report with the exact four-issue phrasing was found. The operative problem report for this scaffold is the current user prompt plus the baseline report.

## Test files changed

- `crates/codegraph-index/src/lib.rs`
- `crates/codegraph-cli/tests/cli_smoke.rs`
- `crates/codegraph-mcp-server/src/lib.rs`

No production implementation behavior was changed outside test modules/integration tests.

## Regression coverage

### A. Non-default scope CLI read

Test:

- `passport_non_default_scope_query_symbols_uses_passport_scope`
- File: `crates/codegraph-cli/tests/cli_smoke.rs`
- Status: ignored known-failing test.

Coverage:

- Creates a temp repo with `.gitignore` normally ignoring `ignored.ts`.
- Indexes with `--include-ignored`.
- Verifies the DB passport records a non-default scope hash.
- Expects `query symbols ignored_scope_symbol` to succeed.
- Expects `db_lifecycle_read.scope_source = "passport"`.
- Expects no unsafe DB rejection.

Additional lower-level index test:

- `passport_non_default_scope_read_preflight_uses_stored_scope_policy`
- File: `crates/codegraph-index/src/lib.rs`
- Status: ignored known-failing test.

### B. Non-default scope context-pack/read

Test:

- `passport_non_default_scope_context_pack_uses_passport_scope`
- File: `crates/codegraph-cli/tests/cli_smoke.rs`
- Status: ignored known-failing test.

Coverage:

- Reuses the non-default-scope DB shape.
- Runs `context-pack` seeded by the ignored-file symbol.
- Expects success using `db_lifecycle_read.scope_source = "passport"`.

### C. Explicit incompatible scope rejection

Tests:

- `passport_explicit_incompatible_scope_read_flags_reject_mismatch`
- File: `crates/codegraph-cli/tests/cli_smoke.rs`
- Status: ignored known-skipped test because CLI read commands do not currently expose explicit read-scope flags.

- `db_lifecycle_explicit_incompatible_scope_reports_scope_mismatch`
- File: `crates/codegraph-index/src/lib.rs`
- Status: enabled passing lower-level guard.

Coverage:

- Builds a DB with a non-default scope.
- Lower-level passport preflight with an incompatible scope reports `index scope policy hash mismatch`.
- CLI read-scope rejection remains a skipped placeholder until read commands expose explicit scope flags.

### D. Incremental cleanup for newly ignored file

Test:

- `update_newly_ignored_file_deletes_stale_facts_before_ignore_skip`
- File: `crates/codegraph-index/src/lib.rs`
- Status: ignored known-failing test.

Coverage:

- Indexes `generated/now_ignored.ts` while it is included.
- Writes `.gitignore` so the path becomes ignored.
- Runs incremental update for that path.
- Expects old file/entities to be deleted before ignore skip.

### E. Include-aware pruning

Test:

- `scope_include_pattern_does_not_descend_unrelated_excluded_directories`
- File: `crates/codegraph-index/src/lib.rs`
- Status: ignored known-failing test.

Coverage:

- Creates `target/`, `node_modules/`, `.git/`, `dist/`, and one included source path.
- Runs scope collection with `include_patterns = ["src/keep.ts"]`.
- Expects unrelated excluded trees to remain pruned, not merely traversed and excluded file-by-file.

### F. MCP status passport gate

Test:

- `mcp_status_passport_gate_reports_db_problem_for_mismatched_db`
- File: `crates/codegraph-mcp-server/src/lib.rs`
- Status: ignored known-failing test.

Coverage:

- Creates a safe indexed DB, then mutates its passport root to be mismatched.
- Confirms MCP search rejects the DB through `db_lifecycle_blocked`.
- Expects MCP status to report `db_problem`/passport blockers and not return direct counts.

## Focused test commands

The suggested package `codegraph-mcp` does not exist in this workspace; the MCP package is `codegraph-mcp-server`.

```text
cargo test -p codegraph-index scope
ok: 18 passed; 0 failed; 2 ignored; matched tests

cargo test -p codegraph-index passport
ok: 1 passed; 0 failed; 1 ignored; matched tests

cargo test -p codegraph-index db_lifecycle
ok: 5 passed; 0 failed; 0 ignored; matched tests

cargo test -p codegraph-index update
ok: 2 passed; 0 failed; 1 ignored; matched tests

cargo test -p codegraph-mcp-server mcp_status
ok: 0 passed; 0 failed; 1 ignored; matched tests

cargo test -p codegraph-cli passport
ok: 1 passed; 0 failed; 3 ignored; matched tests
```

## Workspace result

Required command:

```text
cargo test --workspace
```

Result: failed.

Observed failure is the same UI-server connection reset documented in the baseline:

```text
test tests::ui_server_starts_and_status_endpoint_uses_real_index ... FAILED

read HTTP response: Os { code: 10054, kind: ConnectionReset, message: "An existing connection was forcibly closed by the remote host." }
```

The failing workspace run reached:

```text
codegraph_bench: 55 passed; 0 failed; 1 ignored
codegraph_cli lib: 42 passed; 1 failed; 0 ignored
```

Cargo stopped at `codegraph-cli`, so later workspace crates were not reached in that full run. Targeted rerun:

```text
cargo test -p codegraph-cli tests::ui_server_starts_and_status_endpoint_uses_real_index -- --exact
ok: 1 passed; 0 failed; 0 ignored
```

Interpretation: the required workspace gate remains not green because the exact `cargo test --workspace` command failed. The isolated rerun passed again, consistent with the baseline observation that this UI-server test is likely flaky on this machine.

## Dirty/generated state

Current dirty source/test files:

- `crates/codegraph-cli/src/lib.rs` (pre-existing proof-build metric work)
- `crates/codegraph-cli/tests/cli_smoke.rs` (pre-existing plus this scaffold)
- `crates/codegraph-index/src/lib.rs` (this scaffold)
- `crates/codegraph-mcp-server/src/lib.rs` (this scaffold)

Untracked audit reports:

- `reports/audit/scope_passport_lifecycle_fix_baseline.md`
- `reports/audit/scope_passport_lifecycle_fix_baseline.json`
- `reports/audit/scope_passport_regression_tests.md`
- `reports/audit/scope_passport_regression_tests.json`

`git diff --check` exited successfully with only CRLF normalization warnings.
