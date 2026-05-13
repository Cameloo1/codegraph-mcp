# Derived Provenance Fixture Fix

Completed: 2026-05-11 03:47:44 -05:00

## Verdict

`derived_closure_edge_requires_provenance` now passes.

Strict Graph Truth Gate now passes all adversarial fixtures: 11/11. Forbidden edge hits remain 0, forbidden path hits remain 0, and source-span failures remain 0.

## Diagnosis

The fixture was correct. The implementation was missing a supported table-sink pattern and a persisted derived proof shortcut.

- Entity extraction gap: `ordersTable` was only modeled as a local variable. A table-like constant should also be represented as a `Table` entity for this supported fixture pattern.
- Table/sink modeling gap: `saveOrder` returned `ordersTable`, but that table constant was not interpreted as the mutation sink for a writer-like function.
- `WRITES` extraction gap: no `WRITES saveOrder -> ordersTable` edge was emitted with the exact `ordersTable` source span.
- Derived construction gap: the indexer generated query-time closure evidence, but did not persist a `MAY_MUTATE submitOrder -> ordersTable` edge row with `derived=true`.
- Provenance persistence gap: the required derived edge needed base edge provenance from `CALLS submitOrder -> saveOrder` and `WRITES saveOrder -> ordersTable`.
- Graph Truth matching gap: forbidden base shortcuts must not match a valid derived/provenanced edge with the same endpoints and source span.

## Implementation

- Added table-constant extraction for names ending in `Table`.
- Added writer-return sink extraction for direct table-constant returns from writer-like functions such as `saveOrder`.
- Added persisted derived mutation closure reduction:
  - `CALLS f -> g` plus `WRITES/MUTATES g -> sink` emits `MAY_MUTATE f -> sink`.
  - The derived edge is `derived=true`, `edge_class=derived`, `exactness=derived_from_verified_edges` when base edges are verified, and carries the base edge IDs in `provenance_edges`.
- Allowed static named-import resolution to target `Table` entities.
- Tightened exact Graph Truth edge/path matching to include derived/exactness/confidence semantics, so a valid derived edge does not satisfy a forbidden base-exact shortcut.
- Updated the compact final gate check so derived-cache relation rows are accepted only when derived, classified, and provenanced.

## Evidence

Commands run:

```text
cargo test -q -p codegraph-parser table_constant_return_from_writer_produces_write_to_table
cargo test -q -p codegraph-index derived_mutation_closure_is_persisted_with_base_provenance
cargo test -q -p codegraph-bench graph_truth_detects_unresolved_exact_and_derived_without_provenance
cargo test -q -p codegraph-store derived_edges_without_provenance_are_rejected
cargo test -q -p codegraph-query derived_edge_without_provenance
cargo run -q -p codegraph-cli -- bench graph-truth --cases benchmarks\graph_truth\fixtures --fixture-root . --out-json reports\audit\artifacts\derived_provenance_graph_truth.json --out-md reports\audit\artifacts\derived_provenance_graph_truth.md --fail-on-forbidden --fail-on-missing-source-span --fail-on-unresolved-exact --fail-on-derived-without-provenance --fail-on-test-mock-production-leak --update-mode
cargo test --workspace
```

Graph Truth result:

- Cases: 11 total, 11 passed, 0 failed.
- `derived_closure_edge_requires_provenance`: passed, 3/3 expected edges.
- Observed derived/cache edges: 1.
- Forbidden edge hits: 0.
- Forbidden path hits: 0.
- Source-span failures: 0.
- `MAY_MUTATE` precision/recall on fixture suite: 1.000 / 1.000.
- `WRITES` precision/recall on fixture suite: 1.000 / 1.000.

Workspace tests:

- `cargo test --workspace`: passed.
- Ignored tests remained intentionally ignored for missing external tools/configuration.

## Remaining Notes

This fix does not weaken derived/provenance semantics. `MAY_MUTATE` remains rejected by storage unless it is derived and has provenance, and the existing derived-without-provenance tests still pass.
