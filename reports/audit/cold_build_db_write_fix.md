# Cold Build DB Write Fix

Generated: 2026-05-12 21:51:43 -05:00

Source of truth: `MVP.md`

## Verdict

Status: **fail, materially improved**.

The production `proof-build-only` path is still above the 60s intended target, but the specific DB-write failure called out in the prompt was fixed:

| Metric | Previous / failing value | Current fresh proof-build-only value | Target | Status |
|---|---:|---:|---:|---|
| Cold proof-build-only wall/profile time | 3,001.445s persisted | 297.420s | <=60s | fail |
| Fresh capped run | timed out after >94s | completed | <=60s | fail |
| First batch DB write | 82.209s | 11.430s | must not exceed 60s | pass |
| Proof DB file family size | 171.28 MiB from storage-package run | 171.18 MiB | <=250 MiB | pass |
| DB integrity | ok | ok | ok | pass |
| Graph Truth Gate | 11/11 | 11/11 | 11/11 | pass |
| Context Packet Gate | 11/11 | 11/11 | 11/11 | pass |
| Default query surface | pass | pass | pass | pass |
| `cargo test --workspace` | pass | pass | pass | pass |

This is not a performance victory yet. The first DB-write cliff is gone, and proof storage/correctness stayed green, but the cold proof build is still roughly 4.96x over target.

## Mode Separation

The run measured here is production `proof-build-only`.

It did not include:

- storage audit or `dbstat`
- relation/source-span sampler
- PathEvidence audit sampler
- CGC comparison
- full comprehensive markdown generation
- manual label summary
- repeated DB rebuilds

Validation and audit work remain separate modes:

- `proof-build-only`: build a queryable compact proof DB with quick publish checks.
- `proof-build-plus-validation`: build plus full integrity validation.
- `proof-build-plus-audit`: build plus storage/query/sampler audits.
- `full-comprehensive-gate`: all correctness, quality, storage, performance, and comparison reporting.

## Implementation Changes

The cold-build persistence path was changed in four focused ways:

- Reverse-map writes now use insert-only helpers in cold/after-delete paths, avoiding per-row `DELETE` scans against reverse-map tables while bulk indexes are intentionally absent.
- A transient build-only `idx_entities_entity_hash_build` index exists during bulk load to keep entity-hash lookup bounded, then is dropped before final publish so proof storage size does not regress.
- Derived mutation closure reduction now reads bounded relation-specific edge sets and groups WRITES/MUTATES by head entity instead of expanding and nesting over the full edge surface.
- Stored PathEvidence refresh now samples bounded candidate edges rather than unbounded `list_edges`, preserving fixture context quality while avoiding accidental full graph expansion.

Additional profiling events were added around post-local reduction/application stages so the remaining time is named instead of hidden.

## Fresh Autoresearch Run

Command:

```powershell
cargo run -q -p codegraph-cli -- index <REPO_ROOT>/Desktop\development\autoresearch-codexlab --db <REPO_ROOT>/Desktop\development\codegraph-mcp\reports\audit\artifacts\cold_build_db_write_fix_autoresearch_final_20260512_214058.sqlite --profile --json --workers 16 --storage-mode proof --build-mode proof-build-only
```

Fresh artifact:

`reports/audit/artifacts/cold_build_db_write_fix_autoresearch_final_20260512_214058.sqlite`

Observed profile:

| Metric | Value |
|---|---:|
| Total wall/profile time | 297.420s |
| Total DB write bucket | 220.766s |
| Batch 1 DB write | 11.430s |
| Batches completed | 39/39 |
| Files seen | 8,564 |
| Files indexed | 4,975 |
| Files parsed | 2,555 |
| Files read/hashed | 4,975 / 4,975 |
| Entities emitted | 743,226 |
| Edges emitted | 339,331 |
| Duplicate edges upserted/skipped | 443 |
| Worker count | 16 |
| Storage audit DB size | 179,499,008 bytes |
| Storage audit proof DB MiB | 171.18 MiB |

## Waterfall Profile

These spans are profile spans and can overlap, so the percentages are scale indicators against the 297.420s wall time, not additive accounting.

