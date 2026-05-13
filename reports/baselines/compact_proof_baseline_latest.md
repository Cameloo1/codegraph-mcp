# Compact Proof Baseline Latest

Frozen: 2026-05-12 10:26:46 -05:00

Source of truth: `MVP.md`.

This baseline freezes the latest compact proof DB gate before any further storage, indexing, or update optimization. It is a diagnostic baseline, not an intended-performance pass.

## Executive Verdict

Verdict: **fail, honestly**

The compact proof graph is now semantically trustworthy on the adversarial fixture gates, integrity-clean, and fast enough for the measured `context_pack`, unresolved-calls, and repeat-unchanged paths. It still does not meet the intended large-codebase standard because proof storage, single-file update, and cold proof build are still off target.

## Pass/Fail Criteria

| Criterion | Target | Observed | Status |
| --- | ---: | ---: | --- |
| Graph Truth Gate | 11/11 | 11/11 | pass |
| Context Packet Gate | 11/11 | 11/11 | pass |
| Forbidden edge violations | 0 | 0 | pass |
| Forbidden path violations | 0 | 0 | pass |
| Proof source-span coverage | 100% | 100% | pass |
| Exact-labeled unresolved | 0 | 0 | pass |
| Derived without provenance | 0 | 0 | pass |
| Test/mock production leakage | 0 | 0 | pass |
| Stale fact failures | 0 | 0 | pass |
| DB integrity | ok | ok | pass |
| Full tests | pass | `cargo test --workspace` passed | pass |
| Repeat unchanged | <=5,000 ms | 1,674 ms profile / 2,502.523 ms shell | pass |
| `context_pack` p95 | <=2,000 ms | 834.853 ms profile / 852.213 ms shell | pass |
| unresolved-calls page p95 | <=1,000 ms | 242.777 ms shell | pass |
| Proof DB family size | <=250 MiB | 320.63 MiB | fail |
| Single-file update | <=750 ms | 1,694 ms | fail |
| Cold proof build | <=60,000 ms intended | 3,001,445 ms / 50.02 min | fail |
| Manual real-repo relation precision | labeled and estimated | unknown; samples generated but not labeled | unknown |
| CGC comparison | complete comparable artifacts | incomplete/unknown from latest comparison | unknown |

## Graph Truth Summary

| Metric | Value |
| --- | ---: |
| Cases total | 11 |
| Cases passed | 11 |
| Cases failed | 0 |
| Expected entities | 29 |
| Matched entities | 29 |
| Expected edges | 20 |
| Matched expected edges | 20 |
| Forbidden edges | 14 |
| Matched forbidden edges | 0 |
| Expected paths | 11 |
| Matched expected paths | 11 |
| Forbidden paths | 9 |
| Matched forbidden paths | 0 |
| Source-span failures | 0 |
| Context packet failures | 0 |
| Stale failures | 0 |

Note: the fixture gate implementation currently indexes fixture repos in audit mode so heuristic/unsupported assertions remain observable. The Autoresearch artifact frozen here is proof mode.

## Context Quality Summary

| Metric | Value |
| --- | ---: |
| Cases total | 11 |
| Cases passed | 11 |
| Critical symbol recall | 100% |
| Proof path coverage | 100% |
| Source-span coverage | 100% |
| Critical snippet coverage | 100% |
| Expected test recall | 100% |
| Distractor ratio | 0% |

## DB Integrity Summary

| Artifact | Integrity | Notes |
| --- | --- | --- |
| Proof DB | ok | `PRAGMA integrity_check` ok |
| Audit DB | ok | `PRAGMA integrity_check` ok |
| Repeat/update harness DB | ok | repeat, single-file update, and restore update integrity ok |

Proof DB path:

`reports/final/artifacts/compact_gate_autoresearch_proof.sqlite`

Audit DB path:

`reports/final/artifacts/compact_gate_autoresearch_audit.sqlite`

## Storage Summary

| Metric | Value |
| --- | ---: |
| Proof database bytes | 336,203,776 |
| Proof WAL bytes | 0 |
| Proof SHM bytes | 0 |
| Proof DB family bytes | 336,203,776 |
| Proof DB family MiB | 320.63 |
| Target MiB | 250.00 |
| Gap to target | 70.63 MiB |
| Physical proof edge rows | 31,848 |
| Relation-compatible facts | 170,114 |
| Stored PathEvidence rows | 4,096 |
| Audit DB family bytes | 1,063,632,896 |
| Audit DB family MiB | 1,014.36 |
| Audit-only sidecar rows | 1,150,760 |

Top remaining proof storage contributors:

| Object | Rows | Bytes |
| --- | ---: | ---: |
| `template_entities` | 653,819 | 87,523,328 |
| `template_edges` | 169,290 | 40,804,352 |
| `symbol_dict` | 696,373 | 30,392,320 |
| `qname_prefix_dict` | 155,741 | 22,274,048 |
| `idx_template_edges_head_relation` | n/a | 16,539,648 |
| `idx_template_edges_tail_relation` | n/a | 16,539,648 |
| `idx_symbol_dict_hash` | n/a | 14,602,240 |
| `idx_qualified_name_parts` | n/a | 12,742,656 |
| `qualified_name_dict` | 727,372 | 12,173,312 |
| `file_entities` | 89,407 | 10,248,192 |

