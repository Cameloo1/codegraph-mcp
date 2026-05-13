# Intended Tool Quality Gate

Source of truth: MVP.md.

Generated: 2026-05-13 08:51:00 -05:00

## Verdict

**FAIL**

Reason: Fresh comprehensive gate fails proof_build_only_ms target; official CGC comparison remains blocked/diagnostic only.

CGC comparison may run officially: False.

No CodeGraph vs CGC superiority claim is allowed from this diagnostic run.

## Failed Targets

- proof_build_only_ms: observed 184297 ms, target <=60,000 ms

## Passed Targets

- cargo_test_workspace: pass
- graph_truth_cases: 11/11
- context_packet_cases: 11/11
- db_integrity: ok
- proof_db_mib: 171.184 MiB, target <=250 MiB
- repeat_unchanged_ms: 1674.0 ms
- single_file_update_ms: 336.0 ms
- artifact_freshness: fresh

## CGC Status

CGC was installed and invoked diagnostically. Smoke passed, fixture diagnostic completed but is not comparable, and Autoresearch timed out under the 180s cap. Official comparison remains blocked by the current CodeGraph proof-build target failure and CGC incompletion.