| Stage | Time | Share of wall | Notes |
|---|---:|---:|---|
| DB write total | 220.766s | 74.23% | Still dominant, but first-batch cliff fixed. |
| extract_entities_and_relations | 65.581s | 22.05% | Local extractor work across parsed files. |
| reducer | 46.346s | 15.58% | Deterministic reduction and symbol/import work. |
| edge_insert | 44.791s | 15.06% | Includes compact edge persistence and file maps. |
| reduce_security_edges | 42.780s | 14.38% | Global security resolver pass. |
| reduce_static_import_edges | 42.043s | 14.14% | Global import/static call resolver pass. |
| reduce_test_edges | 40.908s | 13.75% | Still expensive despite producing no Autoresearch test edges in this run. |
| LocalFactBundle creation | 36.060s | 12.12% | Bundle materialization and clone/copy cost. |
| parse_extract_workers_wall | 27.510s | 9.25% | Worker wall time for local parsing/extraction. |
| parse | 18.297s | 6.15% | Parser time inside worker work. |
| content_template_upsert | 15.699s | 5.28% | Template detection/upsert. |
| entity_insert | 15.406s | 5.18% | Entity persistence. |
| dictionary_lookup_insert | 14.738s | 4.96% | Dictionary writes and lookup cache misses. |
| apply_derived_edges | 9.564s | 3.22% | Derived closure persistence. |
| refresh_path_evidence | 6.776s | 2.28% | Bounded stored PathEvidence refresh. |
| apply_import_edges | 6.545s | 2.20% | Static import/call edge application. |
| path_evidence_generation | 2.812s | 0.95% | PathEvidence construction. |
| path_evidence_insert | 2.361s | 0.79% | PathEvidence persistence. |
| reduce_derived_mutation_edges | 1.080s | 0.36% | Previously broader; now relation-specific and grouped. |
| atomic_final_db_validate | 0.504s | 0.17% | Publish validation. |
| atomic_temp_db_finalize | 0.476s | 0.16% | Atomic finalize. |
| index_creation | 0.337s | 0.11% | Final index creation is no longer the bottleneck. |

No stage over 5% is unknown in this run.

## DB Write Path

| Write sub-stage | Time |
|---|---:|
| `sqlite.edge_insert_sql` | 21.309s |
| `edge_insert` total | 44.791s |
| `sqlite.entity_insert_sql` | 2.620s |
| `entity_insert` total | 15.406s |
| `sqlite.template_entity_insert_sql` | 2.802s |
| `sqlite.template_edge_insert_sql` | 0.821s |
| `dictionary_lookup_insert` | 14.738s |
| `path_evidence_insert` | 2.361s |
| `index_creation` | 0.337s |

The first batch DB write no longer exceeds the 60s target. The remaining DB-write cost is spread across the whole build, with edge persistence, reverse maps, and dictionary lookup/insert still the largest persistence-specific areas.

## Correctness And Query Verification

| Gate | Result |
|---|---|
| Graph Truth Gate | passed, 11/11 |
| Context Packet Gate | passed, 11/11 |
| DB integrity | `integrity_check = ok` from storage audit; atomic publish validation completed |
| Default query surface | passed |
| `cargo test --workspace` | passed |

Default compact proof query p95s from the fresh artifact:

| Query | p95 |
|---|---:|
| entity name lookup | 2.855 ms |
| symbol lookup | 195.252 ms |
| qname lookup | 19.934 ms |
| text/FTS query | 0.211 ms |
| bounded CALLS relation query | 244.211 ms |
| bounded READS/WRITES relation query | 237.771 ms |
| PathEvidence lookup | 3.434 ms |
| source snippet batch load | 52.359 ms |
| context_pack normal | 135.373 ms |
| unresolved-calls paginated | 91.036 ms |

## Storage Safety Questions

| Question | Answer |
|---|---|
| Did graph truth still pass? | Yes, 11/11. |
| Did context packet quality still pass? | Yes, 11/11. |
| Did proof DB size decrease? | This phase was build-time focused. The fresh DB remains compact at 171.18 MiB, within the <=250 MiB target and consistent with the latest storage package. |
| Did removed data move to a sidecar, become derivable, or get proven unnecessary? | No proof data was removed in this phase. The transient entity-hash build index is build-only and deliberately dropped before publish. Bounded PathEvidence candidate refresh preserves stored proof paths needed by the gates; broader audit/debug evidence remains outside the proof-build-only path. |

## Remaining Bottlenecks

The next narrow fixes should target the remaining named cold-build stages:

1. `reduce_test_edges` takes 40.908s while producing no useful Autoresearch test edges. It needs a stronger file-level/source-level prefilter or reuse of the static resolver snapshot.
2. `reduce_security_edges` and `reduce_static_import_edges` each take about 42s. These likely rebuild overlapping symbol/import views and should share one reducer snapshot.
3. `edge_insert` still costs 44.791s total. The next persistence pass should batch reverse-map inserts and compact edge inserts more aggressively with prepared multi-row paths.
4. `LocalFactBundle creation` costs 36.060s. The bundle layer is still copying too much high-cardinality local data during cold build.
5. `extract_entities_and_relations` costs 65.581s. Parser/extractor output should be profiled by entity kind and source-derived template rows before changing semantics.

## Final Status

Cold proof-build-only is not yet within the intended large-codebase target.

The serious correctness boundaries stayed intact: graph truth, context packet quality, DB integrity, proof storage size, and default queries remain green. Optimization may continue, but the next work should be targeted at the three global reducer passes and the remaining edge persistence cost, not at validation/reporting.
