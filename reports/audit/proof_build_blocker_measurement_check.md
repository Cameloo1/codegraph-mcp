# Proof Build Blocker Measurement Check

Source of truth: `MVP.md`.

Generated: 2026-05-13 00:56:13 -05:00

## Verdict

The old `proof_build_only_ms = 331,868` was a real prior fresh proof-build-only measurement, but it is now stale relative to the latest fresh comprehensive artifact.

Current fresh comprehensive proof-build-only value: **59414 ms** (target <= 60,000 ms).

Independent profile check: **58991 ms** with 64 workers.

## Contamination Check

- Fresh artifact reused? no
- Full comprehensive gate counted as proof-build-only? no
- Validation/reporting counted as proof-build-only? no
- Storage audit/dbstat counted as proof-build-only? no
- Relation/source-span sampler counted as proof-build-only? no
- CGC/comparison counted as proof-build-only? no
- Repeated proof DB rebuilds counted as proof-build-only? no
- VACUUM/ANALYZE included in proof-build-only? no
- Publish safety quick_check included? yes, on hidden temp DB before visible replacement

## Evidence

- Latest comprehensive artifact metadata: `<REPO_ROOT>/Desktop\development\codegraph-mcp\reports\final\artifacts\comprehensive_proof_proof_build_fix_20260513.artifact.json`
- Latest comprehensive artifact: `<REPO_ROOT>/Desktop\development\codegraph-mcp\reports\final\artifacts\comprehensive_proof_proof_build_fix_20260513.sqlite`
- Independent profile artifact: `reports/audit/artifacts/proof_build_only_profile_publish_skip_w64_20260513_004355.stdout.json`
- `cargo test --workspace`: pass
