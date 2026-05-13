# Structural Relation Compaction

Generated: 2026-05-11 23:28:52 -05:00

## Verdict

Status: **passed with storage target shortfall**

The high-cardinality structural relation rows are no longer stored in `structural_relations` for compact proof storage. `CONTAINS`, `DEFINED_IN`, and `DECLARES` are represented through compact entity placement attributes and structural flags, while `CALLEE` and `ARGUMENT_*` remain in the existing typed callsite tables. Query compatibility is preserved through store adapters and relation-count reporting.

The semantic gates passed, but the measured incremental size reduction was **53.97 MiB**, below the requested 80 MiB target.

## Four-Question Storage Rule

| Question | Result |
| --- | --- |
| Did graph truth still pass? | Yes. Graph Truth Gate passed 11/11. |
| Did context packet quality still pass? | Yes. Context Packet Gate passed 11/11 with 100% critical symbol recall and 100% proof-path coverage. |
| Did proof DB size decrease? | Yes. VACUUM-after copied artifact size decreased from 432,730,112 bytes to 376,135,680 bytes, saving 56,594,432 bytes / 53.97 MiB versus the previous compact proof measurement. |
| Did removed data move to a sidecar, become derivable, or get proven unnecessary? | Yes. Removed generic rows became derivable from `entities.parent_id`, `entities.scope_id`, `entities.file_id`, `entities.declaration_span_id`, and `entities.structural_flags`; callsite structural facts remain in typed `callsites` / `callsite_args` tables. |

## Relation Classification

| Relation | Previous generic representation | Compact representation | Query compatibility |
| --- | --- | --- | --- |
| `CONTAINS` | `structural_relations` row | `entities.parent_id`, `entities.scope_id`, `entities.structural_flags` contained bit | `list_edges`, relation counts, and head/tail relation lookups synthesize compatible structural facts. |
| `DEFINED_IN` | `structural_relations` row | `entities.parent_id`, `entities.scope_id`, `entities.file_id`, `entities.structural_flags` defined-in bit | Same adapter path as `CONTAINS`; source span identity is attached to the entity declaration span. |
| `DECLARES` | `structural_relations` row | `entities.parent_id`, `entities.scope_id`, `entities.structural_flags` declared-by-parent bit | Same adapter path as `CONTAINS`. |
| `CALLEE` | Typed callsite table already | `callsites.callee_entity_id` | Existing callsite adapter remains the compatibility layer. |
| `ARGUMENT_0` | Typed callsite args table already | `callsite_args` with ordinal `0` | Existing callsite adapter remains the compatibility layer. |
| `ARGUMENT_1` | Typed callsite args table already | `callsite_args` with ordinal `1` | Existing callsite adapter remains the compatibility layer. |
| `ARGUMENT_N` | Typed callsite args table already | `callsite_args` with ordinal `N` | Existing callsite adapter remains the compatibility layer. |

## Implementation Summary

- Added schema version 14 with `entities.structural_flags`.
- Added migration from `structural_relations` into indexed temporary parent maps, then compact entity attributes.
- Stopped new `CONTAINS`, `DEFINED_IN`, and `DECLARES` writes from creating physical generic structural rows.
- Added compatibility synthesis for structural relations in edge listing, relation counts, source-span counts, and head/tail relation lookups.
- Preserved `CALLEE` and `ARGUMENT_*` through typed `callsites` and `callsite_args`.
- Kept proof-relevant relations in the proof edge table.
- Confirmed context packet default traversal already excludes broad structural relations by default.

## Measurements

Autoresearch-scale copied artifact:

| Metric | Value |
| --- | ---: |
| `structural_relations` rows after migration | 0 |
| `CONTAINS` compatible facts | 907,170 |
| `DEFINED_IN` compatible facts | 183,023 |
| `DECLARES` compatible facts | 214,406 |
| `CALLEE` typed facts | 38,367 |
| `ARGUMENT_0` typed facts | 332,104 |
| `ARGUMENT_1` typed facts | 73,913 |
| `ARGUMENT_N` typed facts | 60,939 |
| Copied DB before VACUUM/ANALYZE | 606,109,696 bytes |
| Copied DB after VACUUM/ANALYZE | 376,135,680 bytes |
| Incremental saved vs previous compact proof measurement | 56,594,432 bytes / 53.97 MiB |
| Remaining gap to 250 MiB target | 108.71 MiB |

The unvacuumed copy contained a large freelist, so the 229,974,016-byte VACUUM delta is not credited entirely to this phase. The stricter phase-to-phase comparison credits 53.97 MiB.

## Gate Results

| Gate | Result |
| --- | --- |
| Graph Truth Gate | Passed, 11/11 fixtures |
| Context Packet Gate | Passed, 11/11 fixtures |
| Relation query compatibility | Passed through store tests and relation-count audit |
| Source-span preservation | Passed; compatible structural and callsite facts report source spans |
| DB integrity on copied artifact | Passed |
| Full Rust test suite | Passed |

## Artifacts

- `reports/audit/artifacts/structural_relation_compaction_graph_truth.json`
- `reports/audit/artifacts/structural_relation_compaction_graph_truth.md`
- `reports/audit/artifacts/structural_relation_compaction_context_packet.json`
- `reports/audit/artifacts/structural_relation_compaction_context_packet.md`
- `reports/audit/artifacts/structural_relation_compaction_relation_counts.json`
- `reports/audit/artifacts/structural_relation_compaction_relation_counts.md`
- `reports/audit/artifacts/structural_relation_compaction_storage.json`
- `reports/audit/artifacts/structural_relation_compaction_storage.md`
- `reports/audit/artifacts/structural_relation_compaction_storage_experiments.json`
- `reports/audit/artifacts/structural_relation_compaction_storage_experiments.md`

## Caveats

- `audit storage` aggregate `semantic_edge/fact rows` is still a physical-table-oriented accounting number and does not fully include attribute-derived structural compatibility facts. `audit relation-counts` is the authoritative relation compatibility count for this phase.
- This phase did not move `callsite_args` into a smaller encoding; it only preserved the existing typed representation.
- The 80 MiB target was not reached. The next storage pressure is still in `entities`, dictionary tables, entity indexes, and `callsite_args`.

## Next Work

1. Make storage audit aggregate metrics aware of attribute-derived structural facts so size accounting and semantic counts are not split across reports.
2. Compact `callsite_args` further with ordinal/value packing or a narrower typed table if graph truth and context gates continue to pass.
3. Continue reducing `entities` and dictionary/index bloat, because `structural_relations` is now effectively empty.
