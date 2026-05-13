# Dictionary And Index Bloat Compaction

Generated: 2026-05-11 13:24:29 -05:00

## Verdict

Implemented with semantic gates passing.

This phase removes the largest redundant dictionary storage layer: full qualified-name strings are no longer stored as canonical rows with a huge unique text index. Qualified names are stored as `(prefix_id, suffix_id)` and exposed through `qualified_name_lookup` / `qualified_name_debug` views for human-readable queries.

Large mutable string dictionaries now use compact `value_hash, value_len` indexes with exact stored-string verification instead of full-text unique indexes:

- `object_id_dict`
- `symbol_dict`
- `qname_prefix_dict`

`path_dict` remains unchanged because it is small in the clean Autoresearch artifact and still benefits from direct unique path lookup. Edge/source-span rows already use `path_id` and `span_path_id`; no path strings are stored in proof edge rows.

## Schema Changes

- Bumped SQLite schema version to `9`.
- Added deterministic `codegraph_text_hash()` migration function backed by the same FNV-style stable hash family used by graph digests.
- Rebuilt `object_id_dict`, `symbol_dict`, and `qname_prefix_dict` without `UNIQUE(value)`.
- Added:
  - `idx_object_id_dict_hash`
  - `idx_symbol_dict_hash`
  - `idx_qname_prefix_dict_hash`
  - `idx_qualified_name_parts`
- Rebuilt `qualified_name_dict` so `value` is nullable and no longer canonical.
- Added `qualified_name_lookup`, `qualified_name_debug`, and `object_id_debug` views.
- Preserved dictionary IDs during migration so entity/edge/source-span foreign key style references remain stable.

## Storage Measurement

Measurements use the copied Autoresearch-scale artifact from the structural compaction phase:

`reports/audit/artifacts/dictionary_index_compaction_autoresearch_sim.sqlite`

The copy was migrated, then `VACUUM`/`ANALYZE` were run on the copy only before dbstat measurement.

| Metric | Bytes | MiB |
| --- | ---: | ---: |
| Clean Autoresearch DB from clean rerun | 1,712,730,112 | 1,633.39 |
| After structural/callsite compaction | 1,265,242,112 | 1,206.63 |
| After dictionary/index compaction | 860,700,672 | 820.83 |
| Reduction from structural baseline | 404,541,440 | 385.80 |
| Reduction from clean rerun baseline | 852,029,440 | 812.56 |

Key dictionary deltas on the copied Autoresearch DB:

| Area | Before | After | Delta |
| --- | ---: | ---: | ---: |
| `qualified_name_dict` + qname index | 380.13 MiB | 45.03 MiB | -335.11 MiB |
| `object_id_dict` + lookup index | 157.59 MiB | 125.00 MiB | -32.59 MiB |
| `qname_prefix_dict` + lookup index | 80.28 MiB | 50.25 MiB | -30.03 MiB |
| `symbol_dict` + lookup index | 51.18 MiB | 48.37 MiB | -2.81 MiB |

Top remaining contributors after compaction:

| Object | Bytes |
| --- | ---: |
| `structural_relations` | 139,460,608 |
| `edges` | 131,022,848 |
| `entities` | 119,382,016 |
| `object_id_dict` | 99,954,688 |
| `qname_prefix_dict` | 46,112,768 |
| `callsites` | 41,451,520 |
| `symbol_dict` | 35,655,680 |
| `callsite_args` | 33,017,856 |
| `idx_object_id_dict_hash` | 31,113,216 |
| `qualified_name_dict` | 24,363,008 |

The DB is still above the intended `250 MiB` target. The next storage target is real-row compaction in entities/object IDs/structural tables, not another qname-index pass.

## Correctness Gates

| Gate | Result |
| --- | --- |
| Graph Truth Gate | pass, 11/11 |
| Context Packet Gate | pass, 11/11 |
| Full Rust test suite | pass |
| Copied Autoresearch DB integrity_check | pass |
| Large unresolved-calls page smoke | pass, about 10 ms total |

## Notes

- `qualified_name_dict.value` is now debug/backward-compatibility storage only; new rows store `NULL` there.
- Human-readable qualified names are reconstructed from `qname_prefix_dict.value` and `symbol_dict.value`.
- Hash/length dictionary lookup never trusts hash equality alone; it scans same-hash candidates and verifies the exact string.
- A single global string table was not introduced in this phase. Retaining the purpose-specific dictionary IDs avoided a broad ID-model rewrite and kept graph truth/context semantics stable. This remains a future storage experiment if object/prefix/symbol string payload becomes the next bottleneck.

## Artifacts

- `reports/audit/artifacts/dictionary_index_compaction_autoresearch_sim.sqlite`
- `reports/audit/artifacts/dictionary_index_compaction_autoresearch_storage.json`
- `reports/audit/artifacts/dictionary_index_compaction_autoresearch_storage.md`
- `reports/audit/artifacts/dictionary_index_compaction_graph_truth.json`
- `reports/audit/artifacts/dictionary_index_compaction_graph_truth.md`
- `reports/audit/artifacts/dictionary_index_compaction_context_packet.json`
- `reports/audit/artifacts/dictionary_index_compaction_context_packet.md`
