# Parallel Indexer Implementation

Timestamp: 2026-05-10 20:44:09 -05:00

## Verdict

Status: implemented for deterministic local parsing, bundle reduction, and single-process deterministic writing.

Semantic status: no material regression. The strict Graph Truth Gate remains 10/11, with the same `derived_closure_edge_requires_provenance` failure documented in reports 12 and 14.

Proceeding to storage/indexing optimization remains blocked by the existing semantic gap unless it is explicitly scoped as unsupported. This phase did not optimize storage layout.

## Worker Architecture

The indexing path now has an explicit deterministic worker contract:

1. The CLI accepts `index <repo> --workers <N>`.
2. The indexer builds canonical repo-relative pending file paths and sorts them before worker assignment.
3. `parse_extract_pending_files` shards the sorted path list into stable contiguous chunks.
4. Worker threads only parse source and run local extraction.
5. Workers emit `LocalFactBundle` values containing declarations, imports, exports, local callsites, local reads/writes, unresolved references, source spans, and extraction warnings.
6. Workers do not write to SQLite and do not decide final cross-file graph truth.
7. `reduce_local_fact_bundles` sorts/deduplicates bundles and local facts, builds the preliminary symbol table, and now emits deterministic reducer-owned global facts for static import aliases, exact import edges, exact imported call targets, and unresolved dynamic import evidence when those facts can be resolved from the current bundle set.
8. The main indexer thread is the controlled writer. `persist_reduced_index_plan` persists local bundle facts first, then deterministic reducer global facts, inside the existing bulk-load write path.

The existing single-thread store-backed semantic resolvers still run after bulk loading for full-repo compatibility, especially cross-batch resolution and security/test relations. They are not worker-owned and remain deterministic, but a later phase should move more of this work behind a whole-repo reducer boundary.

## Determinism Rules

- File input order is canonical repo-relative path order.
- Worker sharding is derived only from sorted paths and requested worker count.
- Worker outputs are sorted again by repo-relative path after join.
- Reducer facts are sorted and deduplicated by stable IDs before persistence.
- The graph fact hash canonicalizes entities and edges and excludes nondeterministic timestamps.
- The deterministic hash includes relation identity, edge endpoints, source spans, exactness, derived/provenance state, class/context, file hashes, and extractor identity.

## Code Changes

- `crates/codegraph-index/src/lib.rs`
  - Added serializable reducer global fact types.
  - Extended `ReducedIndexPlan` with deterministic `global_facts`.
  - Added bundle-backed static import/alias/call reduction.
  - Added bundle-backed barrel re-export resolution for static imports.
  - Added deterministic persistence of reducer global facts through the main writer path.
  - Added reducer and full-index worker-count determinism tests.

The CLI `--workers <N>` option was already present and is now covered by the determinism path.

## Verification

| Check | Result |
| --- | --- |
| `cargo test -q -p codegraph-index` | passed: 42 passed, 1 ignored |
| `cargo test -q` | passed: all workspace tests passed |
| Graph Truth Gate | failed: 10 passed / 1 failed / 11 total |
| Worker determinism test | passed: workers 1, 2, and 4 produce identical semantic facts and graph fact hashes |
| Duplicate-edge test | passed for parallel worker DBs |

Graph Truth command:

```powershell
cargo run -q -p codegraph-cli -- bench graph-truth --cases benchmarks\graph_truth\fixtures --fixture-root benchmarks\graph_truth\fixtures --out-json reports\audit\artifacts\15_graph_truth_parallel_indexer.json --out-md reports\audit\artifacts\15_graph_truth_parallel_indexer.md --fail-on-forbidden --fail-on-missing-source-span --fail-on-unresolved-exact --fail-on-derived-without-provenance --fail-on-test-mock-production-leak --update-mode --verbose
```

Graph Truth failure details:

- `derived_closure_edge_requires_provenance`: missing expected entity `src/store.ordersTable`
- `derived_closure_edge_requires_provenance`: missing required edge `src/store.saveOrder -WRITES-> src/store.ordersTable at src/store.ts:4`
- `derived_closure_edge_requires_provenance`: missing required edge `src/service.submitOrder -MAY_MUTATE-> src/store.ordersTable at src/service.ts:4`
- `derived_closure_edge_requires_provenance`: missing expected path `path://derived/provenance`

No forbidden edge/path hits, source-span failures, unresolved-exact violations, derived-without-provenance violations, test/mock leakage failures, or stale mutation failures were reported in the phase 15 Graph Truth artifact.

## Fixture-Corpus Timing

Measured on `benchmarks/graph_truth/fixtures` using absolute DB paths under `reports/audit/artifacts`.

| Workers | Files Indexed | Entities | Edges | Total Wall | DB Write | Parse | Extraction |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 28 | 429 | 833 | 200 ms | 154 ms | 0 ms | 3 ms |
| 4 | 28 | 429 | 833 | 181 ms | 143 ms | 0 ms | 3 ms |

The tiny fixture corpus shows a small wall-clock improvement with 4 workers, but the run is dominated by SQLite writes and index maintenance rather than parsing. This is expected at this scale.

## Remaining Bottlenecks

- DB write time dominates small repos.
- Full-repo semantic resolver work still happens after bulk writing; this is deterministic and main-thread controlled, but it is not yet a pure pre-write whole-repo reducer.
- Batch-local reducer resolution can only see entities in the current bundle set. Cross-batch correctness is still covered by the store-backed resolver.
- `memory_bytes` is still `null`; peak-memory tracking remains future work.
- The highest-priority semantic blocker remains provenance-backed table/sink mutation semantics for `derived_closure_edge_requires_provenance`.

## Next Step

Move the remaining store-backed global resolvers toward a whole-repo reducer plan that can be built before final persistence, while preserving the current Graph Truth result and keeping all proof-grade edge classification/source-span checks intact.
