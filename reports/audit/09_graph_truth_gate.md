# 09 Graph Truth Gate

Source of truth: `MVP.md`.

## Verdict

**Failed. Current graph correctness is not trustworthy against the adversarial graph-truth fixtures.**

The Graph Truth Gate runner now exists and indexes each hand-labeled fixture, then compares observed graph facts against expected and forbidden entities, edges, paths, source spans, context symbols, and tests. The first full run is intentionally strict: it reports missing facts instead of converting them into partial credit, and it does not change benchmark scoring or extraction behavior.

Artifacts:

- JSON report: `reports/audit/artifacts/09_graph_truth_gate.json`
- Markdown report: `reports/audit/artifacts/09_graph_truth_gate.md`

Command used:

```powershell
cargo run -p codegraph-cli -- bench graph-truth --cases benchmarks/graph_truth/fixtures --fixture-root . --out-json reports/audit/artifacts/09_graph_truth_gate.json --out-md reports/audit/artifacts/09_graph_truth_gate.md --fail-on-forbidden --fail-on-missing-source-span
```

## Code Surfaces

| Surface | Files |
| --- | --- |
| Runner implementation | `crates/codegraph-bench/src/graph_truth.rs`, `crates/codegraph-bench/src/lib.rs` |
| CLI command | `crates/codegraph-cli/src/lib.rs` |
| CLI integration test | `crates/codegraph-cli/tests/cli_smoke.rs` |
| Schema and fixtures consumed | `benchmarks/graph_truth/schemas/graph_truth_case.schema.json`, `benchmarks/graph_truth/fixtures/*/graph_truth_case.json` |
| Index/query code used by runner | `crates/codegraph-index/src/lib.rs`, `crates/codegraph-query/src/lib.rs`, `crates/codegraph-store/src/traits.rs`, `crates/codegraph-store/src/sqlite.rs` |
| Prior audits read | `reports/audit/01_benchmark_validity.md`, `reports/audit/02_graph_schema_edge_taxonomy.md`, `reports/audit/07_source_span_proof_gate.md`, `reports/audit/08_graph_truth_schema.md` |

## Runner Behavior Added

The command is:

```text
codegraph-mcp bench graph-truth --cases <path> --fixture-root <path> --out-json <path> --out-md <path> [--fail-on-forbidden] [--fail-on-missing-source-span] [--update-mode]
```

The runner:

- discovers `graph_truth_case.json` files recursively or accepts a single case file;
- indexes each fixture repo into a temporary SQLite DB;
- loads observed entities, edges, source text, and a context packet;
- compares expected entities, expected edges, forbidden edges, expected paths, forbidden paths, expected source spans, expected/forbidden context symbols, and expected/forbidden tests;
- reports per-case failures, false positives, false negatives, source-span failures, stale failures, context-packet failures, and per-relation precision/recall where possible;
- fails on forbidden facts when `--fail-on-forbidden` is set;
- fails on missing or wrong proof-grade spans when `--fail-on-missing-source-span` is set;
- keeps mutation/update assertions visible as expected/forbidden facts instead of silently skipping them.

## First Run Summary

| Metric | Result |
| --- | ---: |
| Cases total | 10 |
| Cases passed | 0 |
| Cases failed | 10 |
| Expected entities matched | 12 / 16 |
| Expected edges matched | 0 / 17 |
| Forbidden edges observed | 0 / 11 |
| Expected paths matched | 0 / 5 |
| Forbidden paths observed | 0 / 4 |
| Source-span failures | 0 |
| Context-packet failures | 3 |

No source-span failures appeared because no expected proof-grade edge matched closely enough to reach span validation. That is a negative signal: the graph is missing or mis-resolving required facts before the stricter source-span gate can become informative.

## Fixture Results

| Fixture | Status | Expected edges | Forbidden hits | Expected paths | Forbidden path hits | Main failure |
| --- | --- | ---: | ---: | ---: | ---: | --- |
| `admin_user_roles_not_conflated` | failed | 0 / 2 | 0 / 2 | 0 / 0 | 0 / 0 | Missing `CHECKS_ROLE` facts and role context symbols. |
| `derived_edge_requires_provenance` | failed | 0 / 3 | 0 / 1 | 0 / 1 | 0 / 1 | Missing base `CALLS`/`WRITES` and derived `MAY_MUTATE` provenance path. |
| `dynamic_import_marked_heuristic` | failed | 0 / 1 | 0 / 1 | 0 / 0 | 0 / 0 | Missing dynamic import fact. |
| `file_rename_prunes_old_path` | failed | 0 / 1 | 0 / 1 | 0 / 0 | 0 / 0 | Missing post-rename `CALLS` target. |
| `import_alias_change_updates_target` | failed | 0 / 2 | 0 / 1 | 0 / 0 | 0 / 0 | Missing `ALIASED_BY` and alias-resolved `CALLS`. |
| `mock_call_not_production_call` | failed | 0 / 3 | 0 / 1 | 0 / 1 | 0 / 1 | Missing production `CALLS`, `MOCKS`, and `STUBS`. |
| `same_name_only_one_imported` | failed | 0 / 2 | 0 / 1 | 0 / 1 | 0 / 1 | Missing import-resolved same-name call target. |
| `sanitizer_exists_but_not_on_flow` | failed | 0 / 1 | 0 / 1 | 0 / 1 | 0 / 1 | Missing raw `FLOWS_TO` path. |
| `source_span_exact_callsite` | failed | 0 / 1 | 0 / 1 | 0 / 1 | 0 / 0 | Missing exact `CALLS` to the second callsite. |
| `stale_cache_after_delete` | failed | 0 / 1 | 0 / 1 | 0 / 0 | 0 / 0 | Missing live post-delete `CALLS` target. |

