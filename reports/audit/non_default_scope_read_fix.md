# Non-Default Scope Read Fix

Generated: 2026-05-15T13:19:42.2665369-05:00

## Source-of-truth preflight

`MVP.md` is still absent in this worktree, so no MVP task timestamp could be written.

```text
Test-Path MVP.md
False

.\.codex-tools\rg.exe --files | .\.codex-tools\rg.exe "(^|/)MVP\.md$"
<no output; exit 1>
```

Read before implementation:

- `reports/audit/shared_passport_preflight_fix.md`

## Fix Summary

P1 fixed: DBs built with non-default scope options remain readable by default read paths when the DB passport is valid.

Implemented behavior:

- Default read commands use the stored passport scope policy instead of `IndexOptions::default()`.
- Default read JSON reports `scope_source = "passport"`.
- Explicit matching read scope flags compare cleanly against the passport and report `scope_source = "explicit"`.
- Explicit incompatible scope flags fail with structured `scope_mismatch` details and the message `DB passport scope does not match explicit requested scope`.
- Read evidence now exposes `passport_scope_hash`, `explicit_scope_hash`, and `scope_mismatch` where applicable.

## Code Paths Updated

Shared preflight:

- `crates/codegraph-index/src/lib.rs`
  - `DbLifecyclePreflight`
  - `ScopeMismatchDetails`
  - `inspect_db_lifecycle_preflight`

CLI read paths:

- `query symbols`, `query text`, and relation query subcommands through `run_query_command`
- `context-pack`
- `impact`

MCP read/status paths:

- `codegraph.search`
- `codegraph.search_symbols`
- `codegraph.search_text`
- `codegraph.search_semantic`
- `codegraph.analyze`
- `codegraph.context_pack`
- `codegraph.plan_context`
- relation/path/impact/explain read helpers that open the graph store
- `codegraph.status`

## Regression Coverage

Added or tightened tests for:

- CLI `query symbols` after `index --include-ignored`
- CLI `context-pack` after `index --include-ignored`
- explicit matching CLI scope flags returning `scope_source = "explicit"`
- explicit incompatible CLI scope flags rejecting with scope mismatch details
- lifecycle preflight exposing structured `ScopeMismatchDetails`
- MCP `search`, `analyze`, and `status` after a non-default-scope index

## Validation

```text
cargo test -p codegraph-index passport
ok: 2 passed; 0 failed

cargo test -p codegraph-index db_lifecycle
ok: 5 passed; 0 failed

cargo test -p codegraph-index scope
ok: 20 passed; 0 failed

cargo test -p codegraph-cli passport
ok: 4 integration tests passed; lib/main filtered test bins matched 0 tests

cargo test -p codegraph-mcp-server mcp_status
ok: 1 passed; 0 failed

cargo test -p codegraph-mcp-server non_default_scope
ok: 1 passed; 0 failed

cargo test --workspace
ok: workspace passed

git diff --check
ok: no whitespace errors; CRLF normalization warnings only
```

## Dirty / Generated State

Current `git status --short` before this report was written:

```text
 M crates/codegraph-bench/src/lib.rs
 M crates/codegraph-cli/src/lib.rs
 M crates/codegraph-cli/tests/cli_smoke.rs
 M crates/codegraph-index/src/lib.rs
 M crates/codegraph-index/src/scope.rs
 M crates/codegraph-mcp-server/src/lib.rs
 M crates/codegraph-store/src/lib.rs
 M crates/codegraph-store/src/sqlite.rs
?? reports/audit/scope_passport_lifecycle_fix_baseline.json
?? reports/audit/scope_passport_lifecycle_fix_baseline.md
?? reports/audit/scope_passport_regression_tests.json
?? reports/audit/scope_passport_regression_tests.md
?? reports/audit/shared_passport_preflight_fix.json
?? reports/audit/shared_passport_preflight_fix.md
```

No DB, WAL, SHM, raw log, CGC payload, or `target/` payload is intended for commit. The untracked audit reports are human/machine audit sidecars from this lifecycle/passport work.
