# Implementation Guardrails

Use the root `README.md` for the public project contract. These notes preserve
implementation guardrails for local development.

## Current Report Boundary

Do not infer the current project status from old phase text, raw benchmark
payloads, or local DB files. Use the stable report summaries:

- `reports/final/comprehensive_benchmark_latest.md` / `.json`
- `reports/final/intended_tool_quality_gate.md` / `.json`
- `reports/final/manual_relation_precision.md` / `.json`
- `reports/comparison/codegraph_vs_cgc_latest.md` / `.json`

Current public stance:

- Graph Truth Gate: 11/11.
- Context Packet Gate: 11/11.
- Manual precision: 320 labeled samples, sampled precision only, recall
  unknown.
- Intended Tool Quality Gate: **FAIL** in the stable report because
  `proof_build_only_ms = 184,297 ms` exceeds `<=60,000 ms`.
- CGC comparison: diagnostic/incomplete, no CodeGraph superiority claim.

Generated DBs, benchmark workdirs, CGC raw artifacts, raw logs, and diagnostic
lab payloads must not become public claims or branch content unless explicitly
promoted by a reviewed summary.

## Delivery Discipline

- Keep implementation milestones sequential and independently testable.
- Keep every phase independently testable.
- Add one narrow capability per phase.
- Do not implement graph storage, parsing, vector retrieval, MCP, UI, and
  benchmarks together.
- Defer advanced research features until benchmark evidence identifies the
  remaining gaps.

## Current Phase Boundary

The phase list below is historical implementation context. It is not a green
release verdict and does not override the stable reports above.

The current public baseline includes the implementation milestones below. Phase
00 created:

- repository skeleton
- Rust workspace crate boundaries
- placeholder UI directory
- README and docs
- `.gitignore`
- local milestone checklist
- smoke-test placeholder

Phase 01 adds:

- `codegraph-mcp` CLI binary
- placeholder command parsing
- unit and CLI smoke tests
- formatting and linting setup
- CI workflow
- placeholder backend feature flags

Phase 02 adds:

- MVP entity, relation, and exactness enums
- source spans, graph entities, edges, file/index records
- path evidence, derived closure edges, context packet shape
- stable entity/edge ID helpers
- serde serialization and parsing helpers
- broad relation domain/codomain validation helpers

Phase 03 adds:

- `GraphStore` trait
- `SqliteGraphStore` using `rusqlite`
- migrations for all MVP storage tables
- MVP storage indexes
- schema versioning
- in-memory SQLite test support
- transaction rollback helper

Phase 04 adds:

- `LanguageParser` parser abstraction
- `ParsedFile` parse metadata
- owned `SyntaxNodeRef` metadata wrapper
- JavaScript, JSX, TypeScript, and TSX tree-sitter parsing
- file-type detection
- syntax-error diagnostics without panics
- parser fixtures

Phase 05 adds:

- basic file/module/declaration/import/export entity extraction
- stable entity IDs and source spans for extracted entities
- structural edges: `CONTAINS`, `DEFINED_IN`, `DEFINES`, `DECLARES`, `IMPORTS`,
  `EXPORTS`
- `codegraph-mcp index <repo>` persistence into `.codegraph/codegraph.sqlite`
- index summary output

Phase 06 adds:

- `CallSite` and `ReturnSite` entities
- conservative core relations: `CALLS`, `CALLEE`, `ARGUMENT_0`, `ARGUMENT_1`,
  `ARGUMENT_N`, `RETURNS`, `RETURNS_TO`, `READS`, `WRITES`, `MUTATES`,
  `ASSIGNED_FROM`, and direct `FLOWS_TO`
- `parser_verified` exactness for syntax-derived facts
- `static_heuristic` exactness and lower confidence for unresolved best-effort
  symbols
- relation domain/codomain validation before extracted edges are inserted

Phase 07 adds:

- pattern-based auth/security relations: `AUTHORIZES`, `CHECKS_ROLE`,
  `CHECKS_PERMISSION`, `SANITIZES`, `VALIDATES`, `EXPOSES`,
  `TRUST_BOUNDARY`, `SOURCE_OF_TAINT`, and `SINKS_TO`
- pattern-based event/async relations: `PUBLISHES`, `EMITS`, `CONSUMES`,
  `LISTENS_TO`, `SUBSCRIBES_TO`, `HANDLES`, `SPAWNS`, and `AWAITS`
- pattern-based persistence/schema relations: `MIGRATES`, `READS_TABLE`,
  `WRITES_TABLE`, `ALTERS_COLUMN`, and `DEPENDS_ON_SCHEMA`
- pattern-based test relations: `TESTS`, `ASSERTS`, `MOCKS`, `STUBS`,
  `COVERS`, and `FIXTURES_FOR`
