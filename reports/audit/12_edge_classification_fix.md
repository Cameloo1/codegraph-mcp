# 12 Edge Classification Fix

Source of truth: `MVP.md`.

## Verdict

**Partially complete with blocker.** Base exact, base heuristic, reified callsite, derived/cache, inverse, and test/mock edge classes are now separable in query/reporting logic, and storage now preserves/enforces provenance metadata that already existed in the schema.

The relevant graph-truth fixture `derived_edge_requires_provenance` still fails because the current extractor does not emit the required base `CALLS`, base `WRITES`, `users` entity shape, or persisted/observed derived `MAY_MUTATE` edge for that fixture. This phase did not implement broad semantic extraction fixes.

## MVP Contract Applied

`MVP.md` says base triples are atomic truth, derived closure edges are cached shortcuts, and every derived edge must carry provenance base edges. It also says exact graph/path verification must check edge existence, source spans, type/domain-codomain, and derived provenance.

This phase enforces that contract at the classification/proof boundary:

- exact base edges can remain proof-grade when source spans validate;
- heuristic or unresolved edges are labeled non-proof-grade by default;
- derived/cache edges require provenance edge IDs;
- derived/cache edges are counted separately from raw base facts;
- inverse edges are labeled separately and are not counted as raw base proof facts;
- path evidence exposes per-edge fact classes and derived provenance metadata.

## Code Changes

| Surface | Files / functions |
| --- | --- |
| Edge fact classification | `crates/codegraph-query/src/lib.rs`: `EdgeFactClass`, `classify_edge_fact`, `validate_proof_path_edge_classes` |
| Proof-path metadata | `crates/codegraph-query/src/lib.rs`: `ExactGraphQueryEngine::path_evidence`, `path_evidence_with_source_validation` |
| Derived provenance enforcement | `crates/codegraph-store/src/sqlite.rs`: `normalize_edge_for_storage`, `validate_edge_derivation`, `validate_derived_closure_edge` |
| Lossless compact insert | `crates/codegraph-store/src/sqlite.rs`: `insert_edge_after_file_delete` now writes `provenance_edges_json` and `metadata_json` |
| Unresolved downgrade guard | `crates/codegraph-store/src/sqlite.rs`: unresolved exact edges are stored as `static_heuristic` |
| Graph Truth output | `crates/codegraph-bench/src/graph_truth.rs`: per-case and total `edge_class_counts`, base edge counts, derived/cache counts |

## Schema Changes

No SQLite schema migration was required. The existing compact schema already had:

- `exactness_id`
- `derived`
- `provenance_edges_json`
- `extractor_id`
- `confidence`
- `metadata_json`

The fix makes existing columns meaningful by preserving them on the normal compact insert path and rejecting derived/cache facts without provenance.

## Storage Behavior

| Case | Behavior |
| --- | --- |
| Exact base edge | Stored as before. It can be proof-grade only after source-span validation. |
| Heuristic edge | Stored with heuristic exactness; path evidence marks it non-proof-grade by default. |
| Unresolved textual match labeled exact | Downgraded to `static_heuristic` before storage and annotated with `exactness_downgraded_from`. |
| Derived/cache edge | Must set `derived=true` and include non-empty `provenance_edges`. |
| Derived closure row | `upsert_derived_edge` rejects empty provenance. |
| Inverse edge | Classified as `inverse`, so reports do not count it as a raw base fact. |

## Proof-Path Behavior

Path evidence now includes per-edge labels:

- `edge_id`
- `relation`
- `exactness`
- `confidence`
- `extractor`
- `derived`
- `provenance_edges`
- `fact_class`
- `proof_grade_edge_class`

Path metadata now includes:

- `proof_grade_edge_classes`
- `derived_edges_have_provenance`
- `edge_class_validation`
- `edge_class_issues` for hard failures
- `production_proof_eligible`

Hard proof failures downgrade path evidence to `inferred` and cap confidence at `0.49`. Soft non-proof-grade cases, such as heuristic or inverse edges, remain visible but are marked `not_proof_grade` rather than hidden.

## Relation Count Changes

The focused Graph Truth run for `derived_edge_requires_provenance` now separates observed edge classes:

| Fact Class | Count |
| --- | ---: |
| `base_exact` | 32 |
| `base_heuristic` | 3 |
| `inverse` | 11 |
| `reified_callsite` | 6 |
| `derived_cache` | 0 |

Base edges observed: 52. Derived/cache edges observed: 0.

Artifacts:

- `reports/audit/artifacts/12_graph_truth_derived_edge_requires_provenance.json`
- `reports/audit/artifacts/12_graph_truth_derived_edge_requires_provenance.md`

## Fixture Status

`derived_edge_requires_provenance` remains failed:

- missing expected entity `users`;
- missing base `CALLS` edge from `src/service.updateUser` to `src/store.writeUser`;
- missing base `WRITES` edge from `src/store.writeUser` to `users`;
- missing derived `MAY_MUTATE` edge with provenance;
- missing expected base proof path.

This is a semantic extraction/projection blocker, not a benchmark scoring change. The gate is now more honest because it reports zero derived/cache observed edges instead of blending them into base edge volume.

## Tests Run

- `cargo test -p codegraph-query`
- `cargo test -p codegraph-store`
- `cargo test -p codegraph-bench graph_truth`
- `cargo run -p codegraph-cli -- bench graph-truth --cases benchmarks/graph_truth/fixtures/derived_edge_requires_provenance/graph_truth_case.json --fixture-root . --out-json reports/audit/artifacts/12_graph_truth_derived_edge_requires_provenance.json --out-md reports/audit/artifacts/12_graph_truth_derived_edge_requires_provenance.md --fail-on-forbidden --fail-on-missing-source-span`

## Remaining Risks

- Persisted derived/cache `Edge` rows are rejected without provenance, but query-generated `DerivedClosureEdge` values are still packet metadata unless a future phase persists them deliberately.
- Inverse edge relations are separated in reports, but their exact inverse provenance is not yet reified as provenance edge IDs.
- Heuristic classification is based on exactness/unresolved metadata. Extractors must keep setting `static_heuristic` for unresolved facts.
- Graph Truth still cannot prove semantic correctness until import/alias/call/write extraction produces the hand-labeled base facts.

## Next Priority

Fix the base semantic facts first: `CALLS` target resolution, `WRITES` target projection, variable/entity kind for storage objects like `users`, and derived `MAY_MUTATE` generation from validated base paths. Storage optimization should stay blocked until this proof boundary is green.
