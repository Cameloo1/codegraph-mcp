# Content Template Dedupe

Generated: 2026-05-12 03:56:17 -05:00

## Verdict

Status: **passed correctness gates, failed storage-savings target, failed cold-index performance**

Duplicate source-content analysis is now shared through content templates while preserving separate file/path identity through path-specific overlay IDs. The implementation confirms the large duplicate-tree pattern on Autoresearch: `4975` source files were read/hashed, but only `2555` were parsed; `2420` same-content files were skipped as duplicate local analysis.

The final clean Autoresearch proof DB is `336,203,776` bytes / `320.63 MiB`. This is a real decrease from the previous compact-proof measurement of `376,135,680` bytes, saving `39,931,904` bytes / `38.08 MiB`, but it misses the requested `150 MiB` target and remains `70.63 MiB` over the `250 MiB` intended proof DB target.

## Four-Question Storage Rule

| Question | Result |
| --- | --- |
| Did graph truth still pass? | Yes. Graph Truth Gate passed `11/11`. |
| Did context packet quality still pass? | Yes. Context Packet Gate passed `11/11` with `100%` critical symbol recall, proof-path coverage, source-span coverage, and expected-test recall. |
| Did proof DB size decrease? | Yes, but not enough. Autoresearch proof DB decreased from `376,135,680` to `336,203,776` bytes. |
| Did removed data move to a sidecar, become derivable, or get proven unnecessary? | Partly. Duplicate local proof entities/edges moved into shared `template_entities` / `template_edges`; duplicate file identity is reconstructed from `file_instance + local_template_entity_id`. High-cardinality structural/callsite template edges were excluded from proof template storage because they are not proof edges and are not used by Graph Truth or normal context packets. Full compatibility synthesis for duplicate structural relation queries remains a follow-up risk. |

## Implementation Summary

- Added content template overlay storage:
  - `source_content_template`
  - `template_entities`
  - `template_edges`
  - `file_instance` view
- Added content-template detection before batching so canonical files can be marked when later duplicate content exists.
- Changed duplicate file handling so duplicate files upsert only file identity and reuse the canonical template extraction.
- Preserved path-specific external identities by synthesizing duplicate entities/edges with the target file path and file hash.
- Kept import and cross-file call resolution path-specific; duplicate trees now produce distinct external target IDs.
- Avoided reintroducing structural bloat by excluding `CONTAINS`, `DEFINED_IN`, `DECLARES`, `CALLEE`, `ARGUMENT_0`, `ARGUMENT_1`, `ARGUMENT_N`, and `RETURNS_TO` from compact proof `template_edges`.
- Added physical-row existence helpers for writer paths so overlay-aware query synthesis does not leak into reducer write decisions.
- Removed broad compact-fact cleanup scans from the post-file-delete bulk insert path; primary keys still handle duplicate insertion safely.

## Tests Added

- `duplicate_content_uses_template_overlay_with_path_specific_imports`
  - duplicate same-content files preserve distinct external IDs
  - duplicate files share template extraction
  - imports resolve to the path-specific duplicate target
  - cross-target contamination is rejected

Existing duplicate file identity tests also continue to pass.

## Gate Results

| Gate | Result |
| --- | --- |
| Graph Truth Gate | Passed, `11/11` fixtures |
| Context Packet Gate | Passed, `11/11` fixtures |
| Full Rust test suite | Passed |
| Autoresearch DB integrity | Passed, storage audit integrity check `ok` |

## Autoresearch Measurement

| Metric | Value |
| --- | ---: |
| Source candidates | `4975` |
| Files read | `4975` |
| Files hashed | `4975` |
| Files parsed | `2555` |
| Duplicate local analyses skipped | `2420` |
| Files indexed | `4975` |
| Source content templates | `2718` |
| Physical entities | `89,407` |
| Template entities | `653,819` |
| Physical proof edges | `31,848` |
| Template proof edges | `169,290` |
| Relation-compatible facts | `170,114` |
| Stored PathEvidence rows | `4096` |
| DB family size | `336,203,776` bytes / `320.63 MiB` |
| Previous compact-proof size | `376,135,680` bytes / `358.71 MiB` |
| Savings vs previous compact-proof size | `39,931,904` bytes / `38.08 MiB` |
| Gap to 250 MiB target | `74,059,776` bytes / `70.63 MiB` |
| Cold index wall time | `2,894,484 ms` / `48.24 min` |

## Top Storage Contributors

| Object | Rows | Bytes |
| --- | ---: | ---: |
| `template_entities` | `653,819` | `87,523,328` |
| `template_edges` | `169,290` | `40,804,352` |
| `symbol_dict` | `696,373` | `30,392,320` |
| `qname_prefix_dict` | `155,741` | `22,274,048` |
| `idx_template_edges_tail_relation` | unknown | `16,539,648` |
| `idx_template_edges_head_relation` | unknown | `16,539,648` |
| `idx_symbol_dict_hash` | unknown | `14,602,240` |
| `idx_qualified_name_parts` | unknown | `12,742,656` |
| `qualified_name_dict` | `727,372` | `12,173,312` |
| `file_entities` | `89,407` | `10,248,192` |

## What Improved

- Duplicate-content local parsing/extraction is skipped for `2420` files.
- Separate file identity is preserved; same-content different-path files synthesize distinct entity IDs.
- Path-specific imports into duplicate trees resolve to the correct target.
- The proof DB decreased by `38.08 MiB`.
- A bad intermediate design that stored structural/callsite template edges produced an `842,547,200` byte DB; excluding those non-proof template edges reduced the clean DB to `336,203,776` bytes.

## What Did Not Meet The Goal

- The requested `150 MiB` savings target was not met.
- The proof DB is still above `250 MiB`.
- Cold index time is unacceptable at `48.24` minutes.
- Canonical template writes are still expensive:
  - `entity_insert`: `786,271.97 ms`
  - `edge_insert`: `1,660,651.07 ms`
  - `content_template_upsert`: `54,697.80 ms`
  - `dictionary_lookup_insert`: `78,441.22 ms`
- The largest remaining table is now `template_entities`; template identity still stores too much qname/symbol payload.

## Safety Notes

Template overlay is safe for the proof/context gates exercised here, but not a final storage architecture. The next safe step is not to add more compression blindly; it is to make template entities narrower:

1. Store local template entity names as local symbol IDs plus compact local qname suffixes.
2. Reconstruct template qualified names from `file_instance.path_id + local scope/name`, not global qname prefixes.
3. Add compact parent/scope/local structural flags to `template_entities` so structural relation compatibility can be synthesized without storing structural edges.
4. Re-evaluate whether `idx_template_edges_head_relation` and `idx_template_edges_tail_relation` are justified for proof-mode queries.

## Artifacts

- `reports/audit/artifacts/content_template_dedupe_graph_truth.json`
- `reports/audit/artifacts/content_template_dedupe_graph_truth.md`
- `reports/audit/artifacts/content_template_dedupe_context_packet.json`
- `reports/audit/artifacts/content_template_dedupe_context_packet.md`
- `reports/audit/artifacts/content_template_dedupe_storage.json`
- `reports/audit/artifacts/content_template_dedupe_storage.md`
- `reports/audit/artifacts/content_template_dedupe_relation_counts.json`
- `reports/audit/artifacts/content_template_dedupe_relation_counts.md`
- `reports/audit/artifacts/content_template_dedupe_autoresearch_v7.sqlite`

