# Storage Optimization Implementation

Status: conservative optimization applied.

This phase implemented only the optimizations explicitly recommended in `reports/audit/19_storage_experiments.md`:

- Delay final secondary-index recreation until the full changed-file indexing run has written local facts, stale cleanup, resolver facts, and repo state.
- Run the existing `ANALYZE`, `PRAGMA optimize`, and `VACUUM` maintenance path at that final bulk-load finish point.

No proof facts, source-span proof metadata, heuristic/debug facts, or default-workflow indexes were removed.

## Optimizations Implemented

| Optimization | Implemented | Why safe |
| --- | --- | --- |
| Final `VACUUM` / `ANALYZE` after bulk load | yes | Report 19 showed copied-DB maintenance preserved measured core/context queries. |
| Delay index creation until after bulk load | yes | Report 19 showed drop/recreate and bulk-load final shape preserved measured queries. |
| Remove full qualified-name text | no | Rejected in report 19 because it breaks human-readable qname output semantics. |
| Drop broad indexes | no | Rejected until source-span/retrieval/default workflow plans are proven. |
| Replace broad index with partial index | no | Rejected until query plans prove it helps required query shapes. |
| Separate exact/base graph | no | Rejected because copied simulation changed graph answers and regressed unresolved-call latency. |
| Disable FTS/source snippets | no | Rejected without a replacement text-index design. |

## Code Changes

- `crates/codegraph-index/src/lib.rs`
  - moved `finish_bulk_index_load()` to after stale cleanup, cross-file resolver writes, and repo-index state writes.
  - kept the existing default secondary indexes intact.
  - added a pipeline test assertion that default lookup indexes still exist after compact indexing.

No schema migration was required.

## Storage Measurement

Fixture baseline: `reports/audit/artifacts/17_fixture_workers4.sqlite`

Optimized fixture DB: `reports/audit/artifacts/20_fixture_storage_optimized.sqlite`

| Metric | Before | After | Delta |
| --- | ---: | ---: | ---: |
| File family bytes | 671,744 | 618,496 | -53,248 |
| Database bytes | 638,976 | 618,496 | -20,480 |
| Edge count | 833 | 833 | 0 |
| Average DB bytes per edge | 806.42 | 742.49 | -63.93 |
| Dictionary table bytes | 114,688 | 114,688 | 0 |
| Unique text index bytes | 122,880 | 110,592 | -12,288 |
| Edge index bytes | 57,344 | 57,344 | 0 |
| FTS bytes | 28,672 | 28,672 | 0 |
| Source-span table/index bytes | 8,192 | 8,192 | 0 |

The compact fixture schema stores proof spans inline on entities/edges, so the `source_spans` table remains empty in both artifacts.

## Query Latency Smoke

Artifact: `reports/audit/artifacts/20_storage_latency_smoke.json`

All measured copied-DB query smoke checks returned `ok`.

| Query | After ms | Status |
| --- | ---: | --- |
| `entity_name_lookup` | 0 | ok |
| `entity_qname_lookup` | 0 | ok |
| `edge_head_relation_lookup` | 0 | ok |
| `edge_tail_relation_lookup` | 0 | ok |
| `edge_span_path_lookup` | 0 | ok |
| `relation_count_scan` | 0 | ok |
| `text_query_fts` | 0 | ok |
| `relation_query_calls` | 0 | ok |
| `context_pack_outbound` | 0 | ok |
| `impact_inbound` | 0 | ok |
| `unresolved_calls_paginated` | 0 | ok |

## Semantic Gates

Graph Truth artifact: `reports/audit/artifacts/20_graph_truth_storage_optimization.json`

| Gate | Result |
| --- | --- |
| Graph Truth | failed, 7 passed / 4 failed / 11 total |
| Context Packet Gate | failed, 0 passed / 11 failed / 11 total |
| Source-span proof validation tests | passed, 11 / 11 |
| Full Rust test suite | passed |

Graph Truth failures:

- `derived_closure_edge_requires_provenance`
- `file_rename_prunes_old_path`
- `import_alias_change_updates_target`
- `stale_graph_cache_after_edit_delete`

Observed Graph Truth failure counts:

- Missing edges: 5
- Forbidden edges found: 2
- Missing paths: 4
- Forbidden paths found: 2
- Source-span failures: 2
- Unresolved-exact violations: 0

These failures are semantic blockers already visible before this storage pass. This implementation does not make a correctness readiness claim.

## Remaining Storage Gap

The fixture DB moved in the right direction, but it is still far above the intended density target:

- After fixture bytes per edge: 742.49
- Intended bytes per stored edge including indexes/dictionaries: 120
- Remaining fixture gap: 622.49 bytes per edge

Autoresearch-scale reindex was not rerun in this phase. Report 19's copied frozen-baseline experiment remains the large-DB evidence: maintenance-only optimization saved about 56.09 MiB, while larger reductions require unsafe schema/query changes that were not implemented.

## Commands Run

- `cargo test -q -p codegraph-index compact_default_storage_preserves_mvp_relations_and_spans`
- `cargo test -q -p codegraph-store finish_bulk_index_load`
- `cargo test -q`
- `cargo run -q -p codegraph-cli --bin codegraph-mcp -- bench graph-truth --cases benchmarks\graph_truth\fixtures --fixture-root benchmarks\graph_truth\fixtures --out-json reports\audit\artifacts\20_graph_truth_storage_optimization.json --out-md reports\audit\artifacts\20_graph_truth_storage_optimization.md --fail-on-forbidden --fail-on-missing-source-span --fail-on-unresolved-exact --fail-on-derived-without-provenance --fail-on-test-mock-production-leak`
- `cargo run -q -p codegraph-cli --bin codegraph-mcp -- bench context-packet --cases benchmarks\graph_truth\fixtures --fixture-root benchmarks\graph_truth\fixtures --out-json reports\audit\artifacts\20_context_packet_storage_optimization.json --out-md reports\audit\artifacts\20_context_packet_storage_optimization.md --top-k 5 --budget 2000`
- `cargo test -q -p codegraph-query proof_span_validation`
- `cargo run -q -p codegraph-cli --bin codegraph-mcp -- --db <REPO_ROOT>/Desktop\development\codegraph-mcp\reports\audit\artifacts\20_fixture_storage_optimized.sqlite index benchmarks\graph_truth\fixtures --profile --json --workers 4`
- `cargo run -q -p codegraph-cli --bin codegraph-mcp -- audit storage --db reports\audit\artifacts\17_fixture_workers4.sqlite --json reports\audit\artifacts\20_storage_before_fixture.json --markdown reports\audit\artifacts\20_storage_before_fixture.md`
- `cargo run -q -p codegraph-cli --bin codegraph-mcp -- audit storage --db reports\audit\artifacts\20_fixture_storage_optimized.sqlite --json reports\audit\artifacts\20_storage_after_fixture.json --markdown reports\audit\artifacts\20_storage_after_fixture.md`
- `cargo run -q -p codegraph-cli --bin codegraph-mcp -- audit storage-experiments --db reports\audit\artifacts\20_fixture_storage_optimized.sqlite --workdir reports\audit\artifacts\20_storage_latency_work --json reports\audit\artifacts\20_storage_latency_smoke.json --markdown reports\audit\artifacts\20_storage_latency_smoke.md`

## Verdict

The safe storage optimization is implemented and measured. It preserves the default graph fact surface and default lookup indexes, and it reduces the fixture DB family by 53,248 bytes.

The system is still not ready for more aggressive storage optimization because Graph Truth and context-packet gates are failing.
