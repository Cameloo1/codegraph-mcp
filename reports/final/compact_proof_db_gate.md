> **Note:** This report is from an earlier benchmark run. Current proof DB size is 171.184 MiB. See `comprehensive_benchmark_latest.md` for the latest gate numbers.

# Compact Proof DB Gate

Generated: 2026-05-12 06:21:33 -05:00

## Verdict

Status: **fail**

The compact proof DB is now correctness-clean and much smaller than the original debug-scale Autoresearch artifact, but it still misses the final compact proof target. The two hard failures are:

- Proof DB family size: `336,203,776` bytes / `320.63 MiB`, target `<=250 MiB`.
- Single-file update: `1,694 ms`, target `<=750 ms`.

Cold proof indexing also remains far outside the intended large-repo target: `3,001,445 ms` / `50.02 min`. This prompt's pass criteria did not list cold index time as a hard compact-proof DB pass criterion, but it is still a real performance failure.

## Important Gate Boundary

The Graph Truth and Context Packet fixture gate CLIs currently index fixtures in audit mode so heuristic and unsupported assertions remain observable. The final Autoresearch artifact measured here is proof mode:

`reports/final/artifacts/compact_gate_autoresearch_proof.sqlite`

So the final gate has two truths:

- Fixture correctness gate: passed, but the fixture runner itself still has no proof-mode switch.
- Large-repo proof artifact: built and measured in `proof:compact-proof-graph` mode.

That limitation is not hidden and should be fixed before claiming a pure proof-mode fixture gate.

## Required Run Results

| Required run | Result | Evidence |
| --- | --- | --- |
| Build Autoresearch proof mode | completed | `compact_gate_autoresearch_proof.sqlite` |
| Build Autoresearch audit/debug mode | audit completed | `compact_gate_autoresearch_audit.sqlite`; debug not separately built because current sidecar implementation is not physically split |
| DB integrity check | pass | `PRAGMA integrity_check` status `ok` in storage audit |
| Graph Truth Gate | pass | 11/11 fixtures |
| Context Packet Gate | pass | 11/11 fixtures |
| Stale/update/rename tests | pass | `cargo test --workspace`; update-integrity harness passed |
| Relation/source-span sampler | pass | 50 each for CALLS/READS/WRITES/FLOWS_TO; 20 PathEvidence |
| `context_pack` p95 | pass | `834.853 ms` profile p95 |
| Repeat unchanged index | pass | `1,674 ms` profile, `2,502.523 ms` shell |
| Single-file update | fail | `1,694 ms` |
| Storage audit | pass as audit, fail target | integrity ok; size over target |

## Pass Criteria

| Criterion | Target | Result | Status |
| --- | ---: | ---: | --- |
| Graph Truth Gate | pass | 11/11 | pass |
| Context Packet Gate | pass | 11/11 | pass |
| Forbidden edges/paths | 0 | 0 / 0 | pass |
| Proof source-span coverage | 100% | 100% | pass |
| Exact-labeled unresolved | 0 | 0 by strict graph-truth gate | pass |
| Derived without provenance | 0 | 0 by strict graph-truth gate | pass |
| Test/mock production leakage | 0 | 0 by strict graph-truth gate | pass |
| Stale facts | 0 | 0 graph-truth stale failures; update harness passed | pass |
| Proof DB size | <=250 MiB | 320.63 MiB | fail |
| Repeat unchanged | <=5s | 1.674s profile | pass |
| Single-file update | <=750ms p95 | 1.694s single run | fail |
| `context_pack` p95 | <=2s | 0.835s profile p95 | pass |
| unresolved-calls page p95 | <=1s | 0.243s shell p95 | pass |
| DB integrity | pass | ok | pass |

## Correctness

Graph Truth Gate:

| Metric | Value |
| --- | ---: |
| Cases | 11 |
| Passed | 11 |
| Failed | 0 |
| Expected entities | 29 / 29 |
| Expected edges | 20 / 20 |
| Forbidden edge violations | 0 / 14 |
| Expected paths | 11 / 11 |
| Forbidden path violations | 0 / 9 |
| Source-span failures | 0 |
| Context packet failures | 0 |
| Stale failures | 0 |

Context Packet Gate:

