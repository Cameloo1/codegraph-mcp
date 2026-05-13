# 22 Parallel Indexing Design

Source of truth: `MVP.md`. The relevant constraints are deterministic typed graph facts, exact source/span proof, local SQLite storage, and single-agent reasoning. Internal Rust code may use parallelism, but worker concurrency must not make global graph truth nondeterministic.

## Verdict

Current indexing is **partially deterministic and mostly safe at the worker boundary**.

Workers parse/extract files concurrently and return local bundles to the main thread. They do **not** write to SQLite directly. The main thread sorts worker outputs by repo-relative path before persistence, then performs a controlled DB write path.

However, the current design is not yet the clean MVP map/reduce architecture. Local syntax facts are written to DB before cross-file resolution, and the import/security resolvers then read the store and write final global edges directly. That is single-threaded and deterministic in practice, but it is not yet a pure reducer with an immutable global fact plan.

## Code Inspected

| Area | Code locations |
| --- | --- |
| Full cold index flow | `crates/codegraph-index/src/lib.rs::index_repo_to_db_with_options` |
| Batch processing | `crates/codegraph-index/src/lib.rs::process_index_batch` |
| Parser workers | `crates/codegraph-index/src/lib.rs::parse_extract_pending_files` |
| Local extraction | `crates/codegraph-parser/src/lib.rs::extract_entities_and_relations` |
| Manifest diff | `crates/codegraph-index/src/lib.rs::manifest_metadata_matches`, `file_manifest_metadata`, `delete_stale_indexed_files` |
| Repo snapshot/file ordering | `crates/codegraph-index/src/lib.rs::collect_repo_files` |
| Incremental update path | `crates/codegraph-index/src/lib.rs::update_changed_files_with_cache_to_db` |
| Cross-file reducers today | `crates/codegraph-index/src/lib.rs::resolve_static_import_edges_for_store`, `resolve_security_edges_for_store` |
| DB write path | `crates/codegraph-index/src/lib.rs::persist_indexed_files`, `crates/codegraph-store/src/sqlite.rs::{insert_entity_after_file_delete, insert_edge_after_file_delete, upsert_entity, upsert_edge}` |
| Stable IDs | `crates/codegraph-core/src/ids.rs::{stable_entity_id_for_kind, stable_edge_id}` |

## Current Flow

1. `collect_repo_files` walks the repo recursively, skips ignored/generated paths, sorts directory entries, and returns a sorted file list.
2. `index_repo_to_db_with_options` builds a current path set and deletes stale indexed files.
3. For each candidate, it checks metadata first, reads source only when needed, hashes content, detects duplicate source content, and forms deterministic batches.
4. `process_index_batch` chooses `deterministic_worker_count`, calls `parse_extract_pending_files`, receives local outputs, and writes them through `persist_indexed_files`.
5. `parse_extract_pending_files` sorts pending files by path, chunks them, spawns parser workers, joins them, and sorts outputs by path again.
6. `persist_indexed_files` deletes stale facts per changed file, writes file records, entities, source snippets when enabled, and local edge rows.
7. After all batches, `resolve_static_import_edges_for_store` and `resolve_security_edges_for_store` read indexed files/entities and add final resolved global edges.
8. The repo index state is written with count/timestamp metadata.

## Worker Safety

| Question | Answer |
| --- | --- |
| Do parser workers write directly to DB? | No. Workers only produce `IndexedFileOutput` and `ParseExtractStat`. |
| Do workers decide global truth? | Mostly no. They produce local syntax facts, imports/exports, local callsites, reads/writes, source spans, hashes, and warnings. |
| Can workers emit final-looking edges? | Yes for local syntax facts. They can emit parser-derived `CALLS`, `READS`, `WRITES`, etc. Cross-file exact import/security edges are added later. |
| Is output sorted before persistence? | Yes. Pending inputs and worker outputs are sorted by `repo_relative_path`. |
| Is DB writing controlled? | Yes. The cold path uses one SQLite store, one bulk transaction sequence, and a main-thread write loop. |
| Are relation IDs/orderings guaranteed across worker counts? | Edge/entity object IDs are stable. Internal SQLite dictionary row IDs are insertion-order artifacts and should not be used as semantic truth. |

## Determinism Status

Deterministic today:

- File discovery order is sorted.
- Worker input and output ordering is sorted by repo-relative path.
- Entity IDs include normalized path, kind/name, and optional signature hash.
- Edge IDs include head ID, relation, tail ID, and source span.
- Store list queries order entities by qualified name/object ID and edges by compact edge key.

