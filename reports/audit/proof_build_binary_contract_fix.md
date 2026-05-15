# Proof Build Binary Contract Fix

Generated: `2026-05-15 12:11:08 -05:00`

Verdict: **contract fixed; final product gate still FAIL**.

## What Changed

- Benchmark JSON now includes `binary_metadata` with `current_exe`, `command_line`, `debug_assertions`, `binary_profile`, `claimable_for_thresholds`, and `timing_classification`.
- `bench proof-build-only`, `bench proof-build-validated`, and fresh `bench comprehensive` reject debug proof-build timing unless `--allow-debug-timing` is passed.
- Debug timings are `diagnostic_only` and `claimable_for_thresholds=false`.
- Release timings are `production` and `claimable_for_thresholds=true`.
- Proof artifact metadata now records build command, executable path, binary profile, claimability, and threshold contract.
- Comprehensive benchmark freshness metrics now include `proof_build_binary_profile_release` and `proof_build_timing_claimable_for_thresholds`.
- `proof-build-validated` now reports the proof-build portion instead of emitting `proof_build_only_ms=0`; validation time remains separate.

## Before / After

| Measurement | Value | Claimable | Notes |
| --- | ---: | --- | --- |
| Previous final `proof_build_only_ms` | `154,217 ms` | ambiguous | Old report lacked binary-profile metadata and was consistent with debug/Cargo-run contamination. |
| Current release standalone proof-build-only | `57,517 ms` | yes | `target\\release\\codegraph-mcp.exe`; no Cargo build time included. |
| Current release comprehensive proof-build-only | `58,715 ms` | yes | Fresh comprehensive artifact, release binary, `cold_build_result_claimable=true`. |
| Current release proof-build-plus-validation | `91,016 ms wall` | diagnostic for validation mode | Proof-build portion `88,362 ms`; validation `2,589.6454 ms`; artifact validated. |

Earlier same-session release probes observed `91,992 ms` and `61,194 ms` before the final current-source rerun. That variance is preserved as real evidence; OS cache and host load effects remain unknown. The final current-source claimable standalone proof-build-only result is `57,517 ms`.

## Verification

- `cargo test --workspace`: pass.
- `cargo build --workspace`: pass.
- `cargo build --release --bin codegraph-mcp`: pass.
- Targeted benchmark contract tests: pass.
- Graph Truth Gate: `11/11`.
- Context Packet Gate: `11/11`.
- DB schema/integrity: `ok`.
- Default query surface: pass.
- PathEvidence sampler: pass.
- Side-effect audit: no side-effect bugs detected.

## Still Failing

- Overall comprehensive verdict remains **FAIL** because `bytes_per_proof_edge` and `proof_db_mib_stretch` fail.
- Index-scope audit still reports `84` suspicious parsed paths by dry-run model.
- Explicit `update-fast` final-gate probe still fails with a DB passport repo-root mismatch.

No benchmark thresholds were changed, no proof facts were removed, no CGC work was performed, and no final intended-performance pass is claimed.

## Key Artifacts

- Release standalone proof-build-only: `reports/audit/artifacts/pb_rel2_20260515_115841/proof.stdout.json`
- Release proof-build-plus-validation: `reports/audit/artifacts/pbv_rel2_20260515_115953/proof_validated.stdout.json`
- Release comprehensive rerun: `reports/final/artifacts/pg_rel2_20260515_115657/comp/comprehensive_benchmark_latest.json`
- Gate artifacts: `reports/final/artifacts/pg_rel2_20260515_115657/gates/`
