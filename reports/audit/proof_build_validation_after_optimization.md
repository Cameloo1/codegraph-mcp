# Proof Build Validation After Optimization

Source of truth: `MVP.md`.

Generated: 2026-05-13 00:56:13 -05:00

## Verdict

Fresh optimized proof artifact is valid and claimable.

- Proof-build-only: **59414 ms** (target <= 60,000 ms)
- Proof DB: **171.184 MiB** (target <= 250 MiB)
- Artifact stale: no
- DB integrity: ok
- Graph Truth Gate: 11/11
- Context Packet Gate: 11/11
- Repeat unchanged p95: 3215 ms
- Single-file update p95: 512 ms
- context_pack p95: 158.565 ms
- unresolved-calls p95: 115.45 ms
- PathEvidence sample 20/100: 27 ms / 35 ms
- `cargo test --workspace`: pass

Fresh artifact: `<REPO_ROOT>/Desktop\development\codegraph-mcp\reports\final\artifacts\comprehensive_proof_proof_build_fix_20260513.sqlite`
