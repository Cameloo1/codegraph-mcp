# 06 Manifest, Hashing, and Stale Update Audit

Source of truth: `MVP.md`, especially the requirements for file hashes, index manifests, incremental indexing, live directory watching, and proof-oriented graph state. This phase audits behavior only; it does not optimize storage or extraction.

## Verdict

**Partially correct.** Cold indexing, changed-file reparsing, and known deleted-path cleanup work. Repeat indexing can skip parsing and DB writes for unchanged files, but it still reads and hashes each source file before it can skip. Rename detection, old/new identity mapping, dependency closure, metadata-first hashing, and cross-file import alias target updates are not implemented yet.

## Code Inspected

| Area | Files and locations | Finding |
| --- | --- | --- |
| Cold index entry | `crates/codegraph-index/src/lib.rs:269` | Main index flow resolves repo root, opens DB, walks files, deletes stale DB paths, reads/hashes source, then batches parse/extract/persist. |
| Read before hash check | `crates/codegraph-index/src/lib.rs:349`, `crates/codegraph-index/src/lib.rs:368` | Repeat index reads full file text before computing `content_hash` and comparing `files.file_hash`. |
| Stale full-index delete | `crates/codegraph-index/src/lib.rs:560`, `crates/codegraph-index/src/lib.rs:578` | Cold index lists existing DB files and deletes facts for paths absent from the current walk. |
| Incremental update entry | `crates/codegraph-index/src/lib.rs:971` | Changed paths are normalized, sorted, deduped, then handled inside one DB transaction. |
| Incremental delete | `crates/codegraph-index/src/lib.rs:1019` | Missing changed path calls `delete_facts_for_file`. |
| Incremental hash/reparse | `crates/codegraph-index/src/lib.rs:1029`, `crates/codegraph-index/src/lib.rs:1031`, `crates/codegraph-index/src/lib.rs:1039` | Existing file is read, hashed, skipped if unchanged, otherwise old facts are deleted before parse/insert. |
| Stale fact deletion | `crates/codegraph-store/src/sqlite.rs:802` | Deletes edges connected to entities from the path or with spans from the path, then source spans, entities, and file record. |
| Manifest tables | `crates/codegraph-store/src/sqlite.rs:2203`, `crates/codegraph-store/src/sqlite.rs:2212` | `files` stores path, file hash, language, size, indexed time; `repo_index_state` stores repo root, optional commit, counts. |

## Current Flow

### Cold Index

1. Walk the repository and collect supported source files.
2. Delete DB paths that are no longer present in the walk.
3. Read every candidate file into memory.
4. Compute FNV64 source hash.
5. Compare hash against `files.file_hash`.
6. Skip parse/extract/DB writes when hash matches.
7. Detect duplicate content within the current run using an in-memory hash map.
8. Parse/extract/persist changed or new non-duplicate files.

This means the previous full Autoresearch timing of about 169.9s is best interpreted as cold-index behavior unless a repeat-index profile says otherwise. A repeat index can become much faster than cold parsing, but not as fast as a metadata-first manifest diff because it still pays file read and content-hash cost for every supported file.

### Incremental Update

1. Normalize changed paths relative to repo root.
2. If path is missing, delete facts for that path.
3. If unsupported language, skip.
4. Read the changed file and compute hash.
5. If hash matches the stored file hash, skip.
6. Otherwise delete old facts for that path, parse, extract, and insert replacement facts.
7. Refresh the in-memory binary signature and adjacency cache.

There is no observed dependency closure. A change to an exported symbol or import alias reparses the changed file, but dependents are not automatically located and reparsed by the indexer.

## Required Behavior Audit

