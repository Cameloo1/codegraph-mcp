# Focused Scope/Passport Verification

Generated: 2026-05-15T15:06:40.6583510-05:00

## Source-of-truth read

- Required first read: `MVP.md`
- Result: `MVP.md NOT FOUND`
- Follow-on impact: the required MVP task timestamp could not be applied because the file is absent in this worktree.

## Sequence reports read

- `reports/audit/scope_passport_lifecycle_fix_baseline.md`
- `reports/audit/scope_passport_regression_tests.md`
- `reports/audit/shared_passport_preflight_fix.md`
- `reports/audit/non_default_scope_read_fix.md`
- `reports/audit/incremental_ignored_stale_cleanup_fix.md`
- `reports/audit/include_aware_pruning_fix.md`
- `reports/audit/mcp_status_passport_gate_fix.md`
- `reports/audit/cli_mcp_lifecycle_integration_tests.md`

## Overall result

Focused verification is blocked on the literal requested MCP package command shape:

- `cargo test -p codegraph-mcp mcp_status` failed before test discovery because no package named `codegraph-mcp` exists in this workspace.
- `cargo test -p codegraph-mcp lifecycle` failed for the same reason.

The equivalent real package checks passed:

- `cargo test -p codegraph-mcp-server mcp_status`
- `cargo test -p codegraph-cli lifecycle_integration -- --test-threads=1`

No broad full-gate run was started after the literal focused-command blocker.

## Focused cargo commands

| Command | Result | Matched tests |
| --- | --- | --- |
| `cargo test -p codegraph-index scope` | pass | 27 passed; 63 filtered out |
| `cargo test -p codegraph-index passport` | pass | 3 passed; 87 filtered out |
| `cargo test -p codegraph-index db_lifecycle` | pass | 5 passed; 85 filtered out |
| `cargo test -p codegraph-index update` | pass | 6 passed; 84 filtered out |
| `cargo test -p codegraph-index pruning` | pass | 1 passed; 89 filtered out |
| `cargo test -p codegraph-mcp mcp_status` | fail | package resolution failed before discovery |
| `cargo test -p codegraph-mcp lifecycle` | fail | package resolution failed before discovery |
| `cargo test -p codegraph-mcp-server mcp_status` | pass | 6 passed; 14 filtered out |
| `cargo test -p codegraph-cli lifecycle_integration -- --test-threads=1` | pass | 6 passed; 51 filtered out |

Literal MCP package failure text:

```text
error: package ID specification `codegraph-mcp` did not match any packages
help: a package with a similar name exists: codegraph-cli
```

## CLI manual smoke

Executable used:

```powershell
.\target\debug\codegraph-mcp.exe
```

Fixture handling:

- Temporary repos were created under `%TEMP%`.
- Fixture text and `.gitignore` files were written as UTF-8 without BOM.
- Temp repos were removed after the smoke run.

Results:

| Scenario | Result | Evidence |
| --- | --- | --- |
| Index with non-default scope | pass | `index . --json --include-ignored --workers 1` returned `status=indexed` |
| Query symbols | pass | `query symbols manual_ignored_symbol` returned `status=ok`, `scope_source=passport`, `hits=6` |
| Context-pack | pass | `context-pack --task "manual scope smoke" --seed manual_ignored_symbol --budget 1200 --mode impact` returned `status=ok`, `scope_source=passport` |
| Update after ignore change | pass | `watch --once --changed generated/now_ignored.ts` returned `ignored_paths_seen=1`, `ignored_paths_with_existing_facts=1`, `stale_facts_deleted_for_ignored_paths=1`, cleanup reasons `now_ignored` and `scope_changed` |
| Post-update query | pass | `query symbols manual_stale_symbol` returned `hits=0` after cleanup |

Note: an earlier smoke attempt wrote `.gitignore` with PowerShell `Set-Content -Encoding UTF8`, which produced an invalid ignore-pattern fixture for this parser on this shell. That attempt was discarded and rerun with BOM-free fixture writes.

## MCP manual smoke

Executable used:

```powershell
.\target\debug\codegraph-mcp.exe serve-mcp
```

Transport:

- JSON-RPC stdio `tools/call`
- Tools invoked: `codegraph.status`, `codegraph.search`

Results:

| Scenario | Result | Evidence |
| --- | --- | --- |
| Status safe DB | pass | `status=ok`, `safe_to_query=true`, `scope_source=passport` |
| Search safe DB | pass | `isError=false`, `status=ok`, `scope_source=passport`, `hits=2` |
| Status unsafe DB | pass | copied DB into a different repo root; `status=db_problem`, `problem=repo_root_mismatch`, `safe_to_query=false`, blocker included expected and observed roots |
| Search unsafe DB | pass | `isError=true`, `status=error`, `error=db_lifecycle_blocked` |

## Working tree state after verification

`git status --short` before adding this report:

```text
 M crates/codegraph-bench/src/lib.rs
 M crates/codegraph-cli/src/lib.rs
 M crates/codegraph-cli/tests/cli_smoke.rs
 M crates/codegraph-index/src/lib.rs
 M crates/codegraph-index/src/scope.rs
 M crates/codegraph-mcp-server/src/lib.rs
 M crates/codegraph-store/src/lib.rs
 M crates/codegraph-store/src/sqlite.rs
?? reports/audit/cli_mcp_lifecycle_integration_tests.json
?? reports/audit/cli_mcp_lifecycle_integration_tests.md
?? reports/audit/include_aware_pruning_fix.json
?? reports/audit/include_aware_pruning_fix.md
?? reports/audit/incremental_ignored_stale_cleanup_fix.json
?? reports/audit/incremental_ignored_stale_cleanup_fix.md
?? reports/audit/mcp_status_passport_gate_fix.json
?? reports/audit/mcp_status_passport_gate_fix.md
?? reports/audit/non_default_scope_read_fix.json
?? reports/audit/non_default_scope_read_fix.md
?? reports/audit/scope_passport_lifecycle_fix_baseline.json
?? reports/audit/scope_passport_lifecycle_fix_baseline.md
?? reports/audit/scope_passport_regression_tests.json
?? reports/audit/scope_passport_regression_tests.md
?? reports/audit/shared_passport_preflight_fix.json
?? reports/audit/shared_passport_preflight_fix.md
```

`git diff --stat` before adding this report:

```text
crates/codegraph-bench/src/lib.rs       |    8 +-
crates/codegraph-cli/src/lib.rs         |  708 +++++++++++++++++----
crates/codegraph-cli/tests/cli_smoke.rs |  862 +++++++++++++++++++++++++-
crates/codegraph-index/src/lib.rs       | 1034 +++++++++++++++++++++++++++++--
crates/codegraph-index/src/scope.rs     |  178 ++++++
crates/codegraph-mcp-server/src/lib.rs  |  623 ++++++++++++++++++-
crates/codegraph-store/src/lib.rs       |    6 +-
crates/codegraph-store/src/sqlite.rs    |   19 +-
8 files changed, 3203 insertions(+), 235 deletions(-)
```

`git diff --check` result:

- No whitespace errors.
- Git emitted CRLF conversion warnings for modified Rust files.

## Acceptance status

- Focused index-package tests matched at least one test and passed.
- Equivalent real MCP package tests matched at least one test and passed.
- Literal `codegraph-mcp` package commands did not match any package and therefore did not pass.
- CLI manual smoke is recorded and passed.
- MCP manual smoke is recorded and passed.
- No broad full-gate run was started after the focused-command blocker.
