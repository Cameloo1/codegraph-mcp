# Fix Branch Commit Plan

Generated: 2026-05-15T15:17:12.5438881-05:00

## Source-of-truth read

- Required first read: `MVP.md`
- Result: `MVP.md NOT FOUND`
- Impact: the required MVP timestamp could not be applied because the file is absent in this worktree.

## Input report read

- `reports/audit/full_scope_passport_verification.md`

## Branch state

- Intended branch: `fix`
- Current `git rev-parse --abbrev-ref HEAD`: `fix`
- Commit readiness note: worktree is attached to `fix`; commit from this branch after staging only the approved source, test, and intentional audit-report files.

## Source Change Summary

### Commit packaging note

- The current CLI and CLI smoke-test diff contains both the DB/scope lifecycle fix and proof-build binary timing-contract guards.
- These hunks are interleaved in shared command parsing, benchmark metadata, and smoke-test fixtures.
- To avoid unsafe partial staging, this checkpoint should be treated as one stabilization commit covering DB lifecycle/scope safety plus benchmark timing claimability.
- Generated final-gate artifacts, DBs, logs, and copied benchmark workspaces remain excluded.

### Shared preflight/lifecycle

- Added a shared DB lifecycle preflight surface around `DbLifecyclePreflight`, backed by store-level `DbPreflightReport`.
- Centralized read safety checks for repo root, schema version, storage mode, passport validity, DB integrity/freshness signals, and scope compatibility.
- Added structured preflight evidence fields for `scope_source`, `passport_scope_hash`, `explicit_scope_hash`, `scope_mismatch`, `passport_scope_policy`, `explicit_scope_policy`, blockers, warnings, and per-check statuses.
- Routed CLI read guards through the shared preflight instead of ad hoc default-scope validation.

### Non-default scope read behavior

- Read commands without explicit scope flags now use the stored passport scope and report `scope_source=passport`.
- Read commands with explicit matching scope report `scope_source=explicit`.
- Explicit incompatible scope reads fail with `scope_mismatch` and report expected passport scope hash versus observed explicit scope hash.
- CLI query/context surfaces and MCP search/analyze/status surfaces expose the lifecycle metadata in JSON.

### Stale cleanup for newly ignored paths

- Incremental update now checks whether a changed path already has indexed facts before honoring an ignore skip.
- Previously indexed files that become ignored are cleaned up with structured reasons `now_ignored` and `scope_changed`.
- Deleted files are cleaned before ignore decisions.
- Update output now reports ignored-path and cleanup counters, including `ignored_paths_seen`, `ignored_paths_with_existing_facts`, `stale_facts_deleted_for_ignored_paths`, `deleted_file_facts_removed`, and `path_cleanup_reasons`.
- Update decisions use the passport scope by default and keep repo-root mismatch as a hard safety failure.

### Include-aware pruning

- Added include-descendant reasoning with `could_include_descendant` / `could_include_descendant_decision`.
- One `--include` no longer globally disables pruning for unrelated excluded directories.
- Directory audit decisions now include directory, exclusion reason, include-pattern presence, descendant-match decision, prune decision, and reason.
- Hard excluded directories remain pruned unless the include can plausibly target inside them; complex globs keep a documented conservative fallback.

### MCP status passport gate

- `codegraph.status` now uses the same shared DB preflight as MCP read/search/analyze paths.
- Unsafe DBs return structured `db_problem`, `safe_to_query=false`, blockers, warnings, and passport/preflight summaries rather than counts-only success.
- Missing DBs report index-required/missing status.
- Safe DBs still report counts and passport metadata.

### Tests added

- Added CLI regression coverage for non-default passport scope reads, context-pack reads, explicit scope mismatch, and explicit matching scope.
- Added index/update tests for ignored-file stale cleanup, deleted-file cleanup, ignored paths without facts, passport-scope updates, and repo-root mismatch safety.
- Added pruning tests for exact include paths, node_modules/dist include targeting, unrelated excluded directory pruning, complex glob fallback, and no-default-excludes behavior.
- Added MCP status tests for safe DBs, missing DBs, repo-root mismatch, scope mismatch, schema mismatch, storage mismatch, and non-default scope reads.
- Added CLI/MCP lifecycle integration tests covering default index/read/status/search, non-default index, incompatible explicit scope, ignored-file cleanup, include-aware pruning audit, and unsafe DB consistency.

### Proof-build binary timing contract

- Benchmark outputs now expose binary metadata such as executable path, exact command, binary profile, debug assertions, and claimability.
- Debug proof-build timing is diagnostic-only and cannot produce a production threshold verdict without explicit allowance.
- Release proof-build timing is marked claimable only when produced by the release binary.

## Verification Summary

### Focused tests

From `reports/audit/focused_scope_passport_verification.md`:

- `cargo test -p codegraph-index scope` passed.
- `cargo test -p codegraph-index passport` passed.
- `cargo test -p codegraph-index db_lifecycle` passed.
- `cargo test -p codegraph-index update` passed.
- `cargo test -p codegraph-index pruning` passed.
- Literal `cargo test -p codegraph-mcp mcp_status` and `cargo test -p codegraph-mcp lifecycle` were blocked because no Cargo package named `codegraph-mcp` exists.
- Equivalent real package checks passed:
  - `cargo test -p codegraph-mcp-server mcp_status`
  - `cargo test -p codegraph-cli lifecycle_integration -- --test-threads=1`