| Requirement | Current result | Evidence |
| --- | --- | --- |
| Cold index full walk and manifest | Present. | `collect_repo_files`, `files`, `repo_index_state`. |
| Repeat index cheap metadata diff first | Missing. | Source is read before hash comparison. |
| Read/hash/parse only when needed | Partial. | Parse is skipped when hash matches; read/hash is not skipped. |
| Watch/update changed path list | Partial. | Changed paths are handled; dependency closure is not observed. |
| Localized reparse | Present for changed path. | Incremental update deletes/reinserts only the changed path. |
| Dependency closure | Missing. | No import graph closure step found in update flow. |
| Duplicate file separate identity | Partial. | Separate file records exist, but duplicate graph facts are suppressed. |
| Rename detection | Missing. | Full index deletes old and inserts new; no same-hash rename map. |
| Deleted file stale cleanup | Present when the deleted path is observed or full index runs. | `delete_facts_for_file`. |
| Import alias change updates CALLS target | Missing in default indexer. | Cross-file alias target tests are ignored with explicit audit gap. |

## Tests Added

Audit tests were added in `crates/codegraph-index/src/lib.rs`:

| Test | Line | Status | Covers |
| --- | ---: | --- | --- |
| `audit_repeat_index_skips_unchanged_file` | 1717 | Pass | Repeat index skips parse/write for unchanged file and profile counts skipped unchanged files. |
| `audit_changed_file_is_reparsed_and_old_symbols_are_removed` | 1753 | Pass | Changed file reparses and stale symbol disappears. |
| `audit_duplicate_file_content_keeps_separate_file_identity` | 1579 | Pass | Duplicate file content retains distinct file records. |
| `audit_rename_same_content_preserves_semantic_identity_or_records_mapping` | 1607 | Ignored | Desired rename identity preservation/mapping, currently unsupported. |
| `audit_full_reindex_after_rename_deletes_old_path_and_indexes_new_path` | 1777 | Pass | Full reindex cleans old path and indexes new path, without preserving identity. |
| `audit_deleted_file_removes_stale_entities_and_edges` | 1647 | Pass | Deleted path removes stale graph facts. |
| `audit_import_alias_change_updates_calls_target` | 1814 | Ignored | Desired dependency/alias target update, currently unsupported. |

## Storage and Manifest Implications

The manifest is path-first and hash-backed, not metadata-first. That is correct enough to avoid stale facts after known changed/deleted paths, but it cannot yet provide the MVP's cheap repeat-index behavior. Because duplicate files can be represented as file-only duplicates, the DB can undercount per-path graph facts while still looking compact. Storage optimization must not remove file records or path dictionaries until duplicate projection semantics are explicit.

## Safe Measurement Experiments

1. Add instrumentation for bytes read, files statted, files hashed, files parsed, and files skipped during cold and repeat indexing.
2. Compare repeat index wall time before and after a metadata-first manifest prototype on a copied fixture DB.
3. Measure same-content rename cases with old path only, new path only, both paths, and full cold reindex.
4. Measure dependency closure cost on small import graphs before enabling automatic dependent reparses.
5. Compare duplicate extraction caching against full per-path graph projection on a tiny duplicate fixture.

## Unsafe Changes To Avoid For Now

- Do not remove duplicate file records just because duplicate graph facts are suppressed.
- Do not remove edge/path indexes until query-path usage is measured.
- Do not treat unchanged-file skip as fully optimized until file reads and hashing are avoided.
- Do not infer rename identity from content hash alone without path/language/kind/qualified-name checks.
- Do not mark alias-driven CALLS targets exact until resolver provenance is stored.

## Next Fixes Needed

1. Add a durable index manifest with path, normalized path, size, mtime, content hash, language, indexed extractor version, and optional repo commit.
2. Implement metadata-first diffing, then hash only suspicious files.
3. Add rename detection that records old/new path mapping when a missing old path and new same-hash path are observed.
4. Preserve semantic identity or record old/new mapping across renames according to the final MVP identity schema.
5. Add import/export dependency closure for changed files.
6. Integrate semantic resolver output for alias and barrel exports, with exactness/provenance stored on edges.

## Bottom Line

Stale deletes are controlled when the indexer sees the exact path change, and repeat indexing already avoids repeat parsing for unchanged files. The missing pieces are metadata-first skip, rename mapping, duplicate graph projection, and dependency-aware alias updates.
