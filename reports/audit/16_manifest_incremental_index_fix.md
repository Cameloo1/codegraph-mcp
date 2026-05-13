# Manifest Incremental Index Fix

Timestamp: 2026-05-10 20:52:47 -05:00

## Verdict

Status: implemented.

Repeat indexing and explicit incremental updates now use a metadata-first manifest flow with visible skip/read/hash/parse/delete/rename counters. Mutation graph-truth fixtures pass. The full strict Graph Truth Gate remains 10/11 because of the pre-existing `derived_closure_edge_requires_provenance` semantic gap, not because of stale update behavior.

## Implemented Flow

The indexer now follows this manifest decision order:

1. Walk repo files and record `files_walked`.
2. Filter unsupported/ignored files.
3. Compare existing manifest metadata by path, size, mtime, and lifecycle metadata.
4. If metadata is definitely unchanged, skip read/hash/parse and increment `files_metadata_unchanged`.
5. If changed or unknown, read source and increment `files_read`.
6. Hash source and increment `files_hashed`.
7. If hash is unchanged, skip parse and refresh file metadata.
8. If hash changed, delete old facts for that path before writing replacement facts.
9. If a new path has the same content hash as a missing old path, count it as `files_renamed` and use cleanup+insert policy.
10. Delete all missing old paths before semantic resolvers run.

Rename policy is explicit cleanup+insert, not ID remap. Duplicate file content at different paths is still indexed as distinct file and symbol identity.

## Metrics Added

Both full indexing and incremental update summaries now expose:

- `files_walked`
- `files_metadata_unchanged`
- `files_read`
- `files_hashed`
- `files_parsed`
- `files_deleted`
- `files_renamed`

Existing counters remain:

- `files_seen`
- `files_indexed`
- `files_skipped`
- `stale_files_deleted` for full indexing
- `files_ignored` for incremental updates

The watch summary now prints the new incremental counters.

## Code Changes

- `crates/codegraph-index/src/lib.rs`
  - Added manifest diff decision engine.
  - Delayed full-index stale cleanup until after new-path hash comparison so same-hash renames can be classified.
  - Added metadata/read/hash/parse/delete/rename counters to full and incremental summaries.
  - Preserved hash-unchanged metadata refresh without parsing.
  - Preserved delete cleanup for removed paths and parse failures.
  - Added manifest diff and incremental skip/reparse/rename tests.
- `crates/codegraph-cli/src/lib.rs`
  - Extended watch logging with manifest counters.

## Verification

| Check | Result |
| --- | --- |
| `cargo test -q -p codegraph-index` | passed: 44 passed, 1 ignored |
| `cargo test -q` | passed: all workspace tests passed |
| Graph Truth Gate | failed: 10 passed / 1 failed / 11 total |
| Mutation fixtures | passed: `file_rename_prunes_old_path`, `import_alias_change_updates_target`, `stale_graph_cache_after_edit_delete` |

Graph Truth artifact:

- JSON: `reports/audit/artifacts/16_graph_truth_incremental.json`
- Markdown: `reports/audit/artifacts/16_graph_truth_incremental.md`

The only strict Graph Truth failure is unchanged:

- `derived_closure_edge_requires_provenance`: missing expected entity `src/store.ordersTable`
- `derived_closure_edge_requires_provenance`: missing required `WRITES`
- `derived_closure_edge_requires_provenance`: missing required `MAY_MUTATE`
- `derived_closure_edge_requires_provenance`: missing expected provenance path

## Repeat-Index Timing

Measured on `benchmarks/graph_truth/fixtures` with `--workers 4` and DB `reports/audit/artifacts/16_repeat_index.sqlite`.

| Run | Files Walked | Metadata Unchanged | Read | Hashed | Parsed | Indexed | Skipped | Deleted | Renamed | Wall Time | DB Write |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Cold | 49 | 0 | 28 | 28 | 28 | 28 | 21 | 0 | 0 | 188 ms | 149 ms |
| Repeat unchanged | 49 | 28 | 0 | 0 | 0 | 0 | 49 | 0 | 0 | 85 ms | 61 ms |

The repeat run avoided all source reads, hashes, and parses for the 28 indexed source files. The remaining time is repo walk, opening SQLite, resolver no-op checks, state refresh, and DB bookkeeping.

## Stale Cleanup Behavior

- Deleted files remove current file/entities/edges/source snippets/path evidence through `delete_facts_for_file`.
- Renamed files use cleanup+insert: the missing old path is counted as a rename when a new path has the same content hash, then old facts are deleted before global semantic resolvers run.
- Changed files still delete old facts for the same path before inserting new facts.
- Hash-unchanged files update manifest metadata and skip parse.
- Duplicate same-content files at different paths are not treated as stale duplicates and preserve separate identity.

## Remaining Bottlenecks

- Repeat indexing still walks the full tree.
- Store-backed global resolvers still run after skipped repeats, even when they are mostly no-op.
- DB bookkeeping is now the dominant cost on tiny corpora.
- Peak memory is still not reported.
- Single-file update latency was covered by integration tests but not separately benchmarked in this phase.
