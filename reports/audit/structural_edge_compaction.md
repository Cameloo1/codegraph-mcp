# Structural Edge Compaction

Generated: 2026-05-11 13:07:47 -05:00

## Verdict

Implemented with semantic gates passing.

The listed high-cardinality bookkeeping relations are no longer written to the generic proof `edges` table on new indexes. They are stored in compact structural/callsite tables and synthesized back through the graph store APIs, so relation counts, graph truth comparisons, and context packet assembly still see the same semantic facts.

## Relation Decisions

| Relation | Decision | Storage |
| --- | --- | --- |
| `CONTAINS` | move out of proof edge table | `structural_relations`; also updates `entities.parent_id` / `entities.scope_id` for the child |
| `DEFINED_IN` | move out of proof edge table | `structural_relations`; also updates parent/scope attributes for the defined entity |
| `DECLARES` | move out of proof edge table | `structural_relations`; also updates parent/scope attributes for the declared entity |
| `CALLEE` | move to callsite model | `callsites` |
| `ARGUMENT_0` | move to callsite model | `callsite_args` with ordinal `0` |
| `ARGUMENT_1` | move to callsite model | `callsite_args` with ordinal `1` |
| `ARGUMENT_N` | move to callsite model | `callsite_args` with ordinal `-1` for N/overflow |

Proof-relevant relations remain in `edges`: `CALLS`, `READS`, `WRITES`, `FLOWS_TO`, `MUTATES`, `AUTHORIZES`, `CHECKS_ROLE`, `SANITIZES`, `EXPOSES`, `TESTS`, `ASSERTS`, `MOCKS`, and `STUBS`.

## Schema Changes

- Added `entities.parent_id`, `entities.file_id`, `entities.scope_id`, and `entities.declaration_span_id`.
- Added `structural_relations`.
- Added `callsites`.
- Added `callsite_args`.
- Bumped SQLite schema version to `8`.
- Kept source spans, exactness, confidence, extractor, edge class, production/test/mock context, and file hash on compact structural/callsite rows.
- Did not add broad secondary indexes on compact tables in this phase; default proof/context workflows should stay on proof edges and stored PathEvidence.

## Storage Measurement

Clean Autoresearch baseline from the Clean Autoresearch Rerun report:

| Metric | Before |
| --- | ---: |
| DB bytes | 1,712,730,112 |
| Generic `edges` rows | 3,866,431 |
| `edges` table bytes | 573,857,792 |

Copied-DB structural compaction simulation on `reports/audit/artifacts/structural_compaction_autoresearch_sim.sqlite`:

| Metric | After |
| --- | ---: |
| DB bytes | 1,265,242,112 |
| Proof edge rows | 855,108 |
| Structural relation rows | 2,019,607 |
| Callsite rows | 524,760 |
| Callsite argument rows | 466,956 |
| Semantic edge/fact rows | 3,866,431 |
| `edges` table bytes | 131,022,848 |
| Compact structural/callsite table bytes | 213,929,984 |

Measured reduction on the copied Autoresearch DB: 447,488,000 bytes, about 426.76 MiB, or 26.13%.

The simulation mutates only a copied DB and passed `PRAGMA integrity_check`. The fresh fixture DB shows the same physical split, but the tiny fixture corpus is dominated by fixed SQLite table/index overhead, so the large-repo copy is the meaningful storage evidence.

## Gates

| Gate | Result |
| --- | --- |
| Graph Truth Gate | pass, 11/11 |
| Context Packet Gate | pass, 11/11 |
| Full Rust test suite | pass |
| Fixture context-pack smoke | pass, 9.63 ms wall profile |
| Autoresearch copied-DB integrity | pass |

## Artifacts

- `reports/audit/artifacts/structural_compaction_graph_truth.json`
- `reports/audit/artifacts/structural_compaction_graph_truth.md`
- `reports/audit/artifacts/structural_compaction_context_packet.json`
- `reports/audit/artifacts/structural_compaction_context_packet.md`
- `reports/audit/artifacts/structural_compaction_fixture.sqlite`
- `reports/audit/artifacts/structural_compaction_storage.json`
- `reports/audit/artifacts/structural_compaction_storage.md`
- `reports/audit/artifacts/structural_compaction_autoresearch_sim.sqlite`
- `reports/audit/artifacts/structural_compaction_autoresearch_storage.json`
- `reports/audit/artifacts/structural_compaction_autoresearch_storage.md`
- `reports/audit/artifacts/structural_compaction_relation_counts.json`
- `reports/audit/artifacts/structural_compaction_relation_counts.md`

## Notes

- `count_edges`, `list_edges`, endpoint relation lookups, relation counts, graph digests, and file-scoped stale cleanup now include compact facts.
- Compact rows are not counted as raw proof edges in storage accounting; audit output now separates proof edge rows from structural/callsite rows.
- The large-repo context-pack p95 should be remeasured on a fresh full reindex when time permits. The fixture context gate and fixture context-pack smoke did not regress.
