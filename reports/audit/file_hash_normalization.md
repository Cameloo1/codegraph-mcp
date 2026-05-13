# File Hash Normalization

Generated: 2026-05-11 20:35:37 -05:00

## Verdict

Passed for this storage slice.

`file_hash` is no longer stored as repeated text payload in proof fact tables. Fact rows now carry `file_id`; the canonical file hash lives once in `files.content_hash` and is joined back only for compatibility in read APIs and audit output.

The copied Autoresearch artifact shrank by `117,186,560` bytes, or `111.76 MiB`, after schema migration plus `VACUUM`. That clears the requested `>=75 MiB` target for this change.

## Four Storage-Change Questions

| Question | Result | Evidence |
| --- | --- | --- |
| Did Graph Truth still pass? | yes | `11/11` fixtures passed in strict update mode. |
| Did Context Packet quality still pass? | yes | `11/11` fixtures passed; critical symbol recall, proof path coverage, source-span coverage, snippet coverage, and expected-test recall were all `100%`. |
| Did proof DB size decrease? | yes | `820.83 MiB` to `709.07 MiB` on copied Autoresearch artifact after migration and `VACUUM`. |
| Did removed data move, become derivable, or get proven unnecessary? | yes | Removed repeated per-fact `file_hash` text moved to canonical `files.content_hash`, referenced by `file_id`. Nothing proof-required was deleted. |

## Implementation

- Bumped SQLite schema to version `10`.
- Changed `files` to the canonical file table:
  - `file_id INTEGER PRIMARY KEY`
  - `path_id INTEGER NOT NULL UNIQUE`
  - `content_hash TEXT NOT NULL`
  - `mtime_unix_ms`
  - `size_bytes`
  - `language_id`
  - `indexed_at_unix_ms`
  - `content_template_id`
  - `metadata_json`
- Removed `file_hash` columns from:
  - `entities`
  - `edges`
  - `structural_relations`
  - `callsites`
  - `callsite_args`
- Added or enforced `file_id` on those fact tables.
- Added migration from legacy file-hash schemas into the normalized schema.
- Updated edge/entity/callsite insertion paths to resolve or create the canonical file row before fact insertion.
- Updated read queries, audit sampling, unresolved-call pagination, and context fallback SQL to join `files.content_hash AS file_hash`.
- Tightened storage accounting so nullable string columns do not zero out payload estimates.

The hash remains in the existing wire string format in `files.content_hash` for compatibility. Converting that one canonical value to `BLOB(32)` is still possible later, but it is now a small per-file optimization instead of a repeated fact-row payload problem.

## Before/After Dbstat

Before artifact:

`reports/audit/artifacts/dictionary_index_compaction_autoresearch_sim.sqlite`

After artifact:

`reports/audit/artifacts/file_hash_norm_storage_experiments/run-1778549197305/vacuum_analyze/codegraph.sqlite`

| Metric | Before | After | Delta |
| --- | ---: | ---: | ---: |
| SQLite DB bytes | 860,700,672 | 743,514,112 | -117,186,560 |
| SQLite DB MiB | 820.83 | 709.07 | -111.76 |
| DB family bytes | 860,700,672 | 743,546,880 | -117,153,792 |
| Average DB bytes per edge | 1006.54 | 869.54 | -137.00 |
| Integrity check | ok | ok | unchanged |

Fact-table dbstat deltas:

| Object | Rows | Before bytes | After bytes | Delta |
| --- | ---: | ---: | ---: | ---: |
| `entities` | 1,630,146 | 119,382,016 | 81,223,680 | -38,158,336 |
| `edges` | 855,108 | 131,022,848 | 113,274,880 | -17,747,968 |
| `structural_relations` | 2,019,607 | 139,460,608 | 98,189,312 | -41,271,296 |
| `callsites` | 524,760 | 41,451,520 | 30,814,208 | -10,637,312 |
| `callsite_args` | 466,956 | 33,017,856 | 23,576,576 | -9,441,280 |
| `files` | 4,975 | 1,466,368 | 1,470,464 | +4,096 |

