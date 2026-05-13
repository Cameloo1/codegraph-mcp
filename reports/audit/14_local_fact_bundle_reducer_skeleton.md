# LocalFactBundle and Reducer Skeleton

Date: 2026-05-10 20:34:17 -05:00

Source of truth read first: `MVP.md`.

Additional inputs read:

- `reports/audit/13_incremental_concurrent_indexer_design.md`
- `reports/audit/12_semantic_correctness_gate.md`
- current indexing pipeline in `crates/codegraph-index/src/lib.rs`
- parser/extractor code in `crates/codegraph-parser/src/lib.rs`
- graph writer code in `crates/codegraph-store/src/sqlite.rs`
- manifest/file state model in `crates/codegraph-core/src/model.rs`

## Verdict

Implemented the first isolated map/reduce chunk without changing final extraction semantics.

The indexer now has a richer serializable `LocalFactBundle` surface, a deterministic reducer skeleton with a preliminary symbol table, and a canonical graph fact hash utility for entity/edge comparisons. Persistence still uses the existing extraction behavior and DB writer path.

Semantic gate status: no material regression. The Graph Truth Gate remains 10/11, with the same `derived_closure_edge_requires_provenance` failure from report 12.

## Files Changed

| File | Change |
| --- | --- |
| `crates/codegraph-index/src/lib.rs` | Extended `LocalFactBundle`, added `LocalFactReference`, `PreliminarySymbolTable`, deterministic reducer symbol-table construction, and `graph_fact_hash`. Added unit tests for bundle serialization, reducer ordering, shuffled input determinism, and graph hash stability. |
| `crates/codegraph-parser/src/lib.rs` | Added serde serialization/deserialization derives for `BasicExtraction` so `LocalFactBundle` can round-trip for tests and future bundle artifacts. |
| `reports/audit/14_local_fact_bundle_reducer_skeleton.md` | This report. |

## LocalFactBundle Model

The bundle now includes:

- `repo_relative_path`
- `file_hash`
- `language`
- `source`
- `needs_delete`
- `duplicate_of`
- `declarations`
- `imports`
- `exports`
- `local_callsites`
- `local_reads_writes`
- `unresolved_references`
- `source_spans`
- `extraction_warnings`
- existing `BasicExtraction` for compatibility with the current writer path

The currently supported parser path emits `LocalFactBundle` through `parse_extract_pending_files`. This remains local-only map output; workers do not write cross-file truth.

## Reducer Skeleton

`reduce_local_fact_bundles` now:

- sorts bundles deterministically by repo-relative path
- sorts and deduplicates entities and edges by stable IDs
- builds a preliminary symbol table:
  - `by_id`
  - `by_file`
  - `by_qualified_name`
- records deterministic warnings for conflicting local/global IDs
- preserves existing `BasicExtraction` content for persistence

The reducer is still a skeleton. It does not yet own import/alias/call/security/test/derived resolution. Those store-reread passes remain unchanged for this phase.

## Graph Fact Hash

Added `graph_fact_hash(entities, edges)`.

Behavior:

- canonicalizes entity and edge facts into sorted text lines
- excludes nondeterministic timestamps
- includes entity identity, kind, name, qualified name, path, source span, extractor, and file hash
- includes edge ID, head, relation, tail, source span, exactness, confidence, edge class, context, derived flag, provenance IDs, extractor, and file hash
- sorts provenance IDs before hashing
- hashes the canonical sorted fact lines with the existing `content_hash`

This is the utility that later worker-count determinism checks should use for `--workers 1` versus `--workers N` comparisons.

## Tests

Commands run:

```text
cargo test -q -p codegraph-index
cargo test -q
cargo run -q -p codegraph-cli -- bench graph-truth --cases benchmarks/graph_truth/fixtures --fixture-root <REPO_ROOT>/Desktop\development\codegraph-mcp --out-json reports/audit/artifacts/14_graph_truth_local_fact_bundle_reducer.json --out-md reports/audit/artifacts/14_graph_truth_local_fact_bundle_reducer.md --fail-on-forbidden --fail-on-missing-source-span --fail-on-unresolved-exact --fail-on-derived-without-provenance --fail-on-test-mock-production-leak --update-mode --verbose
```

Results:

- `cargo test -q -p codegraph-index`: passed, 41 passed / 1 ignored.
- `cargo test -q`: passed across the workspace.
- Graph Truth Gate: failed as expected, 10 passed / 1 failed / 11 total.

New or updated unit coverage:

- `local_fact_bundle_exposes_worker_output_categories`
- `local_fact_bundle_serializes_round_trip`
- `local_fact_reducer_sorts_and_deduplicates_facts`
- `local_fact_reducer_output_is_independent_of_bundle_order`
- `graph_fact_hash_is_order_independent_and_semantic`

## Graph Truth Result

Artifact:

- `reports/audit/artifacts/14_graph_truth_local_fact_bundle_reducer.json`
- `reports/audit/artifacts/14_graph_truth_local_fact_bundle_reducer.md`

Strict gate result:

| Metric | Value |
| --- | ---: |
| Fixtures | 10 passed / 1 failed / 11 total |
| Expected edges | 18/20 |
| Forbidden edge hits | 0 |
| Expected paths | 10/11 |
| Forbidden path hits | 0 |
| Source-span failures | 0 |
| Stale failures | 0 |
| Derived edges observed | 0 |

Failure remains unchanged:

- `derived_closure_edge_requires_provenance`
- missing `src/store.ordersTable`
- missing `src/store.saveOrder -WRITES-> src/store.ordersTable`
- missing `src/service.submitOrder -MAY_MUTATE-> src/store.ordersTable`
- missing `path://derived/provenance`

## Performance Impact

No broad runtime parallelization or storage rewrite was introduced.

The skeleton mostly changes in-memory classification and test-only serialization/hash surfaces, so no significant cold-index regression is expected. Graph Truth fixture timing remained in the same small-fixture range; this phase did not perform Autoresearch-scale timing.

## Current Limitations

- `LocalFactBundle` still carries `BasicExtraction` for compatibility. A later phase should split persistence from the immutable bundle model more cleanly.
- The reducer does not yet perform global import/export/call/security/test resolution.
- Existing global resolver passes still reread the SQLite store and source files.
- Stored PathEvidence and derived provenance remain unresolved blockers from report 12.

## Next Implementation Step

Extract pure manifest diff helpers next:

1. Add `FileSnapshot`, `IndexedState`, and `ManifestDiff`.
2. Move metadata-first skip logic into pure tested helpers.
3. Add tests for unchanged metadata, changed metadata, same-hash skip, rename detection, and duplicate same-hash live paths.
4. Keep graph truth checks as the semantic guard after each step.
