# Heuristic Debug Sidecar

Generated: 2026-05-11 22:50:06 -05:00

## Verdict

Pass.

Compact proof storage no longer stores heuristic, inferred, unresolved, static-reference facts as first-class proof entities or proof edges by default. Audit/debug indexing preserves those diagnostics in explicit sidecar tables:

- `heuristic_edges`
- `unresolved_references`
- `static_references`
- `extraction_warnings`

Normal context packets continue to use proof-grade exact facts and stored PathEvidence only. Debug context can attach heuristic candidates from `heuristic_edges`, and unresolved-call audit output reads from the sidecar with explicit `static_heuristic` labels.

## Four Storage Safety Questions

| Question | Answer |
| --- | --- |
| Did Graph Truth still pass? | Yes. `11/11` fixtures passed. |
| Did Context Packet quality still pass? | Yes. `11/11` fixtures passed; proof-path, source-span, critical-symbol, and expected-test coverage were all `100%`. |
| Did proof DB size decrease? | Yes after compact rebuild/VACUUM measurement. Latest prior copied Autoresearch artifact was `581,713,920` bytes. Migrated proof rows plus VACUUM measured `432,730,112` bytes, saving `148,983,808` bytes (`142.08 MiB`). |
| Did removed data move to a sidecar, become derivable, or get proven unnecessary? | Yes. New audit/debug indexing writes heuristic/static/unresolved facts to sidecar tables. For legacy proof DB migration, old heuristic rows are removed from proof tables; rebuilding in audit/debug mode preserves them in sidecars. |

## Implementation

- Added explicit storage modes: `proof`, `audit`, and `debug`.
- Default proof storage policy is now `proof:compact-proof-graph`.
- Proof-mode entity persistence excludes:
  - `static_reference:*`
  - `dynamic_import:*`
  - entities marked unresolved or heuristic
- Proof-mode edge persistence excludes:
  - `static_heuristic`
  - `inferred`
  - `base_heuristic`
  - edges with static-reference, dynamic-import, unresolved, or unresolved metadata endpoints
- Audit/debug mode routes excluded diagnostic facts into sidecars instead of proof tables.
- Graph Truth and Context Packet fixture gates index fixtures in `audit` mode so unsupported/heuristic assertions remain observable without becoming proof facts.
- `query unresolved-calls` now reads `heuristic_edges` plus `static_references`, and returns no rows instead of failing when opened against older proof-only DBs without sidecars.
- Context packet normal/impact modes reject static heuristic and inferred PathEvidence. Debug mode can include heuristic sidecar candidates as non-proof.

## Storage Evidence

Copied Autoresearch artifact:

`reports/audit/artifacts/object_id_compaction_repo/.codegraph/codegraph.sqlite`

Migration target:

`reports/audit/artifacts/heuristic_debug_sidecar_repo/.codegraph/codegraph.sqlite`

| Metric | Before | After proof migration | Delta |
| --- | ---: | ---: | ---: |
| `entities` rows | 1,630,146 | 1,388,664 | -241,482 |
| `edges` rows | 855,108 | 370,626 | -484,482 |
| `structural_relations` rows | 2,019,607 | 1,304,731 | -714,876 |
| `callsites` rows | 524,760 | 38,367 | -486,393 |
| `heuristic_or_unknown_edges` | unknown before split | 0 in proof audit summary | exact-only proof edge table |
| prior compact artifact size | 581,713,920 bytes | 432,730,112 bytes after VACUUM experiment | -148,983,808 bytes |

The direct migrated copy had `92,262,400` bytes of SQLite freelist pages before compaction. The copied-DB `vacuum_analyze` experiment reduced the file family from `606,109,696` bytes to `432,730,112` bytes with `integrity_check: ok`.

## Gate Results

| Gate | Result | Artifact |
| --- | --- | --- |
| Graph Truth | passed, `11/11` | `reports/audit/artifacts/heuristic_debug_sidecar_graph_truth.json` |
| Context Packet | passed, `11/11` | `reports/audit/artifacts/heuristic_debug_sidecar_context_packet.json` |
| Relation counts | `12` relations, `47` proof edges on audit fixture sample | `reports/audit/artifacts/heuristic_debug_sidecar_relation_counts.json` |
| Relation sampler | `READS` sample count `2`, snippets loaded | `reports/audit/artifacts/heuristic_debug_sidecar_sample_READS.json` |
| Storage audit | integrity `ok`, proof `heuristic_or_unknown_edges: 0` | `reports/audit/artifacts/heuristic_debug_sidecar_storage.json` |
| Storage experiment | VACUUM/ANALYZE copy size `432,730,112` bytes | `reports/audit/artifacts/heuristic_debug_sidecar_storage_experiments.json` |
| Full Rust tests | passed | `cargo test` |

The dynamic import fixture still exposes unresolved/static heuristic CALLS through audit/debug sidecar evidence. A proof index does not store the dynamic import/static-reference entities or static heuristic import/call edges as proof facts.

## Remaining Risks

- Audit/debug sidecars are implemented as tables in the same SQLite database. The physical proof/audit/debug file split from `compact_proof_vs_debug_storage_design.md` is still future work.
- Legacy proof DB migrations discard old heuristic rows from proof storage; diagnostic preservation requires rebuilding in `audit` or `debug` mode.
- `unresolved_references` and `extraction_warnings` are ready sidecars, but only current extractor paths that emit those diagnostics will populate them.
- The proof DB is still above the `<=250 MiB` intended Autoresearch target: `432,730,112` bytes after compaction measurement.

## Next Work

1. Move audit/debug sidecar tables into physically separate sidecar DB files so proof DB size is not diluted by optional diagnostic storage.
2. Compact remaining high-cardinality proof tables, especially `entities`, `structural_relations`, `qname_prefix_dict`, `symbol_dict`, and `callsite_args`.
3. Update storage audit core unresolved-call query-plan reporting to use `heuristic_edges` instead of the legacy proof-edge compatibility query.