The `files` table grew by one page because it now owns canonical file IDs and content hashes. That is the intended trade: one per-file hash row instead of millions of repeated fact-row hash strings.

## Top Remaining Contributors After This Change

| Object | Rows | Bytes |
| --- | ---: | ---: |
| `edges` | 855,108 | 113,274,880 |
| `object_id_dict` | 1,631,103 | 99,954,688 |
| `structural_relations` | 2,019,607 | 98,189,312 |
| `entities` | 1,630,146 | 81,223,680 |
| `qname_prefix_dict` | 339,395 | 46,112,768 |
| `symbol_dict` | 789,868 | 35,655,680 |
| `idx_object_id_dict_hash` | n/a | 31,113,216 |
| `callsites` | 524,760 | 30,814,208 |
| `qualified_name_dict` | 1,447,663 | 24,363,008 |
| `callsite_args` | 466,956 | 23,576,576 |

This confirms the next storage work is still the proof/debug split plus dictionary/object-id width, not more repeated file hashes.

## Validation

Commands run:

```text
cargo test
cargo test -p codegraph-store
cargo run -q -p codegraph-cli -- bench graph-truth --cases benchmarks\graph_truth\fixtures --fixture-root . --out-json reports\audit\artifacts\file_hash_normalization_graph_truth.json --out-md reports\audit\artifacts\file_hash_normalization_graph_truth.md --fail-on-forbidden --fail-on-missing-source-span --fail-on-unresolved-exact --fail-on-derived-without-provenance --fail-on-test-mock-production-leak --update-mode
cargo run -q -p codegraph-cli -- bench context-packet --cases benchmarks\graph_truth\fixtures --fixture-root . --out-json reports\audit\artifacts\file_hash_normalization_context_packet.json --out-md reports\audit\artifacts\file_hash_normalization_context_packet.md --top-k 10 --budget 2400
cargo run -q -p codegraph-cli -- bench update-integrity --iterations 5 --skip-autoresearch --out-json reports\audit\artifacts\file_hash_normalization_update_integrity.json --out-md reports\audit\artifacts\file_hash_normalization_update_integrity.md
cargo run -q -p codegraph-cli -- audit storage --db reports\audit\artifacts\dictionary_index_compaction_autoresearch_sim.sqlite --json reports\audit\artifacts\file_hash_normalization_before_storage.json --markdown reports\audit\artifacts\file_hash_normalization_before_storage.md
cargo run -q -p codegraph-cli -- status reports\audit\artifacts\file_hash_norm_migrate_repo
cargo run -q -p codegraph-cli -- audit storage --db reports\audit\artifacts\file_hash_norm_storage_experiments\run-1778549197305\vacuum_analyze\codegraph.sqlite --json reports\audit\artifacts\file_hash_normalization_after_storage.json --markdown reports\audit\artifacts\file_hash_normalization_after_storage.md
```

Results:

- Full Rust suite: passed.
- Store unit tests: passed after the final accounting assertion change.
- Graph Truth Gate: `11/11` passed, `0` matched forbidden edges, `0` matched forbidden paths, `0` source-span failures, `0` stale failures.
- Context Packet Gate: `11/11` passed, critical symbol recall `100%`, proof path coverage `100%`, source-span coverage `100%`, expected test recall `100%`, distractor ratio `0%`.
- Update-integrity harness: passed on fixture runs.
- Storage audit integrity: before `ok`, after `ok`.

## Notes

- The full storage-experiments command was intentionally stopped after the needed copied-DB `vacuum_analyze` artifact was created; the broad experiment suite was outside this task and timed out while running unrelated experiments on the large copy.
- The original Autoresearch artifact was not mutated. All migration and `VACUUM` measurements were performed on copied DBs only.
- This is still not a `<=250 MiB` proof database. It is a meaningful normalization step inside the current debug-scale graph.
