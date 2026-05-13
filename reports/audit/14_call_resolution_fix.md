# 14 Call Resolution Fix

Source of truth: `MVP.md`.

## Verdict

**Complete with scoped support.** Same-name exported functions no longer get false exact `CALLS` edges when only one target is imported, import aliases resolve to their aliased target, local declarations shadow imports, and unresolved/dynamic calls remain heuristic.

This phase does not implement a full TypeScript compiler resolver. Unsupported patterns stay non-proof-grade unless a later compiler/LSP pass proves them.

## Current Resolution Rules

| Pattern | Behavior |
| --- | --- |
| Same-file local declaration call | Parser scope table resolves exact `CALLS` with `parser_verified`. |
| Named local import | Index resolver resolves the module path, emits `File IMPORTS target`, `target ALIASED_BY import`, and exact `CALLS` from the containing executable to the imported target. |
| Named import alias | The local alias is resolved to the imported declaration, not a same-name fallback. |
| Default import | Resolved when the target file has a parseable `export default` declaration whose declaration entity is present. |
| Local shadowing | A real local declaration in the containing executable suppresses the imported exact `CALLS` edge. |
| Dynamic/subscript/member target without proof | Remains `static_heuristic` with confidence below 1.0. |
| Barrel/re-export/package import | Not promoted to exact by this phase; remains unsupported or heuristic unless another resolver proves it. |

## Code Changes

| Surface | Files / functions |
| --- | --- |
| Static import parser | `crates/codegraph-index/src/lib.rs`: `parse_static_imports`, `default_import_name`, `StaticImportKind` |
| Named/default import target resolution | `crates/codegraph-index/src/lib.rs`: `resolve_import_target`, `resolve_default_import_target`, `resolve_local_module_path` |
| Exact import proof edge | `crates/codegraph-index/src/lib.rs`: `file_entity_for_path`, `resolved_import_edge` |
| Shadowing guard | `crates/codegraph-index/src/lib.rs`: `local_declaration_shadows_import`, `span_contains` |
| Imported callsite matching | `crates/codegraph-index/src/lib.rs`: `call_spans_for_local_name`, `containing_executable` |
| File symbol matching support | `crates/codegraph-parser/src/lib.rs`: file entity names now use the module-style stem while preserving path-qualified identity. |

## False-Positive Reduction

Before this phase, `same_name_only_one_imported` failed because CodeGraph did not emit the exact `IMPORTS` proof edge from `src/main` to `src/a.chooseUser`. The imported `CALLS` edge was already present after phase 13, but the fixture could not fully pass.

After this phase:

- `same_name_only_one_imported` passes with `CALLS` recall 1.000 and `IMPORTS` recall 1.000.
- The forbidden `src/main.handler CALLS src/b.chooseUser` edge is absent.
- `import_alias_change_updates_target` still passes.
- A new local-shadowing test proves the resolver does not add an imported exact `CALLS` edge when a local declaration owns the call.

## Graph Truth Results

| Fixture | Result | Evidence |
| --- | --- | --- |
| `same_name_only_one_imported` | Passed | `reports/audit/artifacts/14_graph_truth_same_name_only_one_imported.json`, `reports/audit/artifacts/14_graph_truth_same_name_only_one_imported.md` |
| `import_alias_change_updates_target` | Passed | `reports/audit/artifacts/14_graph_truth_import_alias_change_updates_target.json`, `reports/audit/artifacts/14_graph_truth_import_alias_change_updates_target.md` |

## Tests Added

| Test | Purpose |
| --- | --- |
| `audit_same_name_only_imported_target_gets_exact_call` | Proves only the imported same-name target receives the exact `CALLS` edge and the `IMPORTS` edge has the import declaration span. |
| `audit_default_import_resolves_to_default_export_when_supported` | Proves supported default imports resolve to the parseable default-export declaration. |
| `audit_local_shadowing_prevents_imported_exact_call` | Proves local declarations suppress imported exact calls. |
| `dynamic_call_targets_remain_heuristic` | Proves dynamic call targets do not become exact. |

## Unsupported JS/TS Patterns

- Barrel re-exports and `export * from ...`.
- Package imports and path aliases from `tsconfig`.
- Namespace imports such as `import * as api from "./api"`.
- CommonJS `require` and dynamic `import(...)`.
- Anonymous default exports without a declaration entity.
- Property/method dispatch such as `obj.method()` unless a local scope symbol proves it.

These patterns should be handled by a future TypeScript compiler/LSP resolver and must remain heuristic until proven.

## Next Work

1. Add compiler/LSP-backed resolution for barrels, package aliases, and default export edge cases.
2. Replace line-scanned imported call matching with AST callsite matching.
3. Add edge metadata that distinguishes parser scope proof from post-index import resolution proof.