Storage-specific hygiene note: proof storage still contains `180` parser-verified `ALIASED_BY` rows with `edge_class=unknown`. These did not violate graph truth, but edge-class cleanup is still unfinished.

## Cold Build Profile Summary

| Metric | Value |
| --- | ---: |
| Storage mode | proof |
| Storage policy | `proof:compact-proof-graph` |
| Wall time | 3,001,445 ms |
| Wall minutes | 50.02 |
| Files walked | 8,561 |
| Files read | 4,975 |
| Files hashed | 4,975 |
| Files parsed | 2,555 |
| Duplicate local analyses skipped | 2,420 |
| Files skipped | 3,586 |
| Entities reported | 743,226 |
| Edges reported | 339,404 |
| Duplicate edges upserted | 443 |
| DB write time | 2,877,316 ms |
| Integrity-check time | 12,937.4035 ms |

This remains the most serious performance failure by ratio: the intended target is `<=60,000 ms`.

## Update Profile Summary

| Step | Target | Observed | Status |
| --- | ---: | ---: | --- |
| Repeat unchanged | <=5,000 ms | 1,674 ms profile / 2,502.523 ms shell | pass |
| Single-file update | <=750 ms | 1,694 ms | fail |
| Restore update | n/a | 1,587 ms | measured |

Single-file update details:

| Metric | Value |
| --- | ---: |
| Files walked | 1 |
| Files read | 1 |
| Files hashed | 1 |
| Files parsed | 1 |
| Entities inserted | 15 |
| Edges inserted | 29 |
| Dirty PathEvidence count | 2 |
| Duplicate edge upserts | 0 |
| Transaction status | committed |
| Integrity status | ok |

## Query Latency Summary

| Query | Target | Observed | Status |
| --- | ---: | ---: | --- |
| `context_pack` p95 | <=2,000 ms | 834.853 ms profile / 852.213 ms shell | pass |
| unresolved-calls page p95 | <=1,000 ms | 242.777 ms shell | pass |

The measured `context_pack` query used one bounded fallback edge and returned no verified path for that seed. Fixture context packet proof-path coverage remains 100%, so this latency number is a smoke measurement, not a real-repo proof-path recall claim.

## Relation Sampling Summary

| Sample | Count |
| --- | ---: |
| CALLS | 50 |
| READS | 50 |
| WRITES | 50 |
| FLOWS_TO | 50 |
| PathEvidence | 20 |

Manual precision remains unknown because the samples are not human-labeled.

## What Is Now Solved

- Graph Truth: adversarial fixtures pass 11/11 with zero forbidden edge/path hits.
- Context Packet: fixture gate passes 11/11 with 100% critical-symbol, proof-path, source-span, snippet, and expected-test recall.
- Integrity: proof, audit, and update harness DB checks are ok.
- Repeat unchanged: under 5s, no reads/hashes/parses on unchanged source files.
- `context_pack`: under 2s p95 on the sampled proof-mode Autoresearch query.
- unresolved-calls: under 1s p95 on paginated query.

## What Remains Unsolved

- Proof DB size: `320.63 MiB`, still `70.63 MiB` over the 250 MiB target.
- Single-file update: `1.694s`, still above the 750 ms target.
- Cold proof build: `50.02 min`, far above the intended 60s target.
- Manual relation precision: unknown until real samples are labeled.
- CGC comparison: incomplete/unknown; latest comparison predates the compact proof gate and does not provide comparable CGC artifacts.
- Pure proof-mode fixture gate: current fixture gates hardcode audit-mode indexing.

## Do Not Regress

- Graph Truth Gate must remain 11/11.
- Context Packet Gate must remain 11/11.
- DB integrity must remain ok.
- `context_pack` p95 must remain <=2s.
- unresolved-calls page p95 must remain <=1s.
- Repeat unchanged must remain <=5s.
- Forbidden edge/path count must remain 0.
- Proof source-span coverage must remain 100%.
- Exact-labeled unresolved and derived-without-provenance counts must remain 0.
- Test/mock production leakage must remain 0.

## Frozen Artifacts

- Source gate: `reports/final/compact_proof_db_gate.md`
- Source gate JSON: `reports/final/compact_proof_db_gate.json`
- Frozen baseline markdown: `reports/baselines/compact_proof_baseline_latest.md`
- Frozen baseline JSON: `reports/baselines/compact_proof_baseline_latest.json`

## Next Optimization Targets

No optimization is included in this baseline. The next storage investigation should start with:

1. Narrowing `template_entities`.
2. Proving whether template edge indexes are required.
3. Reducing symbol/qname dictionary payload for template-only identities.
4. Reducing single-file update from `1.694s` to `<=750ms` without weakening integrity.
5. Adding proof-mode switches to fixture gates.
