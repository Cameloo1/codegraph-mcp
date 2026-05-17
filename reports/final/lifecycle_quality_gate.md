# Lifecycle Quality Gate

Generated: 2026-05-16 00:36:30 -05:00

## Verdict

**PASS: DB lifecycle policy is complete for the tested lifecycle hardening surfaces.**

This is a lifecycle correctness verdict, not a production proof-build timing verdict. The graph/context/query-surface commands in this gate used the already-built debug binary and therefore their emitted binary timing metadata is diagnostic/non-threshold metadata. Correctness pass counts and lifecycle safety assertions are still recorded below.

## Source Of Truth

- `MVP.md` was requested first. It is not present in this worktree, so the canonical sibling file at `C:\Users\wamin\Desktop\development\codegraph-mcp\MVP.md` was read first.
- All lifecycle hardening reports in this sequence were reread before running the gate.
- No subagents were used.

## Artifact Directory

- `reports/final/lifecycle_quality_gate_artifacts/run_20260516_002818`

This directory contains compact gate evidence plus a small temporary fixture DB used for integrity/query-surface checks. The DB and sidecars are generated evidence, not source.

## Required Runs

| Target | Command | Result |
| --- | --- | --- |
| Workspace build | `cargo build --workspace` | pass |
| Workspace tests | `cargo test --workspace` | pass: 466 passed, 3 ignored, 0 failed |
| Graph Truth Gate | `target\debug\codegraph-mcp.exe bench graph-truth ...` | pass: 11/11 |
| Context Packet Gate | `target\debug\codegraph-mcp.exe bench context-packet ...` | pass: 11/11 |
| DB integrity | `target\debug\codegraph-mcp.exe audit schema-check --db <fixture-db>` | pass: status `ok`, user_version `20`, no failures |
| Lifecycle regression suite | `cargo test -p codegraph-cli lifecycle_regression_suite -- --test-threads=1` | pass: 4 matched, 0 failed |
| Default query surface | `target\debug\codegraph-mcp.exe bench query-surface --fresh ...` | pass: 10/10 query metrics |
| Unresolved-calls lifecycle tests | `cargo test -p codegraph-cli unresolved_calls -- --test-threads=1` | pass: 4 matched, 0 failed |
| Watch DB path tests | `cargo test -p codegraph-cli watch -- --test-threads=1` | pass: 6 matched, 0 failed |
| Doctor lifecycle tests | `cargo test -p codegraph-cli doctor -- --test-threads=1` | pass: 9 matched, 0 failed |
| Bundle import safety tests | `cargo test -p codegraph-cli bundle_import -- --test-threads=1` | pass: 7 matched, 0 failed |
| Benchmark read-only inspector tests | `cargo test -p codegraph-cli benchmark_inspection -- --test-threads=1` and `cargo test -p codegraph-cli update_integrity_report_marks_inspection_read_only_and_setup_mutations -- --test-threads=1` | pass: 2 matched, 0 failed |
| Exact caller/callee query tests | `cargo test -p codegraph-cli caller -- --test-threads=1`, `cargo test -p codegraph-cli callee -- --test-threads=1`, `cargo test -p codegraph-mcp-server mcp_call_relation -- --test-threads=1` | pass: 5 matched, 0 failed |
| Status/sidecar tests | `cargo test -p codegraph-cli sidecar -- --test-threads=1` and `cargo test -p codegraph-mcp-server mcp_status -- --test-threads=1` | pass: 8 matched, 0 failed |
| External profile DB error tests | `cargo test -p codegraph-index external_profile -- --test-threads=1` and `cargo test -p codegraph-store preflight -- --test-threads=1` | pass: 5 matched, 0 failed |

## Pass Criteria

| Criterion | Status | Evidence |
| --- | --- | --- |
| cargo build pass | pass | `cargo build --workspace` |
| cargo test pass | pass | `cargo test --workspace` |
| Graph Truth Gate 11/11 | pass | `graph_truth.json`: `cases_passed=11`, `cases_total=11`, `cases_failed=0` |
| Context Packet Gate 11/11 | pass | `context_packet.json`: `cases_passed=11`, `cases_total=11`, `cases_failed=0`; metrics include proof path coverage `1.0`, source span coverage `1.0`, critical snippet coverage `1.0` |
| DB integrity ok | pass | `db_integrity_schema_check.json`: `status=ok`, `user_version=20`, `failures={}` |
| no normal read path can query unsafe DB | pass | lifecycle integration suite, unresolved-calls exact path tests, MCP status/search lifecycle tests |
| no `--db` override bypasses lifecycle | pass | `unresolved_calls_custom_db_preflights_exact_path`, custom mismatched DB rejection, watch external DB tests |
| long-running watch honors `--db` | pass | `watch_long_running_startup_uses_external_db_without_touching_default` and lifecycle regression suite |
| doctor is read-only and lifecycle-aware | pass | doctor old-schema/no-migration, missing passport, repo mismatch, scope mismatch tests |
| bundle import is atomic and repo-identity safe | pass | non-empty default failure, foreign repo default failure, replace atomic tests |
| benchmark inspection is read-only or explicitly marked mutation/setup | pass | benchmark inspection lifecycle and update-integrity read-only/setup tests |
| sidecar status terminology is correct | pass | normal sidecar and orphan-without-main DB tests for status/doctor |
| exact caller/callee mode works | pass | exact unambiguous caller, entity-id caller/callee, ambiguous candidates, MCP call relation tests |
| no docs/text evidence is mislabeled graph proof | pass | Code search found `text_evidence`/`repo-text` only in the docs/text plan report, not in implemented graph proof surfaces; docs text profile remains plan-only |
| external profile DB errors are correctly classified | pass | external profile path and store preflight classification tests |

## Generated State

Current gate generated:

- `reports/final/lifecycle_quality_gate_artifacts/run_20260516_002818/graph_truth.json`
- `reports/final/lifecycle_quality_gate_artifacts/run_20260516_002818/context_packet.json`
- `reports/final/lifecycle_quality_gate_artifacts/run_20260516_002818/db_integrity_schema_check.json`
- `reports/final/lifecycle_quality_gate_artifacts/run_20260516_002818/default_query_surface.json`
- small fixture DB and WAL/SHM sidecars under `reports/final/lifecycle_quality_gate_artifacts/run_20260516_002818/fixture_repo/.codegraph/`
- one query-surface generated DB under `reports/audit/artifacts/default_query_surface_1778909351227.sqlite`

These are generated evidence payloads. They should not be staged unless explicitly selected for final-report evidence.

## Final Statement

No target failed. The DB lifecycle policy is complete for the lifecycle hardening scope covered by this sequence: shared preflight safety, non-default scope reads, stale ignored cleanup, include-aware pruning, MCP status passport gating, exact DB path guarding, watch external DB handling, doctor read-only inspection, safe bundle import semantics, benchmark read-only inspection, sidecar terminology, exact caller/callee separation, docs/text proof separation, and external profile DB error classification.
