# Minimal Storage Package Apply

Verdict: **storage target passed; comprehensive tool gate still failed honestly.**

The fresh Autoresearch compact proof DB built from schema `19` is `171.28 MiB` file-family size (`179,601,408` bytes), below both the required `<=250 MiB` target and the preferred `<=240 MiB` safety target. This is a claimable number: the proof DB was freshly built in this task, then consumed by the comprehensive benchmark with matching freshness metadata and `--fail-on-stale-artifact`.

The full comprehensive benchmark still fails because cold proof build time remains far above `60s`, bytes-per-proof-edge remains above the old intended ratio, and the stretch `<=150 MiB` proof DB target is not met. No comparison or intended-performance win should be claimed from this run.

## Fresh Artifact

| Field | Value |
| --- | --- |
| Proof DB | `reports/audit/artifacts/minimal_storage_package_autoresearch_proof.sqlite` |
| Metadata | `reports/audit/artifacts/minimal_storage_package_autoresearch_proof.artifact.json` |
| Schema | `19` |
| Storage mode | `proof` |
| Integrity | `ok` |
| DB bytes | `179,568,640` |
| File-family bytes | `179,601,408` |
| Proof DB MiB | `171.28` |
| Required target | `<=250 MiB` |
| Preferred safety target | `<=240 MiB` |

Baseline comparison: `320.63 MiB` -> `171.28 MiB`, saving about `149.35 MiB`.

## Package Applied

| Change | Status | Safe-disposition answer |
| --- | --- | --- |
| Drop proof-mode `template_edges` directional indexes | Applied | No logical fact removed. The index payload is derivable/recreatable and the fresh proof DB contains no `idx_template_edges_head_relation` or `idx_template_edges_tail_relation`. |
| Integerize template-local entity/edge/head/tail IDs | Applied | Wide local string IDs are replaced by deterministic integers scoped by `content_template_id`; external identity remains path-specific through the file/template overlay. |
| Normalize PathEvidence JSON | Applied in schema `19` | Verbose per-edge labels are now materialized in `path_evidence_edges`; normal context/audit readers hydrate labels from rows. Optional `path_evidence_debug_metadata` exists for audit/debug sidecar use and is not populated in proof mode. |

The direct `path_evidence` table reduction is smaller than the older lower-bound estimate because core `edges_json` and `source_spans_json` remain for compatibility, but the total fresh proof DB now clears the required target.

## Do-Not-Regress Questions

| Question | Answer |
| --- | --- |
| Did graph truth still pass? | Yes, `11/11`. |
| Did context packet quality still pass? | Yes, `11/11`. |
| Did proof DB size decrease? | Yes, `320.63 MiB` -> `171.28 MiB`. |
| Did removed data move to sidecar, become derivable, or get proven unnecessary? | Yes: indexes are derivable/recreatable, template IDs are compact aliases, PathEvidence labels moved to materialized rows/debug sidecar. |

## Verification

| Gate | Result |
| --- | --- |
| Graph Truth Gate | Passed, `11/11` |
| Context Packet Gate | Passed, `11/11` |
| DB integrity | `ok` |
| Default query surface | Passed |
| PathEvidence sampler | 20 paths in `27 ms`, 100 paths in `33 ms`, no fallback generation |
| CALLS relation sampler | 50 samples, `ok` |
| Repeat unchanged | `1,535 ms`, 0 files read/hashed/parsed |
| Latest single-file update metric | `336 ms`, passing; not rerun here because this storage package did not mutate the external Autoresearch checkout |
| `cargo test --workspace` | Passed |

## Query P95

| Query | p95 |
| --- | ---: |
| Entity name lookup | `2.742 ms` |
| Symbol lookup | `217.194 ms` |
| Qname lookup | `26.950 ms` |
| Text/FTS query | `0.299 ms` |
| CALLS relation query | `283.799 ms` |
| READS/WRITES relation query | `277.285 ms` |
| PathEvidence lookup | `3.633 ms` |
| Source snippet batch load | `56.326 ms` |
| `context_pack` normal | `148.886 ms` |
| Unresolved calls page | `109.706 ms` |

## Remaining Contributors

| Object | Rows | MiB | Share |
| --- | ---: | ---: | ---: |
| `template_entities` | 653,819 | `55.45` | `32.38%` |
| `template_edges` | 169,290 | `16.92` | `9.88%` |
| `file_entities` | 89,407 | `9.77` | `5.71%` |
| `idx_file_entities_entity` | unknown | `8.68` | `5.07%` |
| `symbol_dict` | 176,558 | `7.70` | `4.50%` |
| `qname_prefix_dict` | 48,603 | `5.86` | `3.42%` |

## Comprehensive Result

`reports/final/comprehensive_benchmark_minimal_storage_package_apply.json` reports:

- `proof_db_mib`: pass at `171.28`.
- `context_pack_normal`: pass.
- `unresolved_calls_paginated`: pass.
- `repeat_unchanged_total_ms`: pass.
- `single_file_update_total_ms`: pass from latest persisted metric.
- Final verdict: fail, because `cold_proof_build_total_wall_ms`, `bytes_per_proof_edge`, and stretch `proof_db_mib_stretch` still fail.

Next narrow fix: cold proof-build write/global-reduction time, not more proof DB size. The size target is now cleared.
