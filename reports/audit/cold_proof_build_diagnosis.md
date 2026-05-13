# Cold Proof Build Diagnosis

Generated: 2026-05-12 11:21:10 -05:00

Source of truth: `MVP.md`.

Diagnosis scope: explain the `50.02 min` Autoresearch proof-mode cold build. This phase did not optimize storage or indexing.

## Verdict

Status: **diagnosed fail**.

The `50.02 min` value is the persisted **proof-build-only production cold build** from the compact proof gate:

`reports/final/compact_proof_db_gate.json -> autoresearch.proof_build.wall_ms = 3,001,445 ms`

It is not the full benchmark gate wall time, not the audit/debug sidecar build, not CGC, not report generation, and not storage audit/dbstat. Integrity checking is reported separately.

The exact current top bottleneck is:

`autoresearch.proof_build.db_write_ms = 2,877,316 ms`, or `95.86%` of proof-build-only wall time.

That bucket is too broad. Code inspection shows it currently includes more than raw row insertion: local persistence, post-local global reducers, PathEvidence refresh, index recreation, `ANALYZE`, WAL checkpoint work, and transaction/commit work.

## Mode Distinction

The comprehensive benchmark now separates these modes in `reports/final/comprehensive_benchmark_latest.json`:

| Mode | Observed | Status | Meaning |
| --- | ---: | --- | --- |
| `proof-build-only` | `3,001,445 ms` / `50.024 min` | fail | Actual production proof-mode cold build compared to the `<=60s` target. |
| `proof-build-plus-validation` | `3,014,382 ms` / `50.240 min` | fail | Proof build plus separately persisted integrity check. |
| `proof-build-plus-audit` | `6,501,810 ms` / `108.364 min` | reported | Sequential proof build plus the separate audit-sidecar build. Not the proof target. |
| `full-gate` | unknown | unknown | The compact gate does not persist a single full-gate wall value. |

## Waterfall

| Stage | Elapsed | Share | Included in 50.02 min? | Evidence |
| --- | ---: | ---: | --- | --- |
| Actual proof DB build total | `3,001,445 ms` | 100.00% | yes | `compact_proof_db_gate.autoresearch.proof_build.wall_ms` |
| Production persistence and global reduction bucket | `2,877,316 ms` | 95.86% | yes | `compact_proof_db_gate.autoresearch.proof_build.db_write_ms` |
| Source scan/parse/extract/dedupe/reducer residual | `124,129 ms` | 4.14% | yes | `wall_ms - db_write_ms` |
| Integrity-check validation | `12,937 ms` | 0.43% of build plus validation | no | `compact_proof_db_gate.autoresearch.proof_build.integrity_check_ms` |
| Audit-sidecar build | `3,500,365 ms` | n/a | no | `compact_proof_db_gate.autoresearch.audit_build.wall_ms` |

No high-level operation over 5% is unexplained in the persisted artifact: the broad `db_write_ms` bucket is the known over-5% stage. The missing detail is inside that bucket.

## Required Split

