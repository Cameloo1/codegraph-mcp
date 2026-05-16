# Full Scope/Passport Verification

Generated: 2026-05-15T15:14:40.4424130-05:00

## Source-of-truth read

- Required first read: `MVP.md`
- Result: `MVP.md NOT FOUND`
- Impact: the required MVP timestamp could not be applied because the file is absent in this worktree.

## Input report read

- `reports/audit/focused_scope_passport_verification.md`

## Required verification

| Command | Result | Notes |
| --- | --- | --- |
| `cargo test --workspace -- --test-threads=1` | pass | 424 passed, 0 failed, 3 ignored |
| `cargo build --workspace` | pass | dev workspace build completed |
| `cargo build --release --bin codegraph-mcp` | pass | release production binary built |
| `git diff --check` | pass | no whitespace errors; Git emitted CRLF conversion warnings |

Cargo emitted the existing warning `could not canonicalize path C:\Users\wamin` during Cargo commands, but each required Cargo command exited successfully.

## Docs checks

Docs/audit reports were touched, and both optional scripts are present.

| Command | Result | Notes |
| --- | --- | --- |
| `python scripts/check_readme_artifacts.py` | pass | 12 report paths checked |
| `python scripts/check_markdown_links.py` | pass | 32 local links checked, 0 external links skipped |

## Quality gates

Gate artifacts were written outside the repo under:

```text
C:\Users\wamin\AppData\Local\Temp\full-scope-passport-20260515-151304
```

This keeps benchmark payloads, temp DBs, and raw gate outputs out of the working tree.

| Gate | Command | Result | Notes |
| --- | --- | --- | --- |
| Graph Truth Gate | `.\target\debug\codegraph-mcp.exe bench graph-truth --cases benchmarks\graph_truth\fixtures --fixture-root . --out-json <temp>\graph_truth.json --out-md <temp>\graph_truth.md --fail-on-forbidden --fail-on-missing-source-span --fail-on-unresolved-exact --fail-on-derived-without-provenance --fail-on-test-mock-production-leak --update-mode` | pass | 11/11 cases passed; debug binary marked diagnostic/non-threshold |
| Context Packet Gate | `.\target\debug\codegraph-mcp.exe bench context-packet --cases benchmarks\graph_truth\fixtures --fixture-root . --out-json <temp>\context_packet.json --out-md <temp>\context_packet.md --top-k 10` | pass | 11/11 cases passed; proof path coverage 1.0; source span coverage 1.0; debug binary marked diagnostic/non-threshold |
| DB integrity | `.\target\release\codegraph-mcp.exe index . --json --db <temp>\codegraph.sqlite --workers 4 --build-mode proof-build-plus-validation` then `audit schema-check --db <temp>\codegraph.sqlite` | pass | validated build produced temp DB; schema-check `status=ok`, `failure_count=0`, `user_version=20` |
| Default query surface smoke | `.\target\release\codegraph-mcp.exe bench query-surface --repo . --db <temp>\codegraph.sqlite --iterations 1 --out-json <temp>\query_surface.json --out-md <temp>\query_surface.md` | pass | `status=passed`; release binary; `claimable_for_thresholds=true` |
| PathEvidence sampler smoke | `.\target\release\codegraph-mcp.exe audit sample-paths --db <temp>\codegraph.sqlite --limit 5 --seed 1 --json <temp>\sample_paths.json --markdown <temp>\sample_paths.md --include-snippets --mode proof --timeout-ms 30000 --max-edge-load 200000` | pass | `status=ok`; `sample_count=5`; `stored_path_count=4096`; `generated_fallback_used=false` |

No quality gate was skipped.

## Commit hygiene

`git diff --cached --name-only` returned no files, so nothing is currently staged.

`git status --short` after verification and before adding this report:

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
?? reports/audit/focused_scope_passport_verification.json
?? reports/audit/focused_scope_passport_verification.md
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

After adding this report, current `git status --short` additionally includes:

```text
?? reports/audit/full_scope_passport_verification.json
?? reports/audit/full_scope_passport_verification.md
```

`git ls-files --others --exclude-standard` showed only the untracked audit reports listed above. A generated-artifact pattern check over untracked non-ignored files found no matches for:

- DB files: `.sqlite`, `.sqlite-wal`, `.sqlite-shm`, `.db`
- raw logs: `.log`
- `target/`
- `reports/final/`
- CGC artifacts

Generated quality-gate payloads from this run are outside the repo temp directory and should not be staged.

## Acceptance status

- Full workspace tests pass.
- Workspace build passes.
- Release `codegraph-mcp` build passes.
- Diff check passes.
- Docs checks pass.
- Graph Truth Gate passes.
- Context Packet Gate passes.
- DB integrity/schema check passes.
- Default query surface smoke passes.
- PathEvidence sampler smoke passes.
- No unwanted generated artifacts are staged.
