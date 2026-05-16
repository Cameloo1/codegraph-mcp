# Incremental Ignored Stale Cleanup Fix

Generated: 2026-05-15T13:29:23.6897071-05:00

## Source-of-truth preflight

`MVP.md` is still absent in this worktree, so no MVP task timestamp could be written.

```text
Test-Path MVP.md
False
```

Read before implementation:

- `reports/audit/scope_passport_regression_tests.md`
- `reports/audit/shared_passport_preflight_fix.md`

## Fix Summary

P1 fixed: incremental update now inspects existing DB facts for each changed path before an ignore skip can happen.

The update loop now does this ordering:

1. Normalize changed path.
2. Read whether the path already has indexed DB facts.
3. If the file is deleted or not a file, cleanup facts with reason `deleted` before any ignore decision.
4. If the path is ignored and has existing facts, cleanup facts with reasons `now_ignored` and `scope_changed`.
5. If the path is ignored and has no facts, skip normally.
6. If the path is live and indexable, replacement cleanup uses reason `replaced` before re-indexing.

Update scope still comes from shared passport preflight by default. Repo-root mismatch remains a hard failure before cleanup.

## Implementation

Updated `crates/codegraph-index/src/lib.rs`:

- Added `path_has_indexed_facts`.
- Added `cleanup_facts_for_path`.
- Added cleanup reasons:
  - `deleted`
  - `now_ignored`
  - `scope_changed`
  - `replaced`
- Centralized stale fact deletion through `delete_facts_for_file`, which already removes:
  - file record
  - entities and edges
  - source spans
  - `file_entities`
  - `file_edges`
  - `file_source_spans`
  - related PathEvidence/materialized lookup rows

Added structured update output fields to `IncrementalIndexSummary`:

- `ignored_paths_seen`
- `ignored_paths_with_existing_facts`
- `stale_facts_deleted_for_ignored_paths`
- `deleted_file_facts_removed`
- `path_cleanup_reasons`

Updated `crates/codegraph-cli/src/lib.rs` detailed update JSON to include the same fields.

Also removed the watch-path prefilter that skipped ignored paths before the shared update loop could inspect existing DB facts. Watch events now feed paths into the same update cleanup gate; ignored paths with no facts are still skipped by the update loop.

## Regression Coverage

Tests added or tightened:

- `update_newly_ignored_file_deletes_stale_facts_before_ignore_skip`
- `audit_deleted_file_removes_stale_entities_and_edges`
- `update_ignored_path_without_existing_facts_is_skipped_without_cleanup`
- `update_uses_non_default_passport_scope_for_ignored_path`
- `update_rejects_repo_root_mismatch_before_cleanup`
- `watch_once_reindexes_changed_file_and_prunes_stale_facts`

## Validation

```text
cargo test -p codegraph-index update
ok: 6 passed; 0 failed

cargo test -p codegraph-index db_lifecycle
ok: 5 passed; 0 failed

cargo test -p codegraph-bench graph_truth
ok: 19 passed; 0 failed

cargo test -p codegraph-bench context_packet
ok: 4 passed; 0 failed

cargo test -p codegraph-cli watch_once
ok: 1 integration test passed

cargo test --workspace
first run failed on known flaky UI-server connection reset:
tests::ui_server_starts_and_status_endpoint_uses_real_index

cargo test -p codegraph-cli tests::ui_server_starts_and_status_endpoint_uses_real_index -- --exact
ok: 1 passed; 0 failed

cargo test --workspace
final rerun passed workspace

git diff --check
ok: no whitespace errors; CRLF normalization warnings only
```

## Dirty / Generated State

Current dirty state before this report:

```text
 M crates/codegraph-bench/src/lib.rs
 M crates/codegraph-cli/src/lib.rs
 M crates/codegraph-cli/tests/cli_smoke.rs
 M crates/codegraph-index/src/lib.rs
 M crates/codegraph-index/src/scope.rs
 M crates/codegraph-mcp-server/src/lib.rs
 M crates/codegraph-store/src/lib.rs
 M crates/codegraph-store/src/sqlite.rs
?? reports/audit/non_default_scope_read_fix.json
?? reports/audit/non_default_scope_read_fix.md
?? reports/audit/scope_passport_lifecycle_fix_baseline.json
?? reports/audit/scope_passport_lifecycle_fix_baseline.md
?? reports/audit/scope_passport_regression_tests.json
?? reports/audit/scope_passport_regression_tests.md
?? reports/audit/shared_passport_preflight_fix.json
?? reports/audit/shared_passport_preflight_fix.md
```

No DB, WAL, SHM, raw log, CGC payload, or `target/` payload is intended for commit.
