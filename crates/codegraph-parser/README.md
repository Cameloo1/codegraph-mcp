# codegraph-parser

Parser abstraction crate.

`MVP.md` is the source of truth for this crate's phase boundary and acceptance
criteria.

Phase 04 implemented `LanguageParser`, `ParsedFile`, owned syntax-node
metadata, file-type detection, robust syntax diagnostics, and tree-sitter
parsing for JavaScript, JSX, TypeScript, and TSX.

Phase 05 added basic extraction for file/module/declaration/import/export facts
and structural edges.

Phase 06 adds conservative `CallSite`/`ReturnSite` creation and core
syntax-derived relations for calls, callees, arguments, returns, reads, writes,
mutations, assignments, and direct flows.

Phase 07 adds best-effort pattern extractors for common TS/JS auth/security,
event/async, persistence/schema, and Jest/Vitest-style test relations. Every
Phase 07 edge is `static_heuristic` and carries pattern/framework metadata.

Guardrail: no exact graph query APIs, vector retrieval, MCP behavior, UI
behavior, or benchmark execution lives here.
