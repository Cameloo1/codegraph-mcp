# Include-Aware Pruning Fix

Generated: 2026-05-15T14:13:07.4242831-05:00

## Source preflight

`MVP.md` is still absent in this worktree:

```text
Test-Path MVP.md
MVP.md NOT FOUND
```

The required regression-test report was read:

- `reports/audit/scope_passport_regression_tests.md`

No `MVP.md` timestamp was written because the file does not exist in this checkout.

## Goal

Fix P2: a single `--include` pattern must not globally disable pruning for excluded directories.

The walker should prune excluded directories by default, then descend only when an include pattern could plausibly match something inside that directory. Complex globs remain conservative, but a simple include such as `src/special/generated.rs` no longer forces traversal of unrelated trees like `target/`, `node_modules/`, `.git/`, `dist/`, or generated artifact folders.

## Implementation

Changed files for this fix:

- `crates/codegraph-index/src/scope.rs`
- `crates/codegraph-index/src/lib.rs`

Key implementation points:

- Added `could_include_descendant(directory, include_patterns)` and `could_include_descendant_decision(...)`.
- Handles exact paths, prefix-like path intersections, `/**` glob prefixes, literal-prefix glob patterns, Windows path separators, Linux path separators, and platform case sensitivity.
- Keeps a conservative fallback for complex globs without a safe literal directory prefix.
- Added `IndexScopeDirectoryPruneDecision` to the scope runtime report.
- The walker now records per-directory audit decisions:
  - `directory`
  - `excluded_by`
  - `include_patterns_present`
  - `could_include_descendant`
  - `pruned`
  - `reason`
- The walker prunes excluded directories unless the include-descendant decision says a pattern can plausibly match inside.

Important code locations from this worktree:

- `crates/codegraph-index/src/scope.rs:349` stores `directory_prune_decisions`.
- `crates/codegraph-index/src/scope.rs:404` defines `IndexScopeDirectoryPruneDecision`.
- `crates/codegraph-index/src/scope.rs:607` defines `could_include_descendant_decision`.
- `crates/codegraph-index/src/lib.rs:8234` applies the decision while walking excluded directories.
- `crates/codegraph-index/src/lib.rs:8247` records the prune/descend audit decision.

## Test coverage

Added or tightened tests:

- `could_include_descendant_handles_exact_prefix_and_separators`
- `could_include_descendant_uses_glob_literal_prefixes`
- `could_include_descendant_documents_complex_glob_fallback`
- `scope_include_pattern_does_not_descend_unrelated_excluded_directories`
- `scope_include_pattern_descends_only_needed_node_modules_subtree`
- `scope_include_pattern_descends_soft_dist_only_when_pattern_can_match`
- `scope_complex_glob_conservative_descend_is_audited`
- `generated_junk_include_pruning_keeps_traversal_bounded`

Coverage maps to the requested cases:

- One include under `src/` does not traverse `target/`.
- One include under `src/` does not traverse `node_modules/`.
- Include targeting `node_modules/pkg/file.js` descends into `node_modules/` and `node_modules/pkg/`, but prunes unrelated `node_modules/other/`.
- Include targeting `dist/foo.js` descends into `dist/` only when that include can match inside it.
- Complex glob fallback is explicit and audited with `complex_glob_conservative_descend`.
- `no_default_excludes` remains the opt-out path for default pruning.
- Generated-junk fixture confirms exact includes no longer evaluate as many paths as the unpruned `no_default_excludes` traversal.

## Commands run

Focused scope tests:

```text
cargo test -p codegraph-index scope
ok: 27 passed; 0 failed; 0 ignored; 63 filtered out
```

Workspace gate current validation, first run:

```text
cargo test --workspace
failed: tests::ui_server_starts_and_status_endpoint_uses_real_index
read HTTP response: Os { code: 10054, kind: ConnectionReset, message: "An existing connection was forcibly closed by the remote host." }
```

Targeted rerun of the failed flaky test:

```text
cargo test -p codegraph-cli tests::ui_server_starts_and_status_endpoint_uses_real_index -- --exact
ok: 1 passed; 0 failed
```

Workspace gate current validation, second full run:

```text
cargo test --workspace
failed: tests::ui_server_starts_and_status_endpoint_uses_real_index
read HTTP response: Os { code: 10054, kind: ConnectionReset, message: "An existing connection was forcibly closed by the remote host." }
```

Interpretation: the latest authoritative `cargo test --workspace` result is not green. The failure is the same UI-server connection-reset test previously documented in lifecycle reports, and the isolated exact test passes, but the workspace command itself currently fails and is recorded as such. Cargo stops in `codegraph-cli`, so later workspace crates are not reached in the latest full run.

Diff hygiene:

```text
git diff --check
ok: no whitespace errors; CRLF normalization warnings only
```

## Dirty/generated state

Dirty source files present before writing this report:

- `crates/codegraph-bench/src/lib.rs`
- `crates/codegraph-cli/src/lib.rs`
- `crates/codegraph-cli/tests/cli_smoke.rs`
- `crates/codegraph-index/src/lib.rs`
- `crates/codegraph-index/src/scope.rs`
- `crates/codegraph-mcp-server/src/lib.rs`
- `crates/codegraph-store/src/lib.rs`
- `crates/codegraph-store/src/sqlite.rs`

Untracked audit reports already present before this report:

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

This report adds:

- `reports/audit/include_aware_pruning_fix.md`
- `reports/audit/include_aware_pruning_fix.json`

No benchmark DBs, SQLite sidecars, raw logs, CGC artifacts, `target/` payloads, or generated benchmark report payloads are intended for commit as part of this fix.

## Acceptance status

- Any `--include` no longer globally disables excluded-directory pruning.
- Walker traversal count drops on the generated-junk fixture compared with `no_default_excludes`.
- Explicit includes into excluded trees still work when the include pattern can plausibly match inside them.
- Hard-excluded directories remain pruned unless the include path specifically targets their subtree.
- Scope audit output now records the include-aware prune/descend decision.
- `cargo test --workspace` currently fails on the documented UI-server connection reset; the focused pruning tests pass and the exact failed test passes in isolation.
