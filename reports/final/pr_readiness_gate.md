# PR Readiness Gate

Source of truth: `MVP.md`.

Generated: 2026-05-13 14:40:20 -05:00

This report is a PR-readiness gate for fresh-clone buildability, smoke indexing, docs, CI, Windows/Linux support, and README correctness. It is not a final intended-performance pass report.

## Verdict

**ready_with_known_limitations**

The required PR-readiness checks pass. The only limitations carried into this verdict are the allowed caveats: proof-build-only performance still fails the intended target, official CGC comparison remains blocked/incomplete, macOS is coming soon and not tested, and manual relation precision is sampled precision only with recall unknown.

## Environment

| Field | Value |
| --- | --- |
| OS | Microsoft Windows NT 10.0.26200.0 |
| Shell | Windows PowerShell 5.1.26100.8457 |
| `rustc --version` | `rustc 1.95.0 (59807616e 2026-04-14)` |
| `cargo --version` | `cargo 1.95.0 (f2d3ce0bd 2026-03-21)` |
| Git commit | unavailable in this workspace: `git rev-parse HEAD` failed with `fatal: not a git repository (or any of the parent directories): .git` |

## Local Command Results

| Check | Command | Result | Notes |
| --- | --- | --- | --- |
| Build | `cargo build --workspace` | pass | Finished dev profile successfully. |
| Test | `cargo test --workspace` | pass | Workspace tests passed; optional CGC/Node-dependent tests remain ignored as designed. |
| Help | `cargo run --bin codegraph-mcp -- --help` | pass | CLI help printed successfully. |
| Fixture index | `cargo run --bin codegraph-mcp -- index fixtures/smoke/basic_repo` | pass | Indexed 1 supported source file; status `indexed`; 15 entities and 35 edges reported. |
| Repo index | `target/debug/codegraph-mcp.exe index .` | pass | Feasible locally and completed without crash; warm no-op over existing local index with status `indexed`. |
| README artifact check | `python scripts/check_readme_artifacts.py` | pass | 12 README report paths checked. |
| README link check | `python scripts/check_markdown_links.py` | pass | 27 local links checked. |
| PowerShell index smoke | `powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke_index.ps1 -SkipRepoIndex` | pass | Logs: `reports/smoke/index/windows_20260513_143753_6b31f896`. |
| PowerShell fresh-clone smoke | `powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke_fresh_clone.ps1` | pass | Logs: `reports/smoke/fresh_clone/windows_20260513_143757_f25465cd`; path with spaces was used. |
| Bash smoke | WSL/Bash availability probe | not run | Host is Windows and WSL has no installed distributions, so Linux/WSL Bash smoke is not applicable on this machine. |
| Docker smoke | `docker version --format '{{.Server.Version}}'` | not run | Docker CLI exists, but config access/daemon connection failed; Docker local smoke is an environment skip. |

## CI Configuration

| CI area | Status | Evidence |
| --- | --- | --- |
| Windows CI | pass | `.github/workflows/ci.yml` includes `windows-build-test-smoke` on `windows-latest` with metadata, build, test, help, fixture index smoke, README artifact check, and Markdown link check. |
| Linux CI | pass | `.github/workflows/ci.yml` includes `linux-build-test-smoke` on `ubuntu-latest` with metadata, build, test, help, fixture index smoke, README artifact check, and Markdown link check. |
| Docker CI | pass | `.github/workflows/ci.yml` includes `docker-smoke` on `ubuntu-latest` with `docker build`, container help, and container fixture index smoke. |
| macOS CI | not present | macOS is intentionally not in CI. README marks macOS as coming soon, not currently tested, and no CI coverage. |

The workflow does not require CGC, Autoresearch, or large benchmark artifacts for CI.

## README And Benchmark Honesty

| Requirement | Status | Evidence |
| --- | --- | --- |
| Current benchmark status is honest | pass | README and baseline report preserve Intended Tool Quality Gate `FAIL`. |
| `proof_build_only_ms` failure is visible | pass | README and baseline report record `proof_build_only_ms = 184,297 ms`, target `<=60,000 ms`. |
| No final intended-performance pass claim | pass | README says final intended-performance readiness is not claimed. |
| No CodeGraph-vs-CGC superiority claim | pass | README and baseline report state CGC comparison is incomplete/unknown and no superiority claim is made. |
| Manual precision boundary is honest | pass | README states sampled precision only and recall unknown. |
| macOS support boundary is honest | pass | README says macOS is coming soon, not currently tested, no CI coverage, and not supported by this baseline. |

## Known Blockers

None for this PR-readiness scope.

## Known Limitations

- `proof_build_only_ms = 184,297 ms` still fails the intended `<=60,000 ms` target.
- Official CGC comparison remains blocked/incomplete.
- macOS is coming soon, not currently tested, and has no CI coverage.
- Manual relation precision is sampled precision only; recall is unknown.

## Pass Criteria

| Criterion | Status |
| --- | --- |
| `cargo build --workspace` passes | pass |
| `cargo test --workspace` passes | pass |
| `codegraph-mcp --help` passes | pass |
| Fixture index smoke passes | pass |
| README artifact references pass | pass |
| README relative links pass | pass |
| CI workflow exists for Windows and Linux | pass |
| macOS is marked coming soon | pass |
| Current benchmark status is honest | pass |
| No false CGC/superiority claim | pass |

## Final Verdict

`ready_with_known_limitations`
