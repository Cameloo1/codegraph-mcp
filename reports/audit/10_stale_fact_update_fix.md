# Stale Fact Update Fix

Date: 2026-05-10 19:56:51 -05:00

## Verdict

Stale graph fact cleanup is now enforced for current graph truth on file edits, deletes, renames, and import alias retargeting. The three mutation fixtures pass with no stale-update, source-span, forbidden-edge, or mutation failures.

The full Graph Truth Gate still fails overall at 8/11 because role-check extraction, derived write/provenance semantics, and raw flow extraction remain incomplete. Those failures are visible and are not stale lifecycle regressions.

## Cleanup Strategy

- File lifecycle remains current-only for graph truth: deleted or stale files are removed from `files`, `entities`, `edges`, `source_spans`, and Stage 0 FTS rows through `delete_facts_for_file`.
- Stale cleanup now also invalidates cached `PathEvidence` and `derived_edges` rows that reference stale file entities, stale edge provenance, stale source-span paths, or stale path metadata.
- Full reindex prunes indexed files that no longer exist in the current source tree before inserting new facts.
- Incremental update deletes missing-file facts, deletes old facts before changed-file insertion, and prunes missing same-hash old paths on rename while preserving duplicate same-content files that still exist at separate paths.
- Unchanged files are skipped through manifest metadata or same-hash checks. Same-hash skips now refresh the file record metadata so the next run can use the faster metadata-first skip.
- File records now carry explicit lifecycle metadata: `file_lifecycle_state=current`, `file_lifecycle_policy=current_only`, `historical_visibility=not_visible_as_current`, and `stale_cleanup=delete_before_insert`.

## Mutation Fixture Results

Artifact: `reports/audit/artifacts/10_graph_truth_mutation_run.json`

| Fixture | Status | Notes |
| --- | --- | --- |
| `file_rename_prunes_old_path` | passed | Old-path facts are not visible as current graph truth after rename. |
| `import_alias_change_updates_target` | passed | Alias retarget mutation updates the resolved `CALLS` target. |
| `stale_graph_cache_after_edit_delete` | passed | Edited/deleted facts do not survive as current entities, edges, spans, or mutation assertions. |

Full gate result: 11 total, 8 passed, 3 failed.

Remaining failures:

- `admin_user_middleware_role_separation`: missing role entities, `CHECKS_ROLE` edges, role paths, and role context symbols.
- `derived_closure_edge_requires_provenance`: missing table entity, `WRITES`, `MAY_MUTATE`, provenance path, and table context symbol.
- `sanitizer_exists_but_not_on_flow`: missing `FLOWS_TO` edge from raw comment value to write sink.

## Tests

Targeted tests passed:

- `codegraph-store delete_facts_for_file_removes_stale_entities_edges_spans_and_text`
- `codegraph-index audit_repeat_index_skips_unchanged_file`
- `codegraph-index audit_changed_file_is_reparsed_and_old_symbols_are_removed`
- `codegraph-index audit_full_reindex_after_rename_deletes_old_path_and_indexes_new_path`
- `codegraph-index audit_incremental_rename_prunes_old_path_and_retargets_import_call`
- `codegraph-index audit_incremental_delete_prunes_stale_target_and_retargets_live_import`
- `codegraph-index audit_import_alias_change_updates_calls_target`
- `codegraph-index audit_duplicate_file_content_keeps_separate_file_identity`

Workspace tests passed with `cargo test -q`.

## Timing

Small fixture repeat-index probe on `source_span_exact_callsite/repo`:

- First index outer command time: 179.79 ms; internal profile wall time: 40 ms.
- Repeat index outer command time: 145.5 ms; internal profile wall time: 13 ms.
- Repeat index skipped 2/2 files and indexed 0 files.

This is a small fixture probe, not an Autoresearch-scale measurement.

## Next Semantic Bug

Highest priority after stale cleanup is still semantic extraction, not lifecycle cleanup: implement precise `CHECKS_ROLE` role entity/edge extraction and context packet inclusion, then address `WRITES`/`MAY_MUTATE` provenance and `FLOWS_TO` extraction.
