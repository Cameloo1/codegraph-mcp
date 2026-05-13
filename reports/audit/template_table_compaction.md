# Template Table Compaction

Generated: 2026-05-12 13:05:11 -05:00

Source of truth: `MVP.md`.

## Verdict

Pass for this storage slice.

- Graph Truth Gate: `11/11 passed`
- Context Packet Gate: `11/11 passed`
- DB integrity on copied Autoresearch proof artifact: `ok`
- Proof DB copied-artifact size after schema 16 migration plus VACUUM: `235,778,048 bytes` / `224.855 MiB`
- Previous proof DB size: `336,203,776 bytes` / `320.629 MiB`
- Measured copied-artifact reduction: `100,425,728 bytes` / `95.773 MiB`
- Target contribution from template tables/indexes: save at least `25 MiB`
- Template table/index measured reduction: `93,155,328 bytes` / `88.840 MiB`

The original frozen compact-gate artifact was not mutated. The measurement was performed on `reports/audit/artifacts/template_compaction_probe.sqlite`, copied from `reports/final/artifacts/compact_gate_autoresearch_proof.sqlite`.

## What Changed

1. `template_entities.local_template_entity_id` changed from wide synthetic TEXT IDs to deterministic per-template INTEGER local IDs.
2. `template_edges.local_template_edge_id`, `local_head_entity_id`, and `local_tail_entity_id` changed from wide synthetic TEXT IDs to deterministic per-template INTEGER local IDs.
3. `idx_template_edges_head_relation` and `idx_template_edges_tail_relation` are no longer created in proof mode.
4. Schema version advanced to `16`.
5. Existing DB migration rebuilds template tables from old text IDs into compact integer IDs and drops the two directional template-edge indexes.

## Safety Rule

| Question | Answer |
| --- | --- |
| Did graph truth still pass? | Yes. Strict update-mode Graph Truth Gate passed `11/11`. |
| Did context packet quality still pass? | Yes. Context Packet Gate passed `11/11`, with critical symbol recall, proof path coverage, and source-span coverage at `1.0`. |
| Did proof DB size decrease? | Yes. Copied Autoresearch proof DB decreased from `320.629 MiB` to `224.855 MiB` after migration plus VACUUM. |
| Did removed data move to a sidecar, become derivable, or get proven unnecessary? | The removed wide local ID strings became derivable from `content_template_id + local integer ID + stored qname/span/kind`; they were not proof facts. The removed directional indexes were proven unnecessary for current default query paths because synthesized template lookups materialize by template and filter, and the storage audit had no mapped default workflow for those indexes. |

## Column Audit

### `template_entities`

| Column | Disposition |
| --- | --- |
| `content_template_id` | Proof overlay key; kept as INTEGER. |
| `local_template_entity_id` | Needed only as local overlay key; compacted from TEXT to INTEGER. |
| `kind_id` | Proof/display enum dictionary ID; kept as INTEGER. |
| `name_id` | Human-readable symbol lookup; kept as INTEGER dictionary ID. |
| `qualified_name_id` | Needed for overlay identity reconstruction; kept as INTEGER dictionary ID. |
| `start_line`, `start_column`, `end_line`, `end_column` | Needed for source-span reconstruction; kept as compact local span. |
| `content_hash` | Optional and mostly null; left unchanged. |
| `created_from_id` | Needed for extractor label and stable overlay signature; kept as INTEGER dictionary ID. |
| `confidence` | Needed for proof/context metadata; left as REAL for now. |
| `metadata_json` | Still a remaining bloat target; not moved in this phase to avoid mixing changes. |

### `template_edges`

| Column | Disposition |
| --- | --- |
| `content_template_id` | Proof overlay key; kept as INTEGER. |
| `local_template_edge_id` | Needed only as local overlay key; compacted from TEXT to INTEGER. |
| `local_head_entity_id`, `local_tail_entity_id` | Needed for local endpoint mapping; compacted from TEXT to INTEGER. |
| `relation_id` | Already compact dictionary enum ID; kept. |
| `start_line`, `start_column`, `end_line`, `end_column` | Needed for proof source spans; kept as compact local span. |
| `extractor_id`, `exactness_id`, `resolution_kind_id`, `edge_class_id`, `context_id`, `context_kind_id`, `flags_bitset`, `confidence_q`, `provenance_id` | Already compact metadata columns; kept. |
| `repo_commit`, `confidence`, `derived`, `provenance_edges_json`, `metadata_json` | Kept. JSON fields remain future candidates but were not changed here. |

## Measured Storage Impact

| Object | Before bytes | After bytes | Saved MiB |
| --- | ---: | ---: | ---: |
| `template_entities` | `87,523,328` | `52,879,360` | `33.039` |
| `template_edges` | `40,804,352` | `15,372,288` | `24.254` |
| `idx_template_edges_head_relation` | `16,539,648` | `0` | `15.773` |
| `idx_template_edges_tail_relation` | `16,539,648` | `0` | `15.773` |
| Template total | `161,406,976` | `68,251,648` | `88.840` |
| Whole copied proof DB | `336,203,776` | `235,778,048` | `95.773` |

## Validation

| Check | Result |
| --- | --- |
| `cargo test --workspace` | passed |
| Targeted template overlay test | passed |
| Migration smoke test | passed |
| Graph Truth Gate | passed, `11/11` |
| Context Packet Gate | passed, `11/11` |
| Update integrity fixture harness | passed |
| Copied Autoresearch DB `PRAGMA integrity_check` | `ok` |
| Context-pack smoke on copied DB | `672.743 ms` |
| Comprehensive benchmark command | ran; still reports the frozen persisted compact-gate snapshot unless a fresh compact gate JSON is supplied |

## Remaining Storage Risks

- `template_entities.metadata_json` remains about `25 MiB` raw in the previous drilldown and is the next direct template-table target.
- `symbol_dict`, `qname_prefix_dict`, and `qualified_name_dict` remain large because template-local display names still share the proof dictionary.
- PathEvidence JSON normalization remains a separate safe package and was not mixed into this change.
- The final comprehensive benchmark latest report still uses the persisted compact-gate artifact and therefore does not by itself prove this migrated copied-artifact size.