| Metric | Value |
| --- | ---: |
| Critical symbol recall | 100% |
| Proof path coverage | 100% |
| Proof source-span coverage | 100% |
| Critical snippet coverage | 100% |
| Expected test recall | 100% |
| Distractor ratio | 0% |

Full tests:

`cargo test --workspace` passed. Observed totals: `334` passed, `3` ignored.

## Autoresearch Builds

Proof-mode cold build:

| Metric | Value |
| --- | ---: |
| Storage policy | `proof:compact-proof-graph` |
| Wall time | `3,001,445 ms` / `50.02 min` |
| Files walked | 8,561 |
| Source files read/hashed | 4,975 / 4,975 |
| Files parsed | 2,555 |
| Duplicate local analyses skipped | 2,420 |
| Files skipped | 3,586 |
| Entities reported by indexer | 743,226 |
| Edges reported by indexer | 339,404 |
| Duplicate edges upserted | 443 |
| DB write time | 2,877,316 ms |
| Integrity-check time | 12,937 ms |

Audit-mode build:

| Metric | Value |
| --- | ---: |
| Storage policy | `audit:compact-proof-plus-diagnostic-sidecars` |
| Wall time | `3,500,365 ms` / `58.34 min` |
| Audit DB family size | `1,063,632,896` bytes / `1014.36 MiB` |
| Integrity | ok |

Debug-mode physical DB was not separately built. The current implementation exposes a `debug` storage mode string, but the physical proof/audit/debug sidecar split from the design report is not yet implemented as separate DB families.

## Storage

Proof DB:

| Metric | Value |
| --- | ---: |
| DB path | `reports/final/artifacts/compact_gate_autoresearch_proof.sqlite` |
| Database bytes | 336,203,776 |
| WAL bytes | 0 |
| SHM bytes at final audit | 0 |
| DB family bytes | 336,203,776 |
| DB family MiB | 320.63 |
| Target bytes | 262,144,000 |
| Gap to target | 74,059,776 bytes / 70.63 MiB |
| Physical proof edge rows | 31,848 |
| Relation-compatible facts | 170,114 |
| Stored PathEvidence rows | 4,096 |

Top remaining proof contributors:

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

The exact remaining contributors are now template identity payload, template edge payload, symbol/qname dictionaries, and template/dictionary indexes. The generic proof `edges` table itself is only `2,105,344` bytes.

## Proof/Audit/Debug Split

| Table | Proof rows | Audit rows | Meaning |
| --- | ---: | ---: | --- |
| `heuristic_edges` | 0 | 513,757 | sidecar diagnostic facts preserved outside proof |
| `unresolved_references` | 0 | 513,752 | sidecar unresolved references preserved outside proof |
| `static_references` | 0 | 123,251 | sidecar static reference entities preserved outside proof |
| `structural_relations` | 0 | 0 | derived from compact attributes/adapters |
| `template_entities` | 653,819 | 653,819 | shared content-template proof identity |
| `template_edges` | 169,290 | 169,290 | shared template proof facts |
| `entities` | 89,407 | 89,407 | physical path-specific entities |
| `edges` | 31,848 | 31,848 | physical proof edges |
| `path_evidence` | 4,096 | 4,096 | stored proof paths for context |

Rows preserved only in audit sidecar tables: `1,150,760`.

Proof edge hygiene note: proof storage still has `180` `ALIASED_BY` edges with `edge_class=unknown` and `exactness=parser_verified`. These are not heuristic/unresolved facts and did not trip graph truth, but the edge-class cleanup is still unfinished.

## Storage Savings By Category

These savings come from the compaction reports and are phase-to-phase measurements. They are useful as evidence, but not every row is independently additive because each phase changed the baseline.

| Category | Reported savings |
| --- | ---: |
| Structural/callsite proof-edge split | 426.76 MiB |
| Dictionary/index bloat compaction | 385.80 MiB |
| File hash normalization | 111.76 MiB |
| Metadata JSON compaction | 56.29 MiB |
| Object ID compaction | 98.02 MiB |
| Heuristic/debug sidecar split | 142.08 MiB |
| Structural relation compaction | 53.97 MiB |
| Content-template dedupe | 38.08 MiB |

The current measured endpoint is the authoritative number: proof DB family `320.63 MiB`, still `70.63 MiB` over target.

## Relation And Source-Span Sampler

