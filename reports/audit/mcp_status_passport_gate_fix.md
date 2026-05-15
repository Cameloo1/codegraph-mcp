# MCP Status Passport Gate Fix

Generated: 2026-05-15T14:20:59.0050136-05:00

## Source preflight

`MVP.md` is still absent in this worktree:

```text
Test-Path MVP.md
MVP.md NOT FOUND

.\.codex-tools\rg.exe --files | .\.codex-tools\rg.exe "(^|/)MVP\.md$"
<no output; exit 1>
```

Required reports read:

- `reports/audit/shared_passport_preflight_fix.md`
- `reports/audit/scope_passport_regression_tests.md`

No `MVP.md` timestamp was written because the file does not exist in this checkout.

## Goal

Fix P2: MCP status must not bypass the passport gate.

Before this pass, the status path had already been routed through shared DB lifecycle preflight, but the public status contract was still not strict enough:

- missing DB status used the older `not_indexed` shape
- status did not expose `safe_to_query`
- top-level blockers/warnings were not consistently present
- passport data was only available through the full preflight payload, not a compact status summary
- status did not classify blocker type as `repo_root_mismatch`, `scope_mismatch`, `schema_mismatch`, or `storage_mismatch`

## Implementation

Changed file:

- `crates/codegraph-mcp-server/src/lib.rs`

Status behavior now:

- uses `inspect_db_lifecycle_preflight` before opening the SQLite store
- returns `status: "missing"` with `problem: "index_required"` and `db_problem: "db_missing"` when the DB file is absent
- returns `status: "db_problem"` and `safe_to_query: false` when preflight is unsafe
- includes top-level `blockers`, `warnings`, `scope_source`, `db_lifecycle_read`, and `passport_summary`
- classifies unsafe DBs as:
  - `repo_root_mismatch`
  - `scope_mismatch`
  - `schema_mismatch`
  - `storage_mismatch`
  - `passport_missing`
  - `passport_corrupt`
  - `passport_invalid`
- only opens SQLite for counts after preflight says the DB is safe
- returns `status: "ok"` and `safe_to_query: true` for safe DBs, with counts and passport metadata

Added helper functions:

- `mcp_db_problem_kind`
- `mcp_passport_summary_json`

Updated the MCP status output schema to include:

- `safe_to_query`
- `blockers`
- `warnings`
- `passport_summary`
- `scope_source`
- `db_lifecycle_read`
- `problem`

## Tests added

Added focused MCP status tests:

- `mcp_status_safe_db_reports_counts_passport_and_scope_source`
- `mcp_status_missing_db_reports_index_required`
- `mcp_status_passport_gate_reports_db_problem_for_mismatched_db`
- `mcp_status_scope_mismatch_reports_scope_mismatch_blocker`
- `mcp_status_schema_mismatch_reports_schema_blocker`
- `mcp_status_storage_mismatch_reports_storage_blocker`

The repo-root mismatch test also verifies search rejects the same unsafe DB, so status agrees with read-tool safety decisions.

## Validation

Requested command:

```text
cargo test -p codegraph-mcp mcp_status
```

Result:

```text
error: package ID specification `codegraph-mcp` did not match any packages
help: a package with a similar name exists: `codegraph-cli`
```

The MCP crate in this workspace is `codegraph-mcp-server`, so the focused package test was run there:

```text
cargo test -p codegraph-mcp-server mcp_status
ok: 6 passed; 0 failed; 0 ignored; 14 filtered out
```

Lifecycle focused test:

```text
cargo test -p codegraph-index db_lifecycle
ok: 5 passed; 0 failed; 0 ignored; 85 filtered out
```

Full MCP server package:

```text
cargo test -p codegraph-mcp-server
ok: 20 passed; 0 failed; 0 ignored
```

Workspace gate:

```text
cargo test --workspace
failed: tests::ui_server_starts_and_status_endpoint_uses_real_index
read HTTP response: Os { code: 10054, kind: ConnectionReset, message: "An existing connection was forcibly closed by the remote host." }
```

The failing exact test was rerun:

```text
cargo test -p codegraph-cli tests::ui_server_starts_and_status_endpoint_uses_real_index -- --exact
ok: 1 passed; 0 failed; 0 ignored
```

Interpretation: MCP status focused coverage is green, but the latest authoritative `cargo test --workspace` command is not green because the same documented UI-server connection-reset test failed in `codegraph-cli`. Cargo stopped before later workspace crates in that full run.

## Dirty/generated state

Pre-existing dirty source files are still present from the broader lifecycle/benchmark work:

- `crates/codegraph-bench/src/lib.rs`
- `crates/codegraph-cli/src/lib.rs`
- `crates/codegraph-cli/tests/cli_smoke.rs`
- `crates/codegraph-index/src/lib.rs`
- `crates/codegraph-index/src/scope.rs`
- `crates/codegraph-mcp-server/src/lib.rs`
- `crates/codegraph-store/src/lib.rs`
- `crates/codegraph-store/src/sqlite.rs`

This pass intentionally changes:

- `crates/codegraph-mcp-server/src/lib.rs`
- `reports/audit/mcp_status_passport_gate_fix.md`
- `reports/audit/mcp_status_passport_gate_fix.json`

No benchmark DBs, SQLite sidecars, raw logs, CGC artifacts, `target/` payloads, or generated benchmark payloads are intended for commit as part of this pass.

## Acceptance status

- MCP status no longer bypasses passport preflight.
- Unsafe DB status is visible but marked non-queryable.
- Counts are only reported for safe DBs.
- Missing DB status is `missing` with `index_required`/`db_missing`, not success.
- Repo-root, scope, schema, and storage blockers are classified.
- Safe DB status includes counts, passport summary, and `scope_source`.
- MCP status agrees with search safety on the repo-root mismatch fixture.
- `cargo test --workspace` remains failed due to the documented UI-server connection reset.
