# 13 Stale Update Fix

Source of truth: `MVP.md`.

## Verdict

**Complete with caveats.** Incremental indexing now deletes stale facts before inserting changed facts, can skip unchanged files using stored manifest metadata, and retargets the focused local named-import/call cases covered by the adversarial fixtures.

The fix is deliberately narrow. It does not implement semantic rename identity preservation, barrel/default/dynamic import resolution, dictionary compaction, or a full dependency-closure updater.

## MVP Contract Applied

`MVP.md` treats the graph as an auditable build artifact where stale facts are not acceptable proof. For this phase that means:

- a deleted file must remove file-scoped entities, edges, source spans, and file text;
- a changed file must delete old facts before inserting replacement facts;
- unchanged files should be skipped when manifest metadata proves the skip is safe;
- duplicate file content must not collapse separate path identity;
- rename handling must at least prevent old path facts from surviving.

## Code Changes

| Surface | Files / functions |
| --- | --- |
| Incremental update flow | `crates/codegraph-index/src/lib.rs`: `update_changed_files_to_db` |
| Full index stale cleanup | `crates/codegraph-index/src/lib.rs`: stale file deletion before bulk insert |
| Manifest metadata skip | `crates/codegraph-index/src/lib.rs`: `manifest_metadata_matches`, `modified_unix_nanos`, `file_manifest_metadata` |
| Rename old-path cleanup | `crates/codegraph-index/src/lib.rs`: `delete_missing_files_with_hash` |
| Focused import retargeting | `crates/codegraph-index/src/lib.rs`: `resolve_static_import_edges_for_store` and helpers |
| Canonical file fact deletion | `crates/codegraph-store/src/sqlite.rs`: `delete_facts_for_file` |
| Update CLI path | `crates/codegraph-cli/src/lib.rs`: existing `watch --once --changed` path uses `update_changed_files_to_db` |

## Stale Cleanup Strategy

`delete_facts_for_file` remains the canonical deletion boundary. It removes:

- FTS/source text rows for the repo-relative path;
- edges connected to entities from that path;
- edges whose source span is in that path;
- source spans for that path;
- entities for that path;
- the file row itself.

Incremental indexing now calls cleanup before replacement insert. Missing changed paths are treated as deletes. Changed paths delete old facts, then parse and insert the new extraction. When a newly seen path has the same hash as a missing old indexed path, the old path is cleaned before the new path is inserted, which prevents same-content rename leftovers.

Dictionaries are not pruned in this phase. That is intentional: without refcounts or a proven garbage-collection invariant, deleting dictionary rows can break compact foreign-key lookups. No storage optimization was applied.

## Manifest Diff Behavior

| Case | Current behavior |
| --- | --- |
| Same path, same size and `modified_unix_nanos` | Skip read, hash, parse, and insert. |
| Same path, metadata mismatch | Read and hash before deciding. |
| Same path, changed hash | Delete old facts for the path, then parse and insert new facts. |
| Same path, metadata mismatch but same hash | Skip parse/write after hash confirms unchanged content. |
| Deleted path | Delete facts for that path. |
| Old path gone, new path same hash | Delete missing old-path facts and insert new path facts. |
| Duplicate same-content files | Preserve separate file identity by path; duplicate extraction can reuse payload only after the first path-specific file row exists. |

## Import Alias Retargeting

This phase added a narrow static resolver for local TypeScript/JavaScript named imports. After full and incremental writes, it:

- scans indexed local named imports;
- resolves relative module specifiers against indexed declaration entities;
- creates an `Import` entity for the local alias;
- emits exact `ALIASED_BY` edges from declaration to alias;
- emits exact `CALLS` edges from containing executable symbols to the resolved target when the alias is called.

This is enough for `import_alias_change_updates_target`, `file_rename_prunes_old_path`, and `stale_cache_after_delete`. It is not a TypeScript compiler replacement.

## Fixture Results

| Fixture | Result | Evidence |
| --- | --- | --- |
| `file_rename_prunes_old_path` | Passed | `reports/audit/artifacts/13_graph_truth_file_rename_prunes_old_path.json`, `reports/audit/artifacts/13_graph_truth_file_rename_prunes_old_path.md` |
| `stale_cache_after_delete` | Passed | `reports/audit/artifacts/13_graph_truth_stale_cache_after_delete.json`, `reports/audit/artifacts/13_graph_truth_stale_cache_after_delete.md` |
| `import_alias_change_updates_target` | Passed | `reports/audit/artifacts/13_graph_truth_import_alias_change_updates_target.json`, `reports/audit/artifacts/13_graph_truth_import_alias_change_updates_target.md` |

The graph-truth fixture files describe future mutation replay in their notes. The current graph-truth runner indexes final fixture state, so direct incremental-index tests are the proof for delete/rename/change sequencing.

## Tests Added Or Activated

| Test | Purpose |
| --- | --- |
| `audit_import_alias_points_to_export_target` | Same local alias resolves to the exported declaration target. |
| `audit_import_alias_change_updates_calls_target` | Changing an alias import retargets `CALLS` and avoids stale same-name target edges. |
| `audit_incremental_rename_prunes_old_path_and_retargets_import_call` | Incremental rename removes old path facts and retargets the call to the new file. |
| `audit_incremental_delete_prunes_stale_target_and_retargets_live_import` | Incremental delete removes stale target facts and retargets the call to the live helper. |

## Performance Implications

Repeat indexing can now avoid reading, hashing, parsing, and rewriting files when stored size and modified time match the filesystem metadata. If metadata is missing or changed, the indexer still reads and hashes before deciding, which keeps correctness ahead of blind speed.

The static import resolver currently runs globally after full and incremental writes. That is acceptable for correctness scaffolding, but future work should replace it with dependency closure over import relationships before optimizing large repositories.

## Remaining Risks

- Semantic rename identity mapping is still not implemented.
- Default exports, barrel re-exports, package imports, namespace imports, and dynamic imports are not resolved by the narrow alias resolver.
- Stale facts in downstream dependent files are only repaired for the focused local named-import cases.
- Dictionary garbage collection remains unimplemented and should not be attempted without refcount or reachability measurements.
- Persisted vector/index entries were not present in the audited storage path; if added later, stale cleanup must include them explicitly.

## Recommended Next Work

1. Implement graph-truth mutation replay instead of final-state-only fixture indexing.
2. Add dependency-closure scheduling for importers of changed files.
3. Add compiler/LSP-backed TypeScript import resolution before broadening alias claims.
4. Add dictionary refcount measurements before any compact-store cleanup.