Not fully deterministic or not yet gateable:

- Full cold index does not expose a worker-count override, so a full DB comparison of `1 worker` vs `N workers` cannot be run cleanly yet.
- `indexed_at_unix_ms` and profile timings naturally differ across runs and must be excluded from graph-fact comparisons.
- Cross-file resolvers currently read/write through the DB rather than consuming a formal immutable reducer input, so reducer invariants are not centrally checkable.
- Duplicate-source handling intentionally preserves file records but skips duplicate graph facts; this is deterministic but must stay explicit in future parallel reducers.

## Test Added

Added `crates/codegraph-index/src/lib.rs::tests::parse_extract_worker_counts_produce_same_local_fact_bundles`.

The test builds four TypeScript pending files and compares 1-worker vs 4-worker local outputs. It checks:

- file hashes and file records,
- entity IDs and source spans,
- edge IDs, relation triples, exactness, spans, and extractors,
- parse/skipped/error stat shape, ignoring timing.

This covers the current worker boundary. A full `index same fixture with 1 worker and N workers` test should be added after `IndexOptions` exposes a test-only or public worker-count override.

## Map/Reduce Design

Target architecture:

1. Snapshot manifest:
   - repo identity,
   - normalized sorted file paths,
   - metadata diff state,
   - content hashes only for changed/maybe-changed files,
   - deleted/renamed/duplicate candidates.

2. Deterministic shards:
   - shard ID is derived from sorted path ranges, not scheduler timing,
   - shard membership is logged in profile output,
   - shard output path order is stable.

3. Worker output type:
   - `LocalFactBundle { file, file_hash, declared_symbols, local_callsites, local_reads, local_writes, imports, exports, unresolved_references, source_spans, warnings }`,
   - no SQLite handle,
   - no cross-file exact target decisions,
   - no derived/cache facts,
   - no final proof-path classification.

4. Reducer input:
   - sorted local bundles,
   - previous manifest state,
   - existing live graph facts for affected paths,
   - dependency closure plan for import/export changes.

5. Reducer responsibilities:
   - build global symbol table,
   - resolve imports/aliases/types,
   - classify exact vs heuristic/unresolved,
   - create final base edges,
   - dedupe facts by stable IDs,
   - attach provenance/exactness/context,
   - reject invariant violations before DB merge.

6. Single DB bulk merge:
   - delete stale file facts first,
   - insert files/entities/source spans/base edges in deterministic sorted order,
   - insert resolved/reducer edges in deterministic sorted order,
   - insert derived/cache facts only with provenance,
   - write repo state and profile,
   - rebuild/analyze/vacuum according to existing storage policy.

## Required Invariants

- A worker must never hold `SqliteGraphStore`.
- A worker must never create cross-file exact `CALLS` unless the target is local to its file and proof is local.
- A worker may emit unresolved references, but the reducer owns exactness upgrades.
- The reducer must sort all final entities and edges by stable semantic key before DB insertion.
- SQLite dictionary integer IDs must remain implementation details; reports/tests compare semantic object IDs and facts.
- Source spans must be carried from worker bundle to final edge without broadening.
- Derived/cache facts must include provenance edge IDs before write.
- Test/mock context must be assigned before proof-path construction.

## Recommended Implementation Sequence

1. Add `IndexOptions { worker_count_override: Option<usize> }` or an internal test-only equivalent.
2. Add a full DB determinism test: index one fixture with 1 worker and 4 workers, compare semantic entities/edges/source spans ignoring insertion order, timestamps, and profile timings.
3. Rename or wrap `IndexedFileOutput` as the first `LocalFactBundle`.
4. Move static import/security resolution out of DB-read/write loops into a pure reducer function returning sorted final fact bundles.
5. Add reducer invariant tests for exactness, provenance, duplicate facts, and stale deletes.
6. Keep the single SQLite bulk merge path and only then tune parallelism.

## Expected Impact On 169.9s Indexing

Parallel parsing already exists, so deterministic map/reduce alone is unlikely to turn the 169.9s Autoresearch index into a small number by itself. Previous storage forensics and profile work point to growing SQLite write/index/dictionary costs as major contributors.

Expected impact:

- CPU-bound parser/extractor portions can improve with more workers.
- DB write time remains single-writer and likely dominates large repos unless reducer output reduces duplicate/redundant writes.
- A pure reducer can avoid re-reading/re-parsing sources for global resolution and can write final facts once, which should help correctness and some latency.
- The largest speed gain should come after this design from measured bulk-merge/index/dictionary changes, not from blindly increasing worker count.
