# Context Pack Latency Fix

Generated: 2026-05-11 11:40:22 -05:00

Verdict: `pass` for the Autoresearch `context_pack` target.

## What Changed

`context-pack` no longer opens the full graph query engine for the default CLI path. The new path is:

1. Resolve a bounded set of seed entities through indexed dictionary lookups.
2. Query stored `PathEvidence` first.
3. Load only proof/source spans referenced by selected paths.
4. Use a bounded seed-adjacent edge fallback only when no stored `PathEvidence` matches.
5. Apply hard budgets for seeds, candidate paths, returned proof paths, snippets, and traversal depth.

The old slow work was removed from the normal path: no `list_edges(1_000_000)`, no full in-memory graph preload, and no all-source-file preload.

## Materialized Lookup Support

Added schema support for:

| Structure | Purpose |
| --- | --- |
| `path_evidence_lookup` | source/target/task-class/relation-signature lookup |
| `path_evidence_edges` | ordered edge IDs by `path_id`, `ordinal` |
| `path_evidence_symbols` | entity-to-path lookup |
| `path_evidence_tests` | test evidence lookup |
| `path_evidence_files` | file-to-path lookup |
| `idx_path_evidence_source`, `idx_path_evidence_target` | legacy direct `path_evidence` source/target lookup |

`upsert_path_evidence` now maintains these materialized rows. Existing DBs without the new lookup tables still use the smaller legacy `path_evidence` table instead of falling back to the full edge graph.

## Hard Budgets

For `impact` mode:

| Budget | Value |
| --- | ---: |
| Max seed entities | 32 |
| Max candidate paths | 256 |
| Max returned proof paths | 24 |
| Max snippets | 24 |
| Max traversal depth | 4 |

`normal` mode is tighter. `debug` mode is larger and allows the broader diagnostic surface.

Default expansion excludes high-cardinality structural relations: `CONTAINS`, `DEFINED_IN`, `DECLARES`, and `ARGUMENT_N`.

## Autoresearch Measurement

Command shape:

```powershell
codegraph-mcp --repo <REPO_ROOT>/Desktop\development\autoresearch-codexlab --db reports\audit\artifacts\clean_autoresearch_rerun.sqlite context-pack --task "Change verifyRun behavior" --mode impact --budget 2000 --seed repo://e/22d1e82104748347633070eaa3072049 --profile
```

Five process-level runs:

| Iteration | Wall time |
| --- | ---: |
| 1 | 1,375.42 ms |
| 2 | 18.51 ms |
| 3 | 19.00 ms |
| 4 | 22.00 ms |
| 5 | 19.00 ms |

P95 by nearest-rank over these five runs: `1,375.42 ms`, under the `2,000 ms` target.

The profile itself reports `12.07 ms` inside the command after process startup. Main spans:

| Span | Time |
| --- | ---: |
| `seed_resolution` | 5.93 ms |
| `load_stored_path_evidence_for_context_pack` | 5.52 ms |
| `open_store` | 0.26 ms |
| `snippet_loading` | 0.14 ms |
| `context_pack_graph_and_packet` | 0.17 ms |

Returned packet: 2 verified paths, 1 snippet, source-span coverage 100%.

## Verification

| Check | Result |
| --- | --- |
| Graph Truth Gate with `--update-mode` | passed, 11/11 |
| Context Packet Gate | passed, 11/11 |
| Critical symbol recall | 100% |
| Proof-path coverage | 100% |
| Source-span coverage | 100% |
| Store PathEvidence materialization tests | passed |
| CLI context-pack mode tests | passed |

## Remaining Notes

The measured Autoresearch DB predates the new materialized lookup tables, so its explain plan shows a scan over the small legacy `path_evidence` table. Newly indexed DBs maintain the materialized lookup tables and indexes during `PathEvidence` upsert.

The next performance bug is single-file update, where the profile still points at global cache refresh and a 1,000,000-edge reload.