### Manual smokes

From focused verification:

- CLI non-default index and query succeeded using `scope_source=passport`.
- CLI context-pack after non-default index succeeded using `scope_source=passport`.
- CLI update after ignore change removed stale facts for the newly ignored file.
- MCP status/search agreed on safe DBs.
- MCP status/search rejected an unsafe copied DB consistently.

### Full tests and gates

From `reports/audit/full_scope_passport_verification.md`:

- `cargo test --workspace -- --test-threads=1` passed: 424 passed, 0 failed, 3 ignored.
- `cargo build --workspace` passed.
- `cargo build --release --bin codegraph-mcp` passed.
- `git diff --check` passed with CRLF warnings only.
- `python scripts/check_readme_artifacts.py` passed.
- `python scripts/check_markdown_links.py` passed.
- Graph Truth Gate passed: 11/11.
- Context Packet Gate passed: 11/11.
- DB integrity/schema check passed.
- Default query surface smoke passed with release binary.
- PathEvidence sampler smoke passed without generated fallback.
- No quality gate was skipped.

## Proposed Commit Contents

### Source files

- `crates/codegraph-bench/src/lib.rs`
- `crates/codegraph-cli/src/lib.rs`
- `crates/codegraph-index/src/lib.rs`
- `crates/codegraph-index/src/scope.rs`
- `crates/codegraph-mcp-server/src/lib.rs`
- `crates/codegraph-store/src/lib.rs`
- `crates/codegraph-store/src/sqlite.rs`

### Test files

- `crates/codegraph-cli/tests/cli_smoke.rs`

### Small intentional fixtures

- No standalone fixture files are proposed for staging.
- New regression fixtures are generated inside tests with temporary directories.

### Docs/audit reports intentionally included

These reports were explicitly requested during the sequence and are proposed as intentional docs:

- `reports/audit/scope_passport_lifecycle_fix_baseline.md`
- `reports/audit/scope_passport_lifecycle_fix_baseline.json`
- `reports/audit/scope_passport_regression_tests.md`
- `reports/audit/scope_passport_regression_tests.json`
- `reports/audit/shared_passport_preflight_fix.md`
- `reports/audit/shared_passport_preflight_fix.json`
- `reports/audit/non_default_scope_read_fix.md`
- `reports/audit/non_default_scope_read_fix.json`
- `reports/audit/incremental_ignored_stale_cleanup_fix.md`
- `reports/audit/incremental_ignored_stale_cleanup_fix.json`
- `reports/audit/include_aware_pruning_fix.md`
- `reports/audit/include_aware_pruning_fix.json`
- `reports/audit/mcp_status_passport_gate_fix.md`
- `reports/audit/mcp_status_passport_gate_fix.json`
- `reports/audit/cli_mcp_lifecycle_integration_tests.md`
- `reports/audit/cli_mcp_lifecycle_integration_tests.json`
- `reports/audit/focused_scope_passport_verification.md`
- `reports/audit/focused_scope_passport_verification.json`
- `reports/audit/full_scope_passport_verification.md`
- `reports/audit/full_scope_passport_verification.json`
- `reports/audit/fix_branch_commit_plan.md`
- `reports/audit/fix_branch_commit_plan.json`
- `reports/audit/proof_build_binary_contract_fix.md`
- `reports/audit/proof_build_binary_contract_fix.json`

## Excluded Files

Do not stage:

- Generated DBs: `*.sqlite`, `*.sqlite-wal`, `*.sqlite-shm`, `*.db`
- Raw logs: `*.log`
- Benchmark payload directories, especially `reports/final/`, `reports/final/artifacts/`, and large run folders.
- CGC comparison/runtime artifacts, especially `reports/comparison/cgc*` payload trees.
- `target/` or crate-local `target/` directories.
- Temp gate outputs from this run under `%TEMP%\full-scope-passport-20260515-151304`.
- Any raw timing payloads or benchmark sidecars not explicitly requested as compact audit reports.
- Any report generated by local smoke/gate tooling that is not one of the intentional audit reports listed above.

Current staged files: none.

Current untracked non-ignored files are the intentional audit reports only. A generated-artifact pattern check over untracked non-ignored files found no DB/WAL/SHM/log/target/final-report/CGC artifact matches.

## Suggested Commit Message

```text
fix: harden DB lifecycle and benchmark timing guards

- reuse stored passport scope for read/update paths
- delete stale facts for now-ignored files
- make include-aware pruning avoid unrelated excluded dirs
- route MCP status through shared DB preflight
- add regression tests for scope/passport lifecycle
- mark non-release proof-build timings as diagnostic-only
```

## Pre-commit Checklist

- Confirm the worktree is still attached to `fix`.
- Stage only the proposed source, test, and intentional audit report files.
- Re-run `git diff --cached --name-only`.
- Re-run `git diff --cached --check`.
- Verify no staged path matches DB/WAL/SHM/log/target/final-report/CGC generated-artifact patterns.
- Commit with the suggested message after user review.