- `static_heuristic` exactness, sub-1.0 confidence, source spans, and
  extractor metadata for every Phase 07 edge

Phase 08 adds:

- `ExactGraphQueryEngine`
- query APIs for callers, callees, reads, writes, mutations, dataflow, auth
  paths, event flow, tests, migrations, path tracing, and core impact analysis
- bounded BFS and cycle-safe traversal
- relation-filtered forward and reverse traversal
- Dijkstra-style shortest path and k-shortest bounded path search
- path length and uncertainty penalties from edge exactness/confidence
- returned paths with edge provenance, traversal direction, and source spans

Phase 09 adds:

- `PathEvidence` generation from query results
- explainable `DerivedClosureEdge` shortcuts: `MAY_MUTATE`, `MAY_READ`,
  `API_REACHES`, `ASYNC_REACHES`, and `SCHEMA_IMPACT`
- provenance base edges for every derived edge
- graph-only `context_pack(task, token_budget, mode)` using explicit seeds
- source-span snippet extraction
- risk summaries from relation/path patterns
- recommended test selection from `TESTS`, `ASSERTS`, `MOCKS`, `STUBS`, and
  `COVERS` evidence

Phase 10 adds:

- prompt seed extraction for symbols, file paths, line numbers, stack traces,
  test names, error messages, and code-like identifiers
- exact seed merge into graph-only `context_pack`
- explicit metadata that exact seeds bypass vector filters
- SQLite FTS5/BM25 indexing for files, entities, and snippets
- exact symbol lookup in local graph storage
- Stage 0 FTS population during `codegraph-mcp index <repo>`

Phase 11 adds:

- `BinaryVectorIndex` trait
- local in-memory bit-packed binary index
- deterministic text-hash binary signature generation placeholder
- Hamming distance via XOR + popcount
- top-k binary sieve retrieval
- exact-seed union so Stage 0 seeds cannot be dropped by Stage 1 filtering
- feature-gated FAISS/Qdrant adapter placeholders without required services

Phase 12 adds:

- `CompressedVectorReranker` trait
- `RerankCandidate`, `RerankQuery`, and `RerankScore` types
- deterministic local compressed reranker
- placeholder `Int8Vector`, `ProductQuantizedVector`, and
  `MatryoshkaVectorView` types
- Matryoshka prefix validation for 32, 64, 128, and 256 dimensions
- exact-seed boosting/preservation in rerank results
- feature-gated FAISS/Qdrant reranker adapter placeholders without required
  services

Phase 13 adds:

- `RetrievalFunnel` orchestration layer
- Stage 0 exact seed and candidate text merge
- Stage 1 binary sieve candidate expansion
- Stage 2 deterministic compressed rerank narrowing
- Stage 3 exact graph verification using `ExactGraphQueryEngine`
- Stage 4 compact context packet emission using the Phase 09 packet builder
- structured kept/dropped trace output for every stage
- final packet membership controlled by graph/source verification, with
  heuristic evidence explicitly labeled

Phase 14 adds:

- deterministic Bayesian/logistic scoring over graph-verified funnel paths
- the full ranking feature set from the project contract
- configurable weights and relation reliability priors
- uncertainty penalties from edge exactness, confidence, and relation priors
- context packet confidence/uncertainty metadata
- calibration placeholders for Brier score and reliability buckets
- SQLite retrieval trace persistence through `GraphStore`

Phase 15 adds:

- `codegraph-mcp serve-mcp`
- local stdio JSON-RPC MCP handling for initialize, tools/list, and tools/call
- all required `codegraph.*` MCP tool names from the project contract
- input validation and structured JSON tool responses
- proof-oriented outputs for symbols, text, context packets, relation paths,
  impact analysis, edge explanations, and path explanations
- local index update tools only for `.codegraph/codegraph.sqlite`
- `.codex/config.toml.example` template

Phase 15 does not create destructive MCP tools, UI runtime, benchmarks, skills,
hooks, watcher logic, RocksDB, RL/autonomous optimization, or advanced research
prototypes.

Phase 16 adds:

- `codegraph-mcp init` setup with dry-run and safe optional templates
- direct CLI status, symbol query, path query, context-pack, and impact commands
- an evidence-oriented impact dashboard across calls, mutations/dataflow, DB
  schema, APIs/auth/security, events/messages, and tests
- `.cgc-bundle` JSON export/import with manifest schema version validation

Phase 17 adds:

- checked-in Codex Skill templates under `templates/skills`
- hook templates for `SessionStart`, `UserPromptSubmit`, `PreToolUse`,
  `PostToolUse`, and `Stop`
