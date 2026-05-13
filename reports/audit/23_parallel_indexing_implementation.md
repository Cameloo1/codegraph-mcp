# 23 Parallel Indexing Implementation

Source of truth: `MVP.md`. This phase implements deterministic map/reduce indexing in isolated chunks without changing extraction semantics, benchmark scoring, storage optimization policy, or adding vector/KGE/eBPF/Z3/RL features.

## Verdict

**Implemented with preserved semantics.**

Parser workers now emit explicit `LocalFactBundle` records, local facts pass through a deterministic reducer before persistence, cross-file import/security facts are assembled into reduction plans before a controlled store transaction writes them, and `codegraph-mcp index` now supports `--workers <n>`.

The implementation is still intentionally conservative: it does not attempt broad parallel DB writes or new semantic extraction. SQLite remains a single controlled merge/write boundary.

## Changed Architecture

| Layer | Before | After |
| --- | --- | --- |
| Worker output | `IndexedFileOutput` carrying file source plus `BasicExtraction` | `LocalFactBundle` carrying file path, file hash, declarations, import/export refs, local call refs, local read/write refs, source spans, warnings, source, and extraction |
| Local reducer | Implicit sort before persistence | `reduce_local_fact_bundles` sorts bundles, dedupes entities/edges by stable ID, and records invariant warnings |
| DB local write | `process_index_batch` persisted worker output directly | `process_index_batch` builds a `ReducedIndexPlan`, then persists the reduced plan |
| Cross-file import resolution | Resolver read store and wrote inside the same loop | `reduce_static_import_edges_from_store` builds a deterministic `GlobalFactReductionPlan`; `apply_global_fact_reduction_plan` writes the sorted plan |
| Security/auth/sanitizer resolution | Resolver read store and wrote inside the same loop | `reduce_security_edges_from_store` builds a deterministic `GlobalFactReductionPlan`; `apply_global_fact_reduction_plan` writes the sorted plan |
| Worker control | Worker count chosen from available parallelism | `IndexOptions.worker_count` and CLI `index --workers <n>` can force 1-vs-N test runs |

## Code Locations

| Area | Files/functions |
| --- | --- |
| Local fact model | `crates/codegraph-index/src/lib.rs::LocalFactBundle`, `LocalFactSymbol`, `LocalFactRelation` |
| Local reducer | `crates/codegraph-index/src/lib.rs::reduce_local_fact_bundles`, `ReducedIndexPlan` |
| Worker parse/extract output | `crates/codegraph-index/src/lib.rs::parse_extract_pending_files` |
| Batch map/reduce/write path | `crates/codegraph-index/src/lib.rs::process_index_batch`, `persist_reduced_index_plan` |
| Worker-count plumbing | `crates/codegraph-index/src/lib.rs::IndexOptions`, `effective_worker_count`; `crates/codegraph-cli/src/lib.rs::parse_index_options` |
| Global fact plans | `crates/codegraph-index/src/lib.rs::GlobalFactReductionPlan`, `apply_global_fact_reduction_plan` |
| Import reducer | `crates/codegraph-index/src/lib.rs::reduce_static_import_edges_from_store` |
| Security reducer | `crates/codegraph-index/src/lib.rs::reduce_security_edges_from_store` |
| CLI smoke | `crates/codegraph-cli/tests/cli_smoke.rs::index_status_and_query_commands_work_on_fixture_repo` |

## Determinism Result

New full-index determinism coverage indexes the same TypeScript fixture into two separate SQLite DBs with `worker_count = 1` and `worker_count = 4`, then compares semantic graph facts while ignoring insertion order and volatile index timestamps.

Result: **passed**.

The comparison covers:

- file path/hash/language/size facts,
- entity IDs, kinds, names, qualified names, paths, source spans, and extractor names,
- edge IDs, head/relation/tail triples, source spans, exactness, derived flags, extractors, and provenance edge IDs,
- duplicate edge ID absence in the parallel run.

## Graph Truth Result

Command:

```powershell
cargo run -p codegraph-cli -- bench graph-truth --cases benchmarks\graph_truth\fixtures --fixture-root . --out-json reports\audit\artifacts\23_graph_truth_gate.json --out-md reports\audit\artifacts\23_graph_truth_gate.md --fail-on-forbidden --fail-on-missing-source-span
```

Result:

- Cases: 10 total, 6 passed, 4 failed.
- Forbidden hits: 0.
- Source-span failures: 0.
- This matches the known current correctness shape and does not show a regression from the prior strict gate.

## Speed

No real indexing-speed claim is made in this phase. The deterministic worker override was added so future benchmark phases can run fair `--workers 1` versus `--workers N` comparisons. The only measured runs here were fixture-scale determinism and graph-truth smoke runs, which are not representative of the 169.9s Autoresearch cold-index baseline.

## Remaining Work

- Move more global resolution input away from store snapshots and toward an all-repo immutable reducer input once dependency-closure updates are implemented.
- Add reducer invariant failures for cross-file exactness upgrades, derived provenance, and test/mock production path classification.
- Add indexing profile reports that include requested worker count, effective worker count per batch, parse/extract wall time, and DB merge time for large fixtures.
- Run cold/warm indexing benchmarks on the real corpus before making any performance claim.

## Tests Run

- `cargo test -p codegraph-index --lib`
- `cargo test -p codegraph-cli --test cli_smoke index_status_and_query_commands_work_on_fixture_repo`
- `cargo test -p codegraph-cli --lib parallel_parse_extract_is_deterministic`
- `cargo run -p codegraph-cli -- bench graph-truth --cases benchmarks\graph_truth\fixtures --fixture-root . --out-json reports\audit\artifacts\23_graph_truth_gate.json --out-md reports\audit\artifacts\23_graph_truth_gate.md --fail-on-forbidden --fail-on-missing-source-span`
- `cargo fmt --check`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
