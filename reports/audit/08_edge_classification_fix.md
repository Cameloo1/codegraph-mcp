# Edge Classification Fix

Date: 2026-05-10

## Verdict

Implemented first-class stored edge classes and execution context labels so proof paths no longer depend on late, report-only inference.

Graph Truth Gate after this change:

- Command: `cargo run -q -p codegraph-cli -- bench graph-truth --cases benchmarks/graph_truth/fixtures --fixture-root benchmarks/graph_truth/fixtures --out-json reports/audit/artifacts/08_edge_classification_graph_truth.json --out-md reports/audit/artifacts/08_edge_classification_graph_truth.md --fail-on-forbidden --fail-on-missing-source-span --fail-on-unresolved-exact --fail-on-derived-without-provenance --fail-on-test-mock-production-leak --update-mode --verbose`
- Result: 10 cases, 6 passed, 4 failed.
- Forbidden hits: 0.
- Stale-update failures: 0.
- Source-span failures: 1.
- Derived edges observed: 0.

## Schema Changes

Core edge model now stores:

- `edge_class`
- `context`

Required edge classes are represented as:

- `base_exact`
- `base_heuristic`
- `reified_callsite`
- `derived`
- `test`
- `mock`
- `mixed`
- `unknown`

SQLite schema version moved to `5` and adds compact dictionaries:

- `edge_class_dict`
- `edge_context_dict`

The `edges` table now stores:

- `edge_class_id INTEGER`
- `context_id INTEGER`

Existing compact databases are migrated by adding the two columns and backfilling conservative classifications from relation, exactness, derived flag, and relation kind. New writes always normalize classification before storage.

## Classification Logic

Stored edge classification is normalized at insertion:

| Condition | Stored class/context |
| --- | --- |
| Parser/compiler/LSP/exact production base edge | `base_exact` / `production` |
| Static heuristic, inferred, or unresolved relation | `base_heuristic` |
| Reified callsite relations such as `CALLEE`, `ARGUMENT_0`, `RETURNS_TO` | `reified_callsite` |
| Derived/cache relations or `derived=true` | `derived` |
| Test relations or test paths | `test` |
| Mock/stub relations or mock-looking endpoints | `mock` |
| Mixed context | `mixed` |
| Inverse/raw ambiguous class | `unknown` |

Proof-path validation now rejects by default:

- `base_heuristic`
- `test`
- `mock`
- `mixed`
- `unknown`
- derived edges without provenance
- derived/cache relations not marked derived
- unresolved exact edges
- inverse edges treated as raw base proof

Context packets now expose per-edge:

- exactness
- confidence
- extractor
- edge class
- context
- derived flag
- provenance edge IDs
- source span
- file hash

Test-impact context still intentionally allows test/mock paths into packets, but those paths are not marked as production proof eligible.

## Fixture Results

| Fixture | Status | Notes |
| --- | --- | --- |
| `admin_user_middleware_role_separation` | failed | Missing role entities and `CHECKS_ROLE` extraction. |
| `barrel_export_default_export_resolution` | passed | Default/named/barrel behavior unchanged. |
| `derived_closure_edge_requires_provenance` | failed | Missing `ordersTable`, base `WRITES`, and derived `MAY_MUTATE` fact; no derived edges observed. |
| `dynamic_import_marked_heuristic` | passed | Dynamic import remains heuristic, not exact proof. |
| `file_rename_prunes_old_path` | passed | Update-mode stale edge check passes. |
| `import_alias_change_updates_target` | passed | Alias mutation updates target under update mode. |
| `same_function_name_only_one_imported` | passed | Same-name import collision remains fixed. |
| `sanitizer_exists_but_not_on_flow` | failed | Missing `FLOWS_TO` raw-to-write dataflow edge. |
| `stale_graph_cache_after_edit_delete` | failed | Edge is current, but source span column does not match expected callsite column. |
| `test_mock_not_production_call` | passed | Test/mock edges are stored as `test`/`mock` and excluded from production proof. |

## Edge Class Counts

From the update-mode graph-truth run:

| Edge class | Count |
| --- | ---: |
| `base_exact` | 439 |
| `base_heuristic` | 32 |
| `mock` | 3 |
| `reified_callsite` | 69 |
| `test` | 65 |
| `unknown` | 161 |

No `derived` edges were observed in the fixture run, which is now visible as a semantic gap rather than being blended into base facts.

## False Positives

No forbidden edges or forbidden paths were hit in the update-mode run.

## False Negatives

Highest-signal missing facts:

- `CHECKS_ROLE` edges for admin/user middleware role separation.
- `WRITES` from `src/store.saveOrder` to `src/store.ordersTable`.
- `MAY_MUTATE` from `src/service.submitOrder` to `src/store.ordersTable`.
- `FLOWS_TO` from raw comment input to write sink.
- Expected stale-update path has a source span column mismatch.

## Storage Impact Estimate

The change adds two integer IDs per edge plus two tiny dictionaries. SQLite varints keep the per-edge payload compact; practical added payload is expected to be roughly 2-18 bytes per edge before page overhead, depending on dictionary ID size. For 1,000,000 edges this is roughly 2-18 MB of raw row payload plus ordinary SQLite page overhead. No storage optimization was done in this phase.

## Trustworthiness

The implementation is more trustworthy for proof hygiene because exactness, provenance, derived status, and test/mock context are now stored and queryable instead of inferred only during reports.

The current semantic graph is not fully trustworthy yet:

- Required security/dataflow/derived facts are still missing.
- No derived fixture edges are produced yet.
- Some inverse/structural edges are explicitly `unknown` and cannot be used as raw proof.

Highest-priority semantic bug: derived/dataflow extraction still fails to produce table-like sink entities and provenance-backed closure edges, especially the `WRITES` plus `MAY_MUTATE` chain in `derived_closure_edge_requires_provenance`.

## Verification

- `cargo test -q` passed.
- Graph Truth Gate emitted:
  - `reports/audit/artifacts/08_edge_classification_graph_truth.json`
  - `reports/audit/artifacts/08_edge_classification_graph_truth.md`