- a short `AGENTS.md` template
- `codegraph-mcp init --with-templates` installation into `AGENTS.md`,
  `.codex/skills`, and `.codex/hooks`
- tests that generated templates include project guardrails, no-subagents rules,
  syntactically valid hook config, and skill metadata

Phase 18 adds:

- `codegraph-mcp watch [repo]` using Rust `notify`
- debounce logic for filesystem save bursts
- ignore rules for `.git`, `.codegraph`, dependency folders, and build outputs
- localized changed-file re-indexing rather than full repo rebuilds
- stale entity, edge, span, and Stage 0 text removal before re-insertion
- binary signature refresh for updated entities
- in-memory exact graph adjacency refresh for watcher state
- concise watcher status logs

Phase 19 adds:

- `codegraph-mcp serve-ui` loopback HTTP server
- bundled static UI assets under `codegraph-ui/static`
- a vendored local D3.js SVG graph renderer with no CDN dependency
- path graph, edge detail, source span, relation filter, impact dashboard, and
  context packet preview views
- local API endpoints for status, path graph JSON, impact, and context packets
- structural tests for server startup, graph JSON, relation filters, and
  context packet preview

Phase 20 adds:

- benchmark task and ground-truth schemas
- synthetic controlled repositories for relation extraction, long-chain path,
  context retrieval, agent patch, compression, security/auth, async/event, and
  test-impact families
- baseline modes for vanilla, BM25, vector-only, graph-only, graph+binary/PQ,
  graph+Bayesian, and full context packet comparison
- precision/recall/F1, recall@k, MRR, NDCG, token, latency, memory, and
  patch/test success metrics
- real-repo commit replay planning when a local git checkout is available
- JSON and Markdown report generation through `codegraph-mcp bench`

Phase 21 adds:

- full workspace test, benchmark, CLI, MCP, watcher, UI, and bundle acceptance
  validation
- index rebuild hardening with one transaction-level indexed-fact clear
- latency profile notes for indexing, query, path tracing, and context packets
- final quickstart, architecture, CLI, MCP, benchmark, troubleshooting, and
  acceptance docs

Phase 21 does not create RL/autonomous optimization, destructive MCP tools,
post-MVP dynamic tracing, SMT/Z3 verification, or LSP-daemon memory overlays.

Phase 27 adds:

- tiered language frontend registry
- `codegraph-mcp languages` table and JSON output
- Python, Go, Rust, Java, C#, C/C++, Ruby, and PHP parser frontends
- optional TypeScript Compiler API resolver hooks
- explicit capability metadata and exactness labels

Phase 28 adds:

- hardened symbol/text/file/reference/definition search
- caller, callee, call-chain, and unresolved-call query commands
- conservative parser-level call extraction for Python, Go, and Rust
- explicit heuristic labels for unresolved calls
- evidence-backed test impact model and minimal test selection

Phase 29 adds:

- `index --profile --json` with timing, throughput, skip, worker, and memory
  fields where measurable
- content-hash skip for unchanged files
- deterministic parallel parse/extract workers
- batched SQLite writes and documented WAL/synchronous/busy-timeout pragmas
- global CLI flags, stable command aliases, `doctor`, shell completions, and
  release metadata output
- synthetic indexing-speed benchmark generation
- dry-run installer templates, archive manifest, Homebrew formula template,
  cargo-binstall metadata template, and release packaging workflow template

The current public baseline also adds:

- proof/neighborhood/impact/auth/event/test/unresolved-call UI graph modes
- local D3 layered path graph metadata, exactness legend, source-span preview,
  path comparison, graph JSON export, and copyable context packet output
- visible node caps, server-side filtering metadata, expand-on-click metadata,
  and explicit truncation warnings
- MCP output schemas, read-only/local-only annotations, pagination, resources,
  prompt templates, source-span/file resource links, and `explain_missing`
- pinned TypeScript, Python, Go, Rust, and Java real-repo maturity corpus
- offline replay plans into ignored `.codegraph-bench-cache/real-repos`
- final parity report artifacts with unknown/skipped fields and no fabricated
  SOTA claims

The public baseline does not start advanced post-baseline research work.

## Required Product Stance

- Rust-first.
- Exact graph first.
- Vectors second.
- Local storage first.
- Evidence and source spans over model memory.
- Visible uncertainty over fake certainty.
- No subagents; single-agent workflow only.

## Retrieval Funnel Contract

The final MVP funnel is:

```text
Stage 0 exact seeds
  -> Stage 1 binary sieve
  -> Stage 2 compressed rerank
  -> Stage 3 exact graph verification
  -> Stage 4 compact context packet
```

Exact seeds cannot be dropped by vector stages. Vector stages are candidate
reducers, not truth sources. Exact graph verification controls final evidence.
