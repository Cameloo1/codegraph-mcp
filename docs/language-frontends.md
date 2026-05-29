# Language Frontends

The root `README.md` is the public setup contract. CodeGraph stays Rust-first,
exact graph first, and vectors second.

Language frontends feed the same attributed graph without flattening all
languages into text. The current fixture matrix covers declarations/imports,
calls, reads/writes, return-flow, test/mock/assertion evidence, async/event
patterns, dynamic import boundaries, generated-source boundaries, same-name
symbols, and macro/preprocessor blindness.

## Support Tiers

| Tier | Meaning |
| ---: | --- |
| 0 | File discovery only |
| 1 | Tree-sitter syntax/entity extraction |
| 2 | Imports, exports, packages, or namespace-equivalent facts |
| 3 | Calls and caller/callee extraction |
| 4 | Compiler or LSP verified resolution |
| 5 | Dataflow, security, or test-impact facts |

Use `codegraph-mcp languages` for the table view and `codegraph-mcp languages --json` for machine-readable capability metadata.

## Current Frontends

- TypeScript/TSX and JavaScript/JSX include syntax/entity/import/export
  extraction, parser-backed direct calls, reads/writes where AST-scoped, test
  blocks/assertions/mocks, route/event/security patterns where fixture-backed,
  and explicit dynamic-boundary labels for computed imports/calls.
- TypeScript/TSX advertise an optional TypeScript Compiler API resolver hook
  with `compiler_verified` exactness when available.
- Python, Go, and Rust include syntax/entity/import-export facts plus
  conservative parser-level calls, caller/callee edges, reads/writes where
  AST-scoped, and fixture-backed test/source-role boundaries where supported.
- Java, C#, C, C++, Ruby, and PHP expose syntax/entity extraction and selected
  conservative relations where the parser fixture proves them. Unsupported
  dynamic or semantic behavior remains heuristic or unknown.

## Proof Rules

- Tree-sitter facts are `parser_verified`.
- TypeScript compiler facts are `compiler_verified`.
- Future LSP facts must be `lsp_verified`.
- Unresolved or best-effort fallback facts must be `static_heuristic`.
- New language frontends must not claim dataflow, security, or test-impact support until fixture-backed extractors exist.
- Macro expansion, C/C++ preprocessor branches, dependency injection,
  monkeypatching, dynamic dispatch, computed callback targets, JavaScript
  coercion behavior, prototype-pollution reachability, and type-shape mutation
  must not be labeled exact unless a fixture-backed semantic pass proves them.
- Generated/dependency/build artifacts should remain excluded by scope policy
  unless explicitly included for a diagnostic run.

The machine-readable language matrix schema lives at
`docs/schemas/language_coverage_matrix.schema.json`.