| Required stage | Current exact timing | Current parent bucket | Notes |
| --- | ---: | --- | --- |
| Actual proof DB build | `3,001,445 ms` | `proof-build-only` | Known. |
| Source scan | unknown | residual | Not separately persisted in compact gate. |
| Parse/extract | unknown | residual | `2,555` files parsed after content-template dedupe. |
| Content-template dedupe | unknown | residual and persistence bucket | `2,420` duplicate local analyses skipped, `2,718` templates. |
| Reducer | unknown | persistence/global-reduction bucket | Post-local reducers are counted in `db_write_ms`. |
| Dictionary interning | unknown | persistence/global-reduction bucket | Current rows: `696,373` symbols, `727,372` qnames, `155,741` qname prefixes. |
| Template entity insert | unknown | persistence/global-reduction bucket | `653,819` rows. |
| Template edge insert | unknown | persistence/global-reduction bucket | `169,290` rows. |
| Proof graph materialization | unknown | persistence/global-reduction bucket | `89,407` proof entities and `31,848` physical proof edges. |
| PathEvidence generation | unknown | persistence/global-reduction bucket | `4,096` PathEvidence rows. |
| Index creation | unknown | persistence/global-reduction bucket | Recreated after bulk load. |
| `VACUUM` | not observed | not included by current evidence | `finish_bulk_index_load` evidence shows index creation, `ANALYZE`, WAL settings/checkpoint; no `VACUUM`. |
| `ANALYZE` | unknown | persistence/global-reduction bucket | Included in bulk index finish SQL, not separately persisted. |
| Integrity check | `12,937 ms` | validation | Separately measured; not dominant. |
| Report generation | unknown | full gate only | Not part of `proof_build.wall_ms`. |
| Benchmark-only cleanup | unknown | full gate only | No evidence it is part of `proof_build.wall_ms`. |
| Artifact copy/compression | unknown | full gate only | No evidence it is part of `proof_build.wall_ms`. |

## Top 5 Bottlenecks

1. **Production persistence and global reduction bucket**: `2,877,316 ms`, `95.86%` of proof build. This is the exact current bottleneck.
2. **Template entity persistence and identity interning**: `653,819` template entity rows, `87,523,328` bytes. Exact current time is not separately persisted.
3. **Template edge persistence and relation/provenance interning**: `169,290` template edge rows, `40,804,352` bytes, plus template edge indexes. Exact current time is not separately persisted.
4. **Dictionary interning**: `696,373` symbols, `727,372` qualified names, `155,741` qname prefixes. The older detailed Autoresearch profile showed `45,618,322` dictionary lookup/insert calls and `144,145 ms`, but that profile predates the compact proof artifact, so it is supporting context, not current exact timing.
5. **Post-local global reducers and PathEvidence refresh hidden under `db_write_ms`**: code inspection shows the post-local import/security/test/derived reducers and stored PathEvidence refresh are included in the broad write bucket.

## Code Evidence

- `crates/codegraph-index/src/lib.rs:1004` starts the post-local section that runs stale cleanup, global reducers, and PathEvidence refresh.
- `crates/codegraph-index/src/lib.rs:1149` adds the entire post-local elapsed time into `db_write_ms`.
- `crates/codegraph-index/src/lib.rs:1168` runs `finish_bulk_index_load()` after local/global facts are visible.
- `crates/codegraph-index/src/lib.rs:1177` records `IndexProfile.total_wall_ms` and `db_write_ms`.
- `crates/codegraph-index/src/lib.rs:1200` runs the post-index integrity check after the profile total is initially recorded.
- `crates/codegraph-store/src/sqlite.rs:782` executes bulk index finish SQL.
- `crates/codegraph-store/src/sqlite.rs:7607` shows `ANALYZE` in bulk index finish SQL.

## Reporting Fix

The master comprehensive benchmark now writes:

- `sections.cold_proof_build_profile.mode_distinction`
- `sections.cold_proof_build_profile.waterfall`
- `sections.cold_proof_build_profile.interpretation`

This does not change production semantics and does not change thresholds. It only prevents future reports from conflating production proof build time with validation/audit/full-gate time.

## Diagnosis Limit

The compact proof gate did not persist nested `IndexProfile.spans` for the proof-mode cold build. Therefore the exact split inside the `2,877,316 ms` bucket cannot be reconstructed from the current artifact without rerunning a profiled cold build.

The next profiling-only fix should archive raw proof-mode `IndexProfile.spans` and split:

- template entity insert
- template edge insert
- dictionary interning by dictionary table
- global import/security/test/derived reducers
- PathEvidence refresh
- index recreation
- `ANALYZE`
- WAL checkpoint
- transaction commit

## Conclusion

The `50.02 min` failure is real production proof-build time. The dominant known cause is the broad production persistence/global-reduction bucket, not validation, audit, CGC, report generation, or storage audit.

No optimization was performed in this phase.
