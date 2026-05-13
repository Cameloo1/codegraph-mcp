# PathEvidence Sampler Fix

Generated: 2026-05-12T20:11:12.3834014-05:00

## Verdict

**Status: passed.** The compact proof DB PathEvidence sampler no longer times out. A sample of 20 completed in **26 ms** and a sample of 100 completed in **49 ms** on `reports/final/artifacts/compact_gate_autoresearch_proof.sqlite`.

## Root Cause

The old sampler did too much work before the sample was bounded:

- Candidate selection used a computed `ORDER BY ((length(id) + seed) % N)`, which could not use rowid or primary-key order.
- Stored PathEvidence expansion performed per-edge lookups through `edges_compat`.
- Per-edge/per-entity lookup repeatedly touched broad compatibility views.
- Proof mode could still generate fallback paths from edge samples.

## Fix

- Added `--max-edge-load`, `--timeout-ms`, and `--mode proof|audit|debug` to `codegraph-mcp audit sample-paths`.
- Changed the default PathEvidence sample limit to `20`.
- Selected bounded path IDs first using a deterministic seeded rowid range with one wraparound pass.
- Batch-loaded `path_evidence` rows by sampled IDs.
- Batch-loaded `path_evidence_edges` rows by sampled path IDs, capped by `--max-edge-load`.
- Reused stored PathEvidence metadata for edge exactness, confidence, provenance, context, source spans, and relation sequence.
- Loaded snippets only for sampled spans when `--include-snippets` is set.
- Disabled generated fallback paths in proof mode.
- Added sampler timing, index checks, fallback status, truncation status, and EXPLAIN QUERY PLAN output.

## Validation

| Check | Result |
| --- | --- |
| 20 PathEvidence samples on compact proof DB | pass, 26 ms |
| 100 PathEvidence samples on compact proof DB | pass, 49 ms |
| Generated fallback paths in proof mode | 0 |
| Graph Truth Gate | 11/11 passed |
| Context Packet Gate | 11/11 passed |
| `cargo test --workspace` | passed |

## Artifacts

- `reports/audit/artifacts/path_evidence_sampler_fix_20.json`
- `reports/audit/artifacts/path_evidence_sampler_fix_20.md`
- `reports/audit/artifacts/path_evidence_sampler_fix_100.json`
- `reports/audit/artifacts/path_evidence_sampler_fix_100.md`
- `reports/audit/artifacts/path_sampler_graph_truth.json`
- `reports/audit/artifacts/path_sampler_context_packet.json`

## Storage-Safety Questions

1. Did Graph Truth still pass? **Yes, 11/11.**
2. Did Context Packet quality still pass? **Yes, 11/11.**
3. Did proof DB size decrease? **No. This was not a storage compaction change.**
4. Did removed data move to a sidecar, become derivable, or get proven unnecessary? **No data was removed.**

## Notes

The endpoint batch lookup now avoids the expensive `object_id_lookup` plus `qualified_name_lookup` path for normal compact `repo://e/<hash>` IDs. It resolves display names through the compact entity hash path and leaves `qualified_name` unset in sampler output when reconstructing it would require debug-style joins. Source spans, snippets, exactness labels, provenance, and production/test/mock labels are still present in sampled PathEvidence.
