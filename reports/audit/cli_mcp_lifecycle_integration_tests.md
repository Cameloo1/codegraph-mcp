# CLI/MCP Lifecycle Integration Tests

Generated: 2026-05-15T14:32:59.6903141-05:00

## Source preflight

`MVP.md` is still absent in this worktree:

```text
Test-Path MVP.md
MVP.md NOT FOUND

.\.codex-tools\rg.exe --files | .\.codex-tools\rg.exe "(^|/)MVP\.md$"
<no output; exit 1>
```

Required reports read:

- `reports/audit/non_default_scope_read_fix.md`
- `reports/audit/incremental_ignored_stale_cleanup_fix.md`
- `reports/audit/include_aware_pruning_fix.md`
- `reports/audit/mcp_status_passport_gate_fix.md`

No `MVP.md` timestamp was written because the file does not exist in this checkout.

## Goal

Add integration smoke tests proving CLI and MCP lifecycle behavior is consistent across default reads, non-default scope reads, explicit scope mismatch, ignored-file stale cleanup, include-aware pruning, and unsafe DB handling.

## Implementation

Changed files for this pass:

- `crates/codegraph-cli/src/lib.rs`
- `crates/codegraph-cli/tests/cli_smoke.rs`
- `crates/codegraph-mcp-server/src/lib.rs`
- `reports/audit/cli_mcp_lifecycle_integration_tests.md`
- `reports/audit/cli_mcp_lifecycle_integration_tests.json`

Integration tests were added to `crates/codegraph-cli/tests/cli_smoke.rs` because that suite can invoke the real `codegraph-mcp` CLI binary and instantiate `codegraph_mcp_server::McpServer` against the same temporary DB.

New tests:

- `lifecycle_integration_default_cli_query_mcp_status_and_search_share_passport`
- `lifecycle_integration_non_default_scope_cli_and_mcp_use_passport_scope`
- `lifecycle_integration_explicit_incompatible_scope_rejected_by_cli_and_mcp`
- `lifecycle_integration_newly_ignored_cleanup_removes_cli_and_mcp_hits`
- `lifecycle_integration_include_aware_pruning_audit_prunes_unrelated_dirs`
- `lifecycle_integration_unsafe_db_rejected_consistently_by_cli_mcp_and_status`

Small supporting behavior change:

- MCP read/status paths now parse explicit read-scope arguments:
  - `include_ignored`
  - `includeIgnored`
  - `no_default_excludes`
  - `noDefaultExcludes`
  - `respect_gitignore`
  - `respectGitignore`
  - `include` / `includes` / `include_patterns`
  - `exclude` / `excludes` / `exclude_patterns`
- Explicit MCP scope is passed into `inspect_db_lifecycle_preflight`, matching the CLI read-scope gate.
- CLI explicit scope mismatch errors now include the `scope_mismatch:` prefix while preserving the existing detailed mismatch message.

## Scenario coverage

1. Default index -> CLI query -> MCP status -> MCP search:
   - all succeed
   - passport scope hash is consistent across CLI query, MCP status, and MCP search

2. Non-default index with `--include-ignored` -> CLI query -> MCP status -> MCP search:
   - all succeed
   - `scope_source = "passport"`
   - MCP status remains `safe_to_query = true`

3. Explicit incompatible read scope:
   - CLI query with `--exclude ignored.ts` rejects with `scope_mismatch`
   - MCP search with `exclude: ["ignored.ts"]` rejects with `scope_mismatch`
   - MCP status with the same explicit scope returns `status = "db_problem"` and `problem = "scope_mismatch"`

4. Newly ignored file cleanup:
   - indexed symbol exists in CLI and MCP search
   - `.gitignore` changes so the path is ignored
   - `watch --once --changed ...` deletes stale facts before skip
   - CLI query and MCP search no longer return the stale symbol
   - MCP status remains safe

5. Include-aware pruning:
   - CLI index with `--include src/keep.ts --explain-scope`
   - `target/`, `node_modules/`, `.git/`, and gitignored `dist/` are pruned
   - `scope.directory_prune_decisions` records `could_include_descendant = false`, `pruned = true`, and an `excluded_directory_pruned` reason

6. Unsafe DB:
   - DB passport copied from one repo into another repo
   - CLI query rejects with repo-root mismatch
   - MCP search rejects with `db_lifecycle_blocked`
   - MCP status returns `db_problem` / `repo_root_mismatch`
   - MCP status does not report counts

## Manual repro command logs

Default lifecycle:

