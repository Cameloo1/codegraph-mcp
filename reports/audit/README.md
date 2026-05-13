# CodeGraph Audit Reports

`MVP.md` is the source of truth for this audit scaffold. The audit checks are anchored to the MVP directives that the graph is truth, vectors only suggest, exact graph/source verification proves, and every edge must preserve source span, extractor, confidence, exactness, derived/provenance metadata, and local repo identity.

This directory is report-only. It is for inspection evidence, findings, schemas, and future audit artifacts. It must not change production logic, benchmark scores, graph extraction, storage layout, or retrieval behavior.

## Audit Phases

| Phase | Report | Verifies | Pass means | Fail means |
| --- | --- | --- | --- | --- |
| 00 scaffold | `reports/audit/README.md`, `AUDIT_STATUS.md`, `audit_status.json` | The audit workspace exists and future phases have a stable place to write findings and artifacts. | Required directories and machine-readable status exist. | Future audit phases have no stable artifact contract. |
| 01 benchmark validity | `reports/audit/01_benchmark_validity.md` | Whether benchmark numbers prove graph correctness instead of command execution, output shape, or implementation-shaped expectations. | Ground truth is independent, negative cases exist, scoring distinguishes file/symbol/relation/path/source-span correctness, and CGC is compared fairly. | Current pass numbers are not enough to trust graph correctness claims. |
| 02 graph schema edge taxonomy | `reports/audit/02_graph_schema_edge_taxonomy.md` | Whether base, heuristic, callsite, derived/cache, and test/mock facts are separable and proof-grade. | Edges carry enough enforced metadata to distinguish exact facts from heuristics, derived closures, test/mock facts, and production proof paths. | A large edge count may mix proof with noise, making context packets hard to trust. |
| 03 storage forensics | `reports/audit/03_storage_forensics.md` | Why the SQLite DB family is large and whether storage layout conflicts with compact-memory goals. | Table/index sizes, dictionary bloat, index query paths, and measurement-only optimization candidates are visible. | Storage changes would be guesswork or could remove proof-critical facts/indexes. |
| edge sampler | `codegraph-mcp audit sample-edges` | Whether humans can inspect random edges with endpoints, span, exactness, provenance, and optional snippets. | Reproducible samples can be manually classified without faking labels. | Aggregate metrics remain unauditable. |
| 04 relation counts and samples | `reports/audit/04_relation_counts_and_samples.md` | Which relations dominate edge volume and which high-risk relations need manual sampling. | Counts, missing-span rows, duplicates, exactness breakdowns, and sample artifacts are visible. | Noisy relations and edge explosion sources stay hidden. |
| 05 stable symbol identity | `reports/audit/05_stable_symbol_identity.md` | Whether entity IDs are stable, collision-resistant, and update-safe across edits, duplicates, aliases, renames, deletes, and tests/mocks. | IDs include enough semantic context and update mappings to preserve proof identity without collisions. | Graph facts may be deterministic but still unstable under realistic repo evolution. |
| 06 manifest hashing stale updates | `reports/audit/06_manifest_hashing_stale_updates.md` | Whether cold, repeat, watch/update, duplicate, rename, and delete indexing flows keep graph state fresh without unnecessary work. | Repeat indexing uses cheap manifest diffing, stale facts are removed, renames are mapped, and dependency closure is handled. | Repeat indexing may stay slow or stale graph facts may survive common file operations. |
| 07 source span proof gate | `reports/audit/07_source_span_proof_gate.md` | Whether proof-sensitive paths have exact source-span provenance before being treated as proof in context packets. | Missing, wrong-file, out-of-range, unavailable, and syntax-mismatched spans cannot be labeled proof-grade. | PathEvidence may look verified while only proving graph connectivity, not source evidence. |

## Artifact Locations

- Human audit reports live in `reports/audit/`.
- Machine-readable audit status lives in `reports/audit/audit_status.json`.
- Phase scratch outputs, copied benchmark summaries, schema drafts, and command logs should go under `reports/audit/artifacts/`.
- Future manually labeled graph-truth fixtures should go under `benchmarks/graph_truth/fixtures/`.
- Future graph-truth JSON schemas should go under `benchmarks/graph_truth/schemas/`.
- Future standalone audit helpers, if needed, should go under `tools/audit/`.

## Audit Commands

Storage forensics:

```powershell
codegraph-mcp audit storage --db <path> --json reports/audit/artifacts/storage.json --markdown reports/audit/artifacts/storage.md
```

Relation counts:

```powershell
codegraph-mcp audit relation-counts --db <path> --json reports/audit/artifacts/relation-counts.json --markdown reports/audit/artifacts/relation-counts.md
```

Manual edge sampling:

```powershell
codegraph-mcp audit sample-edges --db <path> --relation CALLS --limit 50 --seed 20260510 --json reports/audit/artifacts/edge_samples/CALLS.json --markdown reports/audit/artifacts/edge_samples/CALLS.md --include-snippets
```

The sampler leaves `classification:` blank in Markdown. Humans must fill it with one of the allowed labels; the tool must not fake correctness.

## Interpreting Status

- `completed`: inspection/reporting for the phase is written and all required files for that phase exist.
- `partial`: the report exists but some requested evidence could not be inspected or verification did not complete.
- `blocked`: the phase could not complete because a required file, command, or artifact was unavailable.
- `not_started`: reserved for later phases that have not been run.

Pass/fail in these reports is about evidence quality, not whether the Rust test suite passes. Unit tests can prove local code paths compile and expected examples still work, but they cannot prove that benchmark labels are independent, that false positives are rejected, that CGC was compared through an equivalent interface, or that a production context packet is using only proof-grade graph paths. The MVP requires verified graph/source evidence, so this audit treats tests as necessary but not sufficient.
