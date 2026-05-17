# Language Frontends

The root `README.md` is the public setup contract. CodeGraph stays Rust-first,
exact graph first, and vectors second.

Language frontends feed the same attributed graph without flattening all
languages into text. Python, Go, and Rust currently expose conservative Tier 3
caller/callee extraction while unresolved calls remain explicitly heuristic.

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

- TypeScript/TSX and JavaScript/JSX keep the existing extractor behavior.
- TypeScript/TSX advertise an optional TypeScript Compiler API resolver hook with `compiler_verified` exactness when available.
- Python, Go, and Rust are Tier 3: syntax/entity/import-export facts plus conservative parser-level calls and caller/callee edges.
- Java, C#, C, C++, Ruby, and PHP are Tier 1: syntax/entity extraction with explicit limitations.

## Proof Rules

- Tree-sitter facts are `parser_verified`.
- TypeScript compiler facts are `compiler_verified`.
- Future LSP facts must be `lsp_verified`.
- Unresolved or best-effort fallback facts must be `static_heuristic`.
- New language frontends must not claim dataflow, security, or test-impact support until fixture-backed extractors exist.