```powershell
cargo run --bin codegraph-mcp -- index <repo> --json --workers 1
cargo run --bin codegraph-mcp -- query symbols sanitize
# MCP in-process equivalent:
# codegraph.status { "repo": "<repo>" }
# codegraph.search { "repo": "<repo>", "query": "sanitize" }
```

Non-default passport scope:

```powershell
cargo run --bin codegraph-mcp -- index <repo> --json --include-ignored --workers 1
cargo run --bin codegraph-mcp -- query symbols ignored_scope_symbol
# MCP:
# codegraph.status { "repo": "<repo>" }
# codegraph.search { "repo": "<repo>", "query": "ignored_scope_symbol" }
```

Explicit incompatible read scope:

```powershell
cargo run --bin codegraph-mcp -- query symbols ignored_scope_symbol --exclude ignored.ts
# MCP:
# codegraph.search { "repo": "<repo>", "query": "ignored_scope_symbol", "exclude": ["ignored.ts"] }
# codegraph.status { "repo": "<repo>", "exclude": ["ignored.ts"] }
```

Newly ignored cleanup:

```powershell
cargo run --bin codegraph-mcp -- index <repo> --json --workers 1
cargo run --bin codegraph-mcp -- query symbols stale_generated_symbol
# write .gitignore containing: generated/
cargo run --bin codegraph-mcp -- watch --once --changed generated/now_ignored.ts
cargo run --bin codegraph-mcp -- query symbols stale_generated_symbol
# MCP:
# codegraph.search { "repo": "<repo>", "query": "stale_generated_symbol" }
# codegraph.status { "repo": "<repo>" }
```

Include-aware pruning:

```powershell
cargo run --bin codegraph-mcp -- index <repo> --json --include src/keep.ts --explain-scope --workers 1
# Inspect .scope.directory_prune_decisions for target, node_modules, .git, and gitignored dist.
```

Unsafe DB:

```powershell
cargo run --bin codegraph-mcp -- index <repo-a> --json --workers 1
Copy-Item <repo-a>\.codegraph\codegraph.sqlite <repo-b>\.codegraph\codegraph.sqlite
cargo run --bin codegraph-mcp -- query symbols sanitize
# MCP:
# codegraph.search { "repo": "<repo-b>", "query": "sanitize" }
# codegraph.status { "repo": "<repo-b>" }
```

## Validation

Focused lifecycle integration command:

```text
cargo test -p codegraph-cli lifecycle_integration -- --test-threads=1
ok: 6 passed; 0 failed; 0 ignored; 51 filtered out
```

MCP status focused guard after explicit-scope parser change:

```text
cargo test -p codegraph-mcp-server mcp_status -- --test-threads=1
ok: 6 passed; 0 failed; 0 ignored; 14 filtered out
```

DB lifecycle focused guard:

```text
cargo test -p codegraph-index db_lifecycle -- --test-threads=1
ok: 5 passed; 0 failed; 0 ignored; 85 filtered out
```

Required workspace command:

```text
cargo test --workspace -- --test-threads=1
ok: workspace passed
```

Key observed package/test counts from the workspace run:

- `codegraph_bench`: 55 passed, 1 ignored
- `codegraph_cli` lib: 43 passed
- `cli_smoke`: 57 passed
- `codegraph_index`: 89 passed, 1 ignored
- `codegraph_mcp_server`: 20 passed
- remaining parser/query/store/trace/vector/core tests passed

## Dirty/generated state

Current dirty source files include pre-existing lifecycle/benchmark work plus this integration pass:

- `crates/codegraph-bench/src/lib.rs`
- `crates/codegraph-cli/src/lib.rs`
- `crates/codegraph-cli/tests/cli_smoke.rs`
- `crates/codegraph-index/src/lib.rs`
- `crates/codegraph-index/src/scope.rs`
- `crates/codegraph-mcp-server/src/lib.rs`
- `crates/codegraph-store/src/lib.rs`
- `crates/codegraph-store/src/sqlite.rs`

This pass intentionally added:

- `reports/audit/cli_mcp_lifecycle_integration_tests.md`
- `reports/audit/cli_mcp_lifecycle_integration_tests.json`

No benchmark DBs, SQLite sidecars, raw logs, CGC artifacts, `target/` payloads, or generated benchmark payloads are intended for commit as part of this pass.

## Acceptance status

- CLI and MCP agree on DB safety.
- Non-default scope DBs are readable by default through both surfaces.
- Explicit incompatible scope is rejected by both surfaces.
- Stale ignored facts are deleted and disappear from both CLI and MCP search.
- Include-aware pruning is verified through CLI scope audit output.
- Unsafe DBs remain rejected, and MCP status does not present counts as usable.
- `cargo test --workspace -- --test-threads=1` passed.
