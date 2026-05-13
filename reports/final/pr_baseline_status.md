# PR Baseline Status

Source of truth: `MVP.md`.

Generated: 2026-05-13 10:12:19 -05:00

This report freezes the repo status before PR-readiness edits. The PR scope is fresh-clone buildability, smoke indexing, docs, CI, Windows/Linux support, and README correctness. It is not a final intended-performance PR.

## Verdict

**FAIL**

Reason: the Intended Tool Quality Gate fails the `proof_build_only_ms` target.

No final intended-performance pass claim is allowed from this baseline. No CodeGraph-vs-CGC superiority claim is allowed from this baseline.

## Current Status

| Metric | Observed | Target | Status |
| --- | ---: | ---: | --- |
| `cargo_test_workspace` | pass | pass | pass |
| `graph_truth_cases` | 11/11 | 11/11 | pass |
| `context_packet_cases` | 11/11 | 11/11 | pass |
| `db_integrity` | ok | ok | pass |
| `proof_db_mib` | 171.184 MiB | <=250 MiB | pass |
| `proof_build_only_ms` | 184,297 ms | <=60,000 ms | fail |
| `repeat_unchanged_ms` | 1,674 ms | <=5,000 ms | pass |
| `single_file_update_ms` | 336 ms | <=750 ms | pass |
| `artifact_freshness` | fresh | fresh | pass |

## Known Limitations

- The Intended Tool Quality Gate verdict is `FAIL`.
- The main failed target is `proof_build_only_ms = 184,297 ms`, above the `<=60,000 ms` target.
- This baseline does not prove final intended-performance readiness.
- This baseline does not prove CodeGraph beats CGC.
- Benchmark thresholds were not changed.
- Graph Truth, Context Packet, DB integrity, and benchmark honesty must remain protected during PR-readiness edits.

## CGC Status

CGC was installed and invoked diagnostically. The diagnostic path produced smoke and fixture evidence, but the official comparison remains blocked/incomplete. CGC Autoresearch did not complete under the diagnostic cap, and CodeGraph currently fails the proof-build-only target. CodeGraph-vs-CGC superiority claims are not allowed.

## Manual Precision Boundary

Manual relation precision evidence currently covers 320 labeled samples. This is sampled precision only. Recall is unknown because there is no false-negative gold denominator. There is no precision claim for absent proof-mode relations, including `AUTHORIZES`, `CHECKS_ROLE`, `SANITIZES`, `EXPOSES`, `TESTS`, `ASSERTS`, `MOCKS`, and `STUBS`.

## Platform-Support Goal

The current PR-readiness platform goal is Windows and Linux fresh-clone build/smoke support. macOS support is not claimed by this PR baseline.

## PR Scope

In scope:

- Fresh-clone buildability.
- Smoke indexing.
- Documentation and README correctness.
- CI readiness.
- Windows/Linux support boundary.
- Honest baseline reporting.

Out of scope:

- Final intended-performance pass.
- CGC superiority claims.
- Benchmark threshold changes.
- New unrelated product features.
- macOS support claims.
- Weakening Graph Truth, Context Packet, DB integrity, or benchmark honesty.

## Production Behavior

No production behavior changed for this baseline freeze. This report is a documentation/artifact update only.
