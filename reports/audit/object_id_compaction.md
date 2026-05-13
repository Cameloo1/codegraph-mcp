# Object ID Compaction

Generated: 2026-05-11 22:01:45 -05:00

## Verdict

Pass for this storage slice.

Compact proof storage no longer stores one `repo://e/<32hex>` string per entity in `object_id_dict`. Entity rows keep dense integer `id_key` values plus `entity_hash BLOB`, and human-readable IDs are reconstructed through `object_id_lookup` / `object_id_debug`.

The copied Autoresearch artifact decreased from `684,494,848` bytes to `581,713,920` bytes after migration plus `VACUUM`, saving `102,780,928` bytes (`98.02 MiB`). This exceeds the required `80 MiB` target and is just below the ideal `100+ MiB` target when measured in binary MiB.

## Four Storage Safety Questions

| Question | Result | Evidence |
| --- | --- | --- |
| Did Graph Truth still pass? | yes | `object_id_compaction_graph_truth.json`: `11/11` passed |
| Did Context Packet quality still pass? | yes | `object_id_compaction_context_packet.json`: `11/11` passed, proof path/source-span coverage `1.0` |
| Did proof DB size decrease? | yes | `684,494,848` to `581,713,920` bytes |
| Did removed data move to a sidecar, become derivable, or get proven unnecessary? | yes | entity IDs are reconstructed from `entities.entity_hash`; temporary update identity reuse uses compact `entity_id_history(entity_hash, id_key)` with no string payload; fallback `object_id_dict` remains only for noncanonical/debug IDs |

## Storage Evidence

Before object ID compaction:

| Object | Rows | Bytes |
| --- | ---: | ---: |
| `object_id_dict` | `1,631,103` | `99,954,688` |
| `idx_object_id_dict_hash` | index | `31,113,216` |
| `entities` | `1,630,146` | `81,223,680` |

After object ID compaction:

| Object | Rows | Bytes |
| --- | ---: | ---: |
| `object_id_dict` | `957` | `61,440` |
| `idx_object_id_dict_hash` | index | `24,576` |
| `entity_id_history` | `0` | `4,096` |
| `entities` | `1,630,146` | `109,420,544` |

The entity table grows because it now stores the compact 16-byte entity hash. The removed string dictionary and index are still a net win of `98.02 MiB`.

Integrity and reconstruction checks on the migrated copy:

- `PRAGMA integrity_check`: `ok`
- canonical `repo://e/%` rows in `object_id_dict`: `0`
- entity rows / distinct hashes: `1,630,146 / 1,630,146`
- `entity_id_history` rows after cold migration: `0`
- sample reconstructed ID: `repo://e/09311d58aba67ff221b49354dc8a59fc`

## Schema And Compatibility

Compact proof entity identity is now:

- `entities.id_key INTEGER PRIMARY KEY`
- `entities.entity_hash BLOB NOT NULL` for canonical `repo://e/<hex>` reconstruction
- existing kind, file, scope, parent, and declaration fields for semantic identity

`object_id_dict` is retained only as a compatibility fallback for noncanonical IDs, such as older test/debug IDs and non-entity debug objects. It no longer stores canonical entity IDs in compact proof mode.

`object_id_lookup` reconstructs:

- current entity IDs from `entities.entity_hash`
- noncanonical/debug IDs from fallback `object_id_dict`
- deleted/update-time unresolved entity references from compact `entity_id_history`

`entity_id_history` exists so delete/restore and incremental update cycles can reuse dense integer IDs without preserving string IDs. It is empty after cold migration and only receives compact `(entity_hash, id_key)` rows for changed/deleted file paths or temporarily referenced canonical entities.

The broad `idx_entities_entity_hash` index was deliberately not kept in proof mode. Keeping it replaced the deleted string bloat with a large BLOB index and failed the storage goal. Default proof workflows use symbol/path/relation/context indexes instead; direct object-ID lookup remains functional through the reconstruction view.

## Validation

Commands run:

```text
cargo test
cargo run -q -p codegraph-cli -- bench graph-truth --cases benchmarks\graph_truth\fixtures --fixture-root . --out-json reports\audit\artifacts\object_id_compaction_graph_truth.json --out-md reports\audit\artifacts\object_id_compaction_graph_truth.md --fail-on-forbidden --fail-on-missing-source-span --fail-on-unresolved-exact --fail-on-derived-without-provenance --fail-on-test-mock-production-leak --update-mode
cargo run -q -p codegraph-cli -- bench context-packet --cases benchmarks\graph_truth\fixtures --fixture-root . --out-json reports\audit\artifacts\object_id_compaction_context_packet.json --out-md reports\audit\artifacts\object_id_compaction_context_packet.md --top-k 10 --budget 2400
cargo run -q -p codegraph-cli -- audit storage --db reports\audit\artifacts\object_id_compaction_repo\.codegraph\codegraph.sqlite --json reports\audit\artifacts\object_id_compaction_after_storage.json --markdown reports\audit\artifacts\object_id_compaction_after_storage.md
```

Results:

- Full Rust tests: passed
- CLI query tests: passed as part of `cargo test`
- MCP output tests: passed as part of `cargo test`
- stable symbol identity tests: passed as part of `cargo test`
- duplicate file identity tests: passed as part of `cargo test`
- update-integrity harness: passed as part of `cargo test`
- Graph Truth Gate: `11/11` passed
- Context Packet Gate: `11/11` passed
- storage audit: passed
- copied Autoresearch DB integrity: `ok`

## Remaining Storage Gap

The copied artifact is now `581,713,920` bytes (`554.77 MiB`). The proof target is still `<=250 MiB`, so object ID compaction is necessary but not sufficient.

Largest remaining contributors after this slice:

| Object | Bytes |
| --- | ---: |
| `entities` | `109,420,544` |
| `structural_relations` | `98,189,312` |
| `edges` | `54,263,808` |
| `qname_prefix_dict` | `46,112,768` |
| `symbol_dict` | `35,655,680` |
| `callsites` | `30,814,208` |
| `qualified_name_dict` | `24,363,008` |
| `callsite_args` | `23,576,576` |

The next safe storage cuts should continue the same four-question rule and focus on proof-mode separation of structural/debug records, remaining dictionary width, and entity row width without weakening source spans, PathEvidence, or graph truth.
