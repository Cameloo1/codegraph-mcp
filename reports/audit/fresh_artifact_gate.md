# Fresh Artifact Gate

Generated: 2026-05-13T00:11:45.061Z

## Verdict

**PASS for harness remediation.** The comprehensive benchmark now builds a fresh proof DB by default and requires explicit artifact reuse for persisted DBs. Stale reused artifacts are labeled and cannot claim storage or cold-build passes.

## Fresh Default Probe

- Execution mode: `fresh_proof_build`
- Freshness metadata present: `true`
- Freshly built: `true`
- Artifact reuse: `false`
- Stale: `false`
- Schema version: `18`
- Integrity: `ok`
- Storage claimable: `true`
- Cold-build claimable: `true`
- Build duration: `533 ms` on fixture probe
- Artifact: `reports/audit/artifacts/fresh_artifact_gate_fresh/comprehensive_benchmark_latest.json`

## Stale Reuse Probe

- Execution mode: `explicit_artifact_reuse`
- Freshness metadata present: `false`
- Artifact reuse: `true`
- Stale: `true`
- Storage claimable: `false`
- Cold-build claimable: `false`
- Stale reasons:
  - missing freshness metadata
  - schema mismatch: metadata=None, current=18
  - database schema mismatch: actual=15, current=18
  - migration mismatch: metadata=None, current=18
  - storage mode mismatch: metadata=None, expected proof
  - missing db_size_bytes
  - missing build_duration_ms
  - missing git_commit
- Artifact: `reports/audit/artifacts/fresh_artifact_gate_reuse/comprehensive_benchmark_latest.json`

## Fail-On-Stale

`--fail-on-stale-artifact` refused the old compact proof artifact as expected. The refusal included missing metadata plus schema and migration mismatch details.

## Tests

- `cargo test -p codegraph-cli bench_comprehensive -- --nocapture` passed 5/5 targeted comprehensive freshness tests.
- `cargo test --workspace` passed.

## Notes

- Storage wins are now claimable only from fresh proof builds or freshness-validated reused artifacts.
- The stale 320.63 MiB compact proof artifact is explicitly reported as stale and non-claimable unless rebuilt with current metadata.
