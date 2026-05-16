# Lifecycle Regression Suite

Generated: 2026-05-16 00:24:10 -05:00

## Source Of Truth

- MVP.md was read first from the canonical CodeGraph checkout because this worktree does not contain an `MVP.md` file.
- Lifecycle hardening reports from the sequence were read before adding tests.
- No subagents were used.

## Added Regression Tests

### `crates/codegraph-cli/tests/cli_smoke.rs`

- `lifecycle_regression_suite_cli_query_diagnostic_and_unresolved_db_guards`
  - Valid DB normal query succeeds.
  - Mismatched repo DB normal query fails closed.
  - Mismatched repo DB diagnostic query succeeds only as `diagnostic_stale_reuse` with `claimable=false` and contamination evidence.
  - `query unresolved-calls --db <path>` records and guards the exact custom DB path.
  - `query unresolved-calls --db <mismatched-path>` fails and reports the exact checked path.

- `lifecycle_regression_suite_watch_once_external_db_uses_exact_path`
  - `watch --once --db <external>` uses the external DB.
  - The default `.codegraph/codegraph.sqlite` DB is not created or mutated when an external DB is configured.
  - Watch output records requested path, actual opened path, lifecycle status, and `auto_index_enabled=false`.

- `lifecycle_regression_suite_bundle_import_contracts_are_explicit`
  - Bundle import into a non-empty DB fails by default.
  - Foreign-repo bundle import fails by default and does not publish a DB.
  - `--replace` import is atomic, replaces the old DB, and leaves no bundle-import backup payloads.

### `crates/codegraph-cli/src/lib.rs`

- `lifecycle_regression_suite_internal_surfaces_share_guardrails`
  - Long-running watch startup uses the external DB and does not touch the default DB.
  - UI `/api/unresolved-calls` refuses a repo-mismatched default DB.
  - Doctor reports lifecycle blockers for old/unsafe DBs and does not migrate them.
  - Benchmark inspection reports old/unsafe DBs as non-claimable and does not migrate them.
  - SQLite sidecars are `normal` when the main DB exists.
  - SQLite sidecars are `orphan_without_main_db` only when the main DB is absent.
  - Exact caller mode returns exact resolved entity results for an unambiguous symbol.
  - Ambiguous caller mode lists candidate entity IDs instead of mixing exact results.

## Required Scenario Coverage

| # | Scenario | Covered By |
|---|---|---|
| 1 | Valid DB normal query succeeds | `lifecycle_regression_suite_cli_query_diagnostic_and_unresolved_db_guards` |
| 2 | Mismatched repo DB normal query fails | `lifecycle_regression_suite_cli_query_diagnostic_and_unresolved_db_guards` |
| 3 | Mismatched repo DB diagnostic query succeeds only with diagnostic label | `lifecycle_regression_suite_cli_query_diagnostic_and_unresolved_db_guards` |
| 4 | `query unresolved-calls --db` exact path is guarded | `lifecycle_regression_suite_cli_query_diagnostic_and_unresolved_db_guards` |
| 5 | UI unresolved-calls exact path is guarded | `lifecycle_regression_suite_internal_surfaces_share_guardrails` |
| 6 | `watch --once --db external` uses external | `lifecycle_regression_suite_watch_once_external_db_uses_exact_path` |
| 7 | Long-running `watch --db external` uses external | `lifecycle_regression_suite_internal_surfaces_share_guardrails` |
| 8 | Long-running watch does not touch default DB when external DB is configured | `lifecycle_regression_suite_internal_surfaces_share_guardrails` |
| 9 | Doctor does not mutate old DB | `lifecycle_regression_suite_internal_surfaces_share_guardrails` |
| 10 | Doctor reports lifecycle blockers | `lifecycle_regression_suite_internal_surfaces_share_guardrails` |
| 11 | Bundle import foreign repo fails by default | `lifecycle_regression_suite_bundle_import_contracts_are_explicit` |
| 12 | Bundle import into non-empty DB fails by default | `lifecycle_regression_suite_bundle_import_contracts_are_explicit` |
| 13 | Bundle import replace is atomic | `lifecycle_regression_suite_bundle_import_contracts_are_explicit` |
| 14 | Benchmark inspection does not mutate DB | `lifecycle_regression_suite_internal_surfaces_share_guardrails` |
| 15 | Sidecars status normal when DB exists | `lifecycle_regression_suite_internal_surfaces_share_guardrails` |
| 16 | Sidecars orphan only when main DB absent | `lifecycle_regression_suite_internal_surfaces_share_guardrails` |
| 17 | Exact caller/callee unambiguous symbol returns exact results | `lifecycle_regression_suite_internal_surfaces_share_guardrails` |
| 18 | Ambiguous caller/callee lists candidate entities | `lifecycle_regression_suite_internal_surfaces_share_guardrails` |

## Verification

- `cargo test -p codegraph-cli lifecycle_regression_suite -- --test-threads=1`
  - Result: passed.
  - Matched tests: 4.

- `cargo test --workspace`
  - Result: passed.
  - Summary: 466 passed, 3 ignored, 0 failed.

## Notes

- Tests use small temporary repositories and fixture DBs only.
- No large external corpus is required.
- No production behavior was changed for this task; changes are test coverage plus this audit report.
