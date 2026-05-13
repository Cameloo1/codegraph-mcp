# Metadata JSON Compaction

Generated: 2026-05-11 21:11:50 -05:00

## Verdict

Pass for this storage slice.

`edges.metadata_json` has been removed from compact proof edge rows and replaced with compact dictionary/bitset columns plus a compatibility view for existing readers. Optional verbose metadata now belongs in `edge_debug_metadata(edge_id, metadata_json)`, which is empty in the compact proof artifact.

The copied Autoresearch artifact decreased from `743,514,112` bytes to `684,494,848` bytes after migration plus `VACUUM`, saving `59,019,264` bytes (`56.29 MiB`). This exceeds the required `40 MiB` target, but does not reach the ideal `60+ MiB`.

## Four Storage Safety Questions

| Question | Result | Evidence |
| --- | --- | --- |
| Did Graph Truth still pass? | yes | `metadata_json_compaction_graph_truth.json`: `11/11` passed |
| Did Context Packet quality still pass? | yes | `metadata_json_compaction_context_packet.json`: `11/11` passed, proof path/source-span coverage `1.0` |
| Did proof DB size decrease? | yes | `743,514,112` to `684,494,848` bytes |
| Did removed data move to a sidecar, become derivable, or get proven unnecessary? | yes | verbose JSON moved to optional `edge_debug_metadata`; common labels are stored in compact columns and reconstructed through `edges_compat` |

## Metadata Audit

The pre-change copied Autoresearch artifact had `855,108` proof-adjacent edge rows and about `62,614,078` raw bytes of `edges.metadata_json`.

Top repeated metadata keys:

| Key | Rows | Raw byte pressure |
| --- | ---: | ---: |
| `language_frontend` | 820,722 | about `23.01 MiB` decimal bytes before SQLite overhead |
| `resolution` | 476,530 | about `20.96 MiB` decimal bytes before SQLite overhead |
| `phase` | 824,562 | about `11.54 MiB` decimal bytes before SQLite overhead |
| `heuristic` | 478,456 | about `8.61 MiB` decimal bytes before SQLite overhead |

Top relation contributors:

| Relation | Rows | Metadata bytes |
| --- | ---: | ---: |
| `CALLS` | 516,392 | `49,619,692` |
| `FLOWS_TO` | 163,481 | `6,297,954` |
| `DEFINES` | 72,897 | `2,932,550` |
| `IMPORTS` | 72,626 | `2,927,217` |

## Schema Changes

Compact proof `edges` now stores:

- `exactness_id`
- `resolution_kind_id`
- `context_kind_id`
- `flags_bitset`
- `confidence_q`
- `extractor_id`
- `provenance_id`
- existing source span/file/provenance identity columns

New dictionary/sidecar tables:

- `resolution_kind_dict(id, value)`
- `edge_provenance_dict(id, value)`
- `edge_debug_metadata(edge_id, metadata_json)`

`edges.metadata_json` is absent from the compact proof table. The migrated artifact has:

- `edge_debug_metadata` rows: `0`
- `resolution_kind_dict` rows: `7`
- `edge_provenance_dict` rows: `51`
- `edges_compat` view: present

## Compatibility Behavior

Existing readers, graph truth, context packets, unresolved-call queries, and audit samplers now read edge metadata through `edges_compat`.

When debug sidecar metadata exists, `edges_compat` returns it. Otherwise it reconstructs compact JSON from proof columns for labels such as:

- heuristic/unresolved flags
- dynamic import marker
- static import resolution
- resolved sanitizer/import call labels
- derived fact class labels

Graph fact hashes use compact proof columns and intentionally exclude sidecar-only debug JSON.

## Storage Before/After

| Metric | Before file-hash normalized artifact | After metadata compaction | Delta |
| --- | ---: | ---: | ---: |
| DB bytes | `743,514,112` | `684,494,848` | `-59,019,264` |
| DB MiB | `709.07` | `652.78` | `-56.29` |
| `edges` table bytes | `113,274,880` | `54,263,808` | `-59,011,072` |
| `edge_debug_metadata` bytes | n/a | `4,096` | sidecar empty |
| Edge row count | `855,108` | `855,108` | unchanged |

Top remaining storage contributors after this slice:

| Object | Rows | Bytes | Percent |
| --- | ---: | ---: | ---: |
| `object_id_dict` | `1,631,103` | `99,954,688` | `14.60%` |
| `structural_relations` | `2,019,607` | `98,189,312` | `14.34%` |
| `entities` | `1,630,146` | `81,223,680` | `11.87%` |
| `edges` | `855,108` | `54,263,808` | `7.93%` |
| `qname_prefix_dict` | `339,395` | `46,112,768` | `6.74%` |

## Gates And Commands

Graph Truth:

```powershell
cargo run -q -p codegraph-cli -- bench graph-truth --cases benchmarks\graph_truth\fixtures --fixture-root . --out-json reports\audit\artifacts\metadata_json_compaction_graph_truth.json --out-md reports\audit\artifacts\metadata_json_compaction_graph_truth.md --fail-on-forbidden --fail-on-missing-source-span --fail-on-unresolved-exact --fail-on-derived-without-provenance --fail-on-test-mock-production-leak --update-mode
```

Result: `11/11` passed.

Context Packet:

```powershell
cargo run -q -p codegraph-cli -- bench context-packet --cases benchmarks\graph_truth\fixtures --fixture-root . --out-json reports\audit\artifacts\metadata_json_compaction_context_packet.json --out-md reports\audit\artifacts\metadata_json_compaction_context_packet.md --top-k 10 --budget 2400
```

Result: `11/11` passed, critical symbol recall `1.0`, proof path coverage `1.0`, source-span coverage `1.0`, distractor ratio `0.0`.

Storage audit:

```powershell
cargo run -q -p codegraph-cli -- audit storage --db reports\audit\artifacts\metadata_compaction_repo\.codegraph\codegraph.sqlite --json reports\audit\artifacts\metadata_json_compaction_after_storage.json --markdown reports\audit\artifacts\metadata_json_compaction_after_storage.md
```

Result: integrity `ok`, file family bytes `684,527,616`.

Relation/source-span sampler:

```powershell
cargo run -q -p codegraph-cli -- audit sample-edges --db reports\audit\artifacts\metadata_compaction_repo\.codegraph\codegraph.sqlite --relation CALLS --limit 20 --seed 11 --json reports\audit\artifacts\metadata_json_compaction_sample_CALLS.json --markdown reports\audit\artifacts\metadata_json_compaction_sample_CALLS.md --include-snippets
cargo run -q -p codegraph-cli -- audit sample-edges --db reports\audit\artifacts\metadata_compaction_repo\.codegraph\codegraph.sqlite --relation READS --limit 20 --seed 11 --json reports\audit\artifacts\metadata_json_compaction_sample_READS.json --markdown reports\audit\artifacts\metadata_json_compaction_sample_READS.md --include-snippets
cargo run -q -p codegraph-cli -- audit sample-edges --db reports\audit\artifacts\metadata_compaction_repo\.codegraph\codegraph.sqlite --relation WRITES --limit 20 --seed 11 --json reports\audit\artifacts\metadata_json_compaction_sample_WRITES.json --markdown reports\audit\artifacts\metadata_json_compaction_sample_WRITES.md --include-snippets
cargo run -q -p codegraph-cli -- audit sample-edges --db reports\audit\artifacts\metadata_compaction_repo\.codegraph\codegraph.sqlite --relation FLOWS_TO --limit 20 --seed 11 --json reports\audit\artifacts\metadata_json_compaction_sample_FLOWS_TO.json --markdown reports\audit\artifacts\metadata_json_compaction_sample_FLOWS_TO.md --include-snippets
```

Result: each sampler returned `20` records with the compact metadata path.

Full tests:

```powershell
cargo test
```

Result: passed.

## Residual Notes

This change removed verbose edge metadata from compact proof storage without deleting proof facts, source spans, or provenance. It did not solve the larger proof DB target by itself: the artifact is still `684.49 MiB`, so the remaining path to `<=250 MiB` must keep attacking row volume and remaining dictionary/structural contributors.

One unrelated observation remains: large-DB `sample-paths` timed out during this slice when attempted against the copied Autoresearch artifact. That is a sampler/path audit performance issue, not evidence that compact proof metadata regressed Graph Truth or Context Packet behavior.
