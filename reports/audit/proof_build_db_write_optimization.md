# Proof Build DB Write Optimization

Source of truth: `MVP.md`.

Generated: 2026-05-13 00:56:13 -05:00

## Verdict

Production proof-build-only is now **59414 ms** in the fresh comprehensive artifact (target <= 60,000 ms): **pass**.

Independent 64-worker profile: **58991 ms**.

## Before / After

- Release baseline before this pass: 98812 ms, DB write 70922 ms.
- Optimized profile: 58991 ms, DB write 35337 ms.
- Fresh comprehensive artifact: 59414 ms.

## Changes

- Shared global resolver workspace for import/security/test reduction.
- Fast compact proof global-edge insert path in proof-build-only.
- Single summary count reconciliation instead of per-edge existence probes.
- Redundant final proof-build-only quick_check skipped after hidden temp DB quick_check passes.
- SQLite file/entity reverse-map cache for hot proof-edge mapping.
- Explicit `bench proof-build-only` and `bench proof-build-validated` modes.

## Stage Checks

- template_entities insert: 764.1944000000001 ms
- template_edges insert: 241.50140000000002 ms
- path_evidence insert: 2187.6020000000008 ms
- index creation: 202.17780000000002 ms
- transaction commit: 132.71720000000002 ms

Correctness/context/integrity gates remain green; full workspace tests passed.