| Sample | Result |
| --- | --- |
| CALLS | 50 samples with snippets |
| READS | 50 samples with snippets |
| WRITES | 50 samples with snippets |
| FLOWS_TO | 50 samples with snippets |
| PathEvidence | 20 stored PathEvidence samples with snippets |

Relation counts on proof DB:

| Relation | Edges | Missing source spans |
| --- | ---: | ---: |
| CONTAINS | 48,532 | 0 |
| DEFINED_IN | 48,109 | 0 |
| ARGUMENT_0 | 19,654 | 0 |
| FLOWS_TO | 14,801 | 0 |
| DECLARES | 11,468 | 0 |
| CALLS | 2,363 | 0 |
| READS | 1,970 | 0 |
| MAY_MUTATE | 1,203 | 0 |
| WRITES | 1,104 | 0 |
| MUTATES | 177 | 0 |

Manual relation precision remains unknown because this gate sampled evidence but did not perform human labeling.

## Query Latency

| Query | Iterations | p95 | Target | Status |
| --- | ---: | ---: | ---: | --- |
| `context_pack` normal mode | 5 | `834.853 ms` profile / `852.213 ms` shell | 2,000 ms | pass |
| unresolved-calls page | 20 | `242.777 ms` shell | 1,000 ms | pass |

The sampled `context_pack` query returned no stored proof path for the chosen seed and used one bounded fallback edge plus one snippet. Fixture context packet proof-path coverage remains 100%, so this is a latency smoke rather than a proof-path recall benchmark.

## Update Path

Update-integrity harness:

| Step | Wall ms | Walked | Read | Hashed | Parsed | Entities inserted | Edges inserted | Duplicate upserts | Integrity |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `cold_index` seed copy | 5,040 | n/a | n/a | n/a | n/a | n/a | n/a | n/a | ok |
| `repeat_unchanged_index` | 2,640 | 8,561 | 0 | 0 | 0 | 0 | 0 | 0 | ok |
| `single_file_update` | 1,694 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | ok |
| `restore_update` | 1,587 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | ok |

The harness verdict passed because integrity, repeat hash stability, changed hash, and restore hash were correct. The compact gate still fails the single-file update latency target.

## Artifacts

- `reports/final/artifacts/compact_gate_autoresearch_proof.sqlite`
- `reports/final/artifacts/compact_gate_autoresearch_audit.sqlite`
- `reports/final/artifacts/compact_gate_graph_truth.json`
- `reports/final/artifacts/compact_gate_graph_truth.md`
- `reports/final/artifacts/compact_gate_context_packet.json`
- `reports/final/artifacts/compact_gate_context_packet.md`
- `reports/final/artifacts/compact_gate_proof_storage.json`
- `reports/final/artifacts/compact_gate_proof_storage.md`
- `reports/final/artifacts/compact_gate_audit_storage.json`
- `reports/final/artifacts/compact_gate_audit_storage.md`
- `reports/final/artifacts/compact_gate_relation_counts.json`
- `reports/final/artifacts/compact_gate_relation_counts.md`
- `reports/final/artifacts/compact_gate_context_pack_latency.json`
- `reports/final/artifacts/compact_gate_unresolved_calls_latency.json`
- `reports/final/artifacts/compact_gate_update_integrity.json`
- `reports/final/artifacts/compact_gate_update_integrity.md`
- `reports/final/artifacts/compact_gate_sample_CALLS.json`
- `reports/final/artifacts/compact_gate_sample_READS.json`
- `reports/final/artifacts/compact_gate_sample_WRITES.json`
- `reports/final/artifacts/compact_gate_sample_FLOWS_TO.json`
- `reports/final/artifacts/compact_gate_sample_PathEvidence.json`

## Next Required Work

1. Shrink `template_entities` by storing local template identity as compact local IDs and reconstructing qualified names from file instance plus local scope/name.
2. Re-evaluate `idx_template_edges_head_relation` and `idx_template_edges_tail_relation`; they cost `33.08 MiB` together.
3. Compact or eliminate remaining `qname_prefix_dict`, `qualified_name_dict`, and `symbol_dict` rows for template-only identities.
4. Bring single-file update from `1.694s` to `<=750ms`, likely by removing graph hash/check work from the benchmarked fast path or making it incremental-only.
5. Add a real proof-mode switch to Graph Truth and Context Packet fixture gates so proof-mode fixture correctness can be measured directly.
