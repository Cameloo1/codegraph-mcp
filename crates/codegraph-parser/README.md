# codegraph-parser

Parser abstraction crate.

`MVP.md` is the source of truth for this crate's phase boundary and acceptance
criteria.

The release registry covers JavaScript, JSX, TypeScript, TSX, Python, Go, Rust,
Java, C#, C, C++, Ruby, and PHP. Each frontend publishes broad capability rows
and a separate `scoped_readiness` contract. All 13 canonical production
frontends pass the representative, source-aware, same-file intraprocedural
local-flow gate with exact local binding/read-write support, derived flow with
provenance, and exact local-flow packet support.

That scoped Tier 5 result does not imply project-wide, compiler, runtime,
framework, dynamic-dispatch, macro, preprocessor, alias-analysis, or security
completeness. Broad capability rows preserve those conservative boundaries.
See [Language Frontends](../../docs/language-frontends.md#scoped-tier-5-mvp4-readiness).

The parser emits owned syntax metadata, source-spanned entities and relations,
micro-nodes, and micro-edges for downstream indexing. TypeScript `.ts` uses the
bounded legacy-v1 local adapter; `.mts` and `.cts` use `ParserFactsV1`; `.d.ts`
remains inactive. Parser recovery, unsupported syntax, and dynamic behavior
must fail closed rather than fabricate exact facts.

Guardrail: graph query APIs, persistence, vector retrieval, MCP behavior, UI
behavior, and benchmark execution do not live in this crate.
