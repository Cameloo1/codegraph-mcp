# Stable Symbol Identity Fix

Date: 2026-05-10

## Verdict

Stable symbol identity is now materially safer for the identity-sensitive fixtures. The four required graph-truth cases now pass:

| Fixture | Result | Notes |
| --- | --- | --- |
| `same_function_name_only_one_imported` | pass | Same-name exported functions stay distinct and only the imported target receives the exact `CALLS`. |
| `barrel_export_default_export_resolution` | pass | Canonical default export entity resolves separately from named export through the barrel. |
| `import_alias_change_updates_target` | pass | Alias retargets after mutation and stale target facts are pruned. |
| `test_mock_not_production_call` | pass | Mock/test facts are separate from production `CALLS`; expected `MOCKS`, `STUBS`, and `ASSERTS` are visible. |

Full graph-truth status after this phase: 5 passed, 5 failed. The implementation is trustworthy for the identity cases fixed here, but not yet trustworthy as a full semantic proof engine.

## Identity Model

Entity IDs remain deterministic hashes over normalized repo-relative path plus semantic identity. The semantic identity includes entity kind, qualified name, and declaration-span signature where available. Qualified names carry the module/class/container path, so same function names in different files and same method names in different classes do not collide.

Changes made in this phase:

- Duplicate source content at different paths is no longer skipped during full indexing. Each path gets its own file and symbol facts.
- Local module resolution now collapses lexical `.` and `..` segments before matching indexed paths, so imports like `../src/service` resolve to `src/service.ts`.
- Default exports now get a canonical `default` function/class entity in addition to the underlying named declaration.
- Barrel re-exports of named and default exports are followed by the static import resolver.
- Test mock module factories create separate `Mock` entities and exact `MOCKS`/`STUBS`/`ASSERTS` edges without overwriting production symbols.
- Graph Truth context seeding no longer forces entities marked as forbidden context symbols into the packet.

No storage-size optimization was done. One previous size optimization, duplicate-content graph-fact elision, was removed because it violated path identity.

## Tests Added Or Updated

- Same function name in two files has distinct IDs.
- Same method name in two classes has distinct IDs.
- Class method and standalone function do not collide.
- Default export and named export remain distinct.
- Barrel re-export resolves default and named targets distinctly.
- Import alias points to the exported target.
- Duplicate file content preserves separate file and symbol identity.
- Test mock symbol does not overwrite production symbol.
- Static test mock factory links to production target without production overwrite.
- Existing graph-truth fixture expectations were updated to the current exclusive end-column source-span convention.

Verification:

- `cargo test --workspace` passed.
- `codegraph-mcp bench graph-truth` passed the four required identity fixtures.
- Full `bench graph-truth` currently reports 5/10 passing.

## Full Graph Truth Result

Artifact: `reports/audit/artifacts/06_identity_after/all_fixtures.json`

| Fixture | Result | Main Remaining Issue |
| --- | --- | --- |
| `admin_user_middleware_role_separation` | fail | Missing role entities and `CHECKS_ROLE` proof edges. |
| `barrel_export_default_export_resolution` | pass | Fixed. |
| `derived_closure_edge_requires_provenance` | fail | Missing table/write entity and derived mutation proof. |
| `dynamic_import_marked_heuristic` | fail | Dynamic import entity/edge not emitted. |
| `file_rename_prunes_old_path` | pass | Existing stale-prune behavior holds. |
| `import_alias_change_updates_target` | pass | Fixed. |
| `same_function_name_only_one_imported` | pass | Fixed. |
| `sanitizer_exists_but_not_on_flow` | fail | Missing explicit raw-to-write `FLOWS_TO`. |
| `stale_graph_cache_after_edit_delete` | fail | Expected edge exists, but callsite span/path comparison still fails. |
| `test_mock_not_production_call` | pass | Fixed. |

Exact false positives: no forbidden edges or forbidden paths were observed in the full run.

Exact false negatives: missing `CHECKS_ROLE`, `WRITES`, `MAY_MUTATE`, dynamic `IMPORTS`, and sanitizer `FLOWS_TO` facts remain in non-identity fixtures.

## Remaining Risks

- Static import/barrel support is intentionally narrow: simple named/default imports and `export { ... } from ...`; namespace exports, star exports, and expression default exports are still not proof-grade.
- Dynamic imports are not represented yet, even as explicit heuristic facts.
- Role, table-write, derived mutation, and sanitizer flow semantics remain incomplete.
- Path evidence is generated at query time for the gate, but broader stored PathEvidence quality is still a known audit gap from `05_relation_source_span_audit.md`.
- The stale-update fixture still exposes a callsite source-span/path comparison mismatch.

Highest-priority semantic bug to fix next: relation-specific semantic extraction for proof-grade security/data-flow facts, starting with role literal `CHECKS_ROLE` edges because that fixture misses both the role entities and the proof paths.
