# Import Alias Call Resolution Fix

Date: 2026-05-10

## Verdict

Deterministic import/alias/call resolution is stricter after this phase. Exact `CALLS` edges are emitted only for parser-resolved local declarations or index-resolved static imports. Runtime dynamic imports now surface as unresolved heuristic `IMPORTS` facts instead of disappearing or being guessed.

The four required graph-truth fixtures pass:

| Fixture | Result | Notes |
| --- | --- | --- |
| `same_function_name_only_one_imported` | pass | No exact `CALLS` edge is created to the unimported same-name function. |
| `dynamic_import_marked_heuristic` | pass | Dynamic `import("./plugins/" + name)` is explicit and `static_heuristic`, with no guessed exact target. |
| `import_alias_change_updates_target` | pass | Alias retargeting updates the exact `CALLS` edge after mutation. |
| `barrel_export_default_export_resolution` | pass | Supported barrel re-export targets resolve to distinct default/named targets. |

Full graph-truth status after this phase: 6 passed, 4 failed.

## Exact Resolution Rules

- Direct local identifier calls resolve exactly only when the parser symbol table has a single in-scope declaration.
- Duplicate same-scope declarations mark the symbol ambiguous; calls to that name fall back to heuristic unresolved references.
- Named imports resolve exactly through the indexed file map and exported target entity.
- Import aliases resolve exactly to the imported target, while preserving the local alias entity.
- Default imports resolve to the canonical `default` entity, distinct from named exports.
- Supported barrel re-exports use `export { name as alias } from "./module"` and `export { default as alias } from "./module"`.
- Dynamic `import(...)` is represented as an `Import` entity with `resolution=unresolved_dynamic_import`, an `IMPORTS` edge, `exactness=static_heuristic`, and confidence below 1.0.
- Unresolved calls remain static heuristic and carry source spans.

## Tests Added Or Updated

- Named import resolves to exported target.
- Alias import resolves to aliased target and stays parser-verified only for the proven target.
- Default import resolves to canonical default export.
- Local shadowing prevents exact import-backed call insertion.
- Same-name unimported function does not receive exact `CALLS`.
- Supported barrel re-export resolves default and named targets distinctly.
- Dynamic import is emitted as unresolved heuristic import evidence.
- Ambiguous same-scope local calls remain heuristic and do not point to either declaration as exact.

Verification:

- `cargo test -p codegraph-parser call -- --nocapture` passed.
- `cargo test -p codegraph-index audit_ -- --nocapture` passed.
- `cargo test --workspace` passed.
- Required graph-truth fixtures passed.

## Full Graph Truth Result

Artifact: `reports/audit/artifacts/07_import_alias_call_resolution/all_fixtures.json`

| Fixture | Result | Main Remaining Issue |
| --- | --- | --- |
| `admin_user_middleware_role_separation` | fail | Missing role entities and `CHECKS_ROLE` proof edges. |
| `barrel_export_default_export_resolution` | pass | Still passing. |
| `derived_closure_edge_requires_provenance` | fail | Missing table/write entity and derived mutation proof. |
| `dynamic_import_marked_heuristic` | pass | Fixed in this phase. |
| `file_rename_prunes_old_path` | pass | Still passing. |
| `import_alias_change_updates_target` | pass | Still passing. |
| `same_function_name_only_one_imported` | pass | Still passing. |
| `sanitizer_exists_but_not_on_flow` | fail | Missing explicit raw-to-write `FLOWS_TO`. |
| `stale_graph_cache_after_edit_delete` | fail | Expected edge exists, but path/span comparison still fails. |
| `test_mock_not_production_call` | pass | Still passing. |

Exact false positives: none observed. No forbidden edge or forbidden path matched in the full run.

Exact false negatives: remaining failures are non-import/call semantics: `CHECKS_ROLE`, `WRITES`, `MAY_MUTATE`, sanitizer `FLOWS_TO`, and one stale-update path/source-span mismatch.

## Unsupported Patterns

- Namespace imports, star exports, and `export * from ...` are not proof-grade.
- CommonJS `require(...)` is not promoted to exact local resolution.
- Dynamic import template literals and complex runtime expressions are surfaced as heuristic unresolved imports, not resolved targets.
- Member calls such as `obj.method()` remain heuristic unless a future type-aware resolver proves the receiver.
- Cross-language or package-manager resolution remains unsupported for exact `CALLS`.

## Remaining False-Positive Risks

The main remaining exact `CALLS` risk is parser-local resolution in complex JavaScript/TypeScript scopes the current tree-sitter symbol table does not fully model, such as hoisting, overloads, destructuring aliases, namespace imports, and conditional runtime rebinding. The new duplicate-name ambiguity guard reduces the simplest silent overwrite case, but a full TypeScript/LSP-backed resolver is still needed before claiming compiler-grade call resolution.