## Relation Results

| Relation | Expected edges | Matched expected | Forbidden edges | Forbidden hits | Recall |
| --- | ---: | ---: | ---: | ---: | ---: |
| `CALLS` | 7 | 0 | 7 | 0 | 0.000 |
| `CHECKS_ROLE` | 2 | 0 | 2 | 0 | 0.000 |
| `IMPORTS` | 2 | 0 | 0 | 0 | 0.000 |
| `ALIASED_BY` | 1 | 0 | 0 | 0 | 0.000 |
| `FLOWS_TO` | 1 | 0 | 0 | 0 | 0.000 |
| `MAY_MUTATE` | 1 | 0 | 1 | 0 | 0.000 |
| `MOCKS` | 1 | 0 | 0 | 0 | 0.000 |
| `STUBS` | 1 | 0 | 0 | 0 | 0.000 |
| `WRITES` | 1 | 0 | 0 | 0 | 0.000 |
| `SANITIZES` | 0 | 0 | 1 | 0 | unknown |

Precision is unknown for most relations because the gate did not match any expected or forbidden relation facts in the first run. Recall is 0 for every relation with expected edges.

## Top False Negatives

- `same_name_only_one_imported`: missing `IMPORTS` from `src/main` to `src/a.chooseUser`.
- `same_name_only_one_imported`: missing `CALLS` from `src/main.handler` to the imported `src/a.chooseUser`.
- `source_span_exact_callsite`: missing exact `CALLS` from `src/main.run` to `src/actions.second` at the second callsite.
- `import_alias_change_updates_target`: missing `ALIASED_BY` from `src/beta.target` to `activeTarget`.
- `import_alias_change_updates_target`: missing alias-resolved `CALLS` from `src/use.run` to `src/beta.target`.
- `mock_call_not_production_call`: missing `MOCKS` and `STUBS` facts for the test mock.
- `admin_user_roles_not_conflated`: missing `CHECKS_ROLE` facts for admin and user routes.
- `derived_edge_requires_provenance`: missing base `CALLS`/`WRITES` and derived `MAY_MUTATE`.
- `sanitizer_exists_but_not_on_flow`: missing raw input-to-save `FLOWS_TO`.
- `dynamic_import_marked_heuristic`: missing dynamic import fact that should be heuristic, not exact.

## Top False Positives

None were observed in this first run. This does not mean the graph avoids false positives; it means the current graph failed earlier by not matching the required hand-labeled facts. Once required semantic edges begin matching, the forbidden facts and paths will become the sharper false-positive gate.

## Trustworthiness

The current implementation is **not trustworthy** as a graph-correctness proof against these fixtures. It can still be useful for smoke indexing and broad graph exploration, but it does not currently satisfy the MVP proof model for hand-labeled exact facts, import/alias resolution, test/mock separation, security role facts, derived provenance, or exact callsite paths.

The most important positive outcome is that the gate now exposes this honestly. It does not convert missing graph truth into a benchmark win.

## Next Fix Priority

1. Make TS/JS import and alias resolution produce endpoint-stable `IMPORTS`, `ALIASED_BY`, and `CALLS` facts against declaration entities.
2. Ensure proof-grade `CALLS` edges point at the exact callsite span and the resolved target, not only an unresolved textual callee.
3. Add first-class production/test/mock context so `MOCKS`/`STUBS` cannot satisfy production paths.
4. Emit or derive security/auth role facts such as `CHECKS_ROLE` only when source evidence supports them.
5. Implement update-mode fixture metadata for rename/delete pre-state replay so stale-cache assertions are tested through actual mutation, not only final-state forbidden facts.

## Tests Run

- `cargo test -p codegraph-bench graph_truth --lib`
- `cargo test -p codegraph-cli --test cli_smoke graph_truth`
- `cargo run -p codegraph-cli -- bench graph-truth --cases benchmarks/graph_truth/fixtures --fixture-root . --out-json reports/audit/artifacts/09_graph_truth_gate.json --out-md reports/audit/artifacts/09_graph_truth_gate.md --fail-on-forbidden --fail-on-missing-source-span`
- `cargo fmt --check`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
