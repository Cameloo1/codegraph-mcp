# Architecture Notes

These notes summarize the public CodeGraph architecture and current hardening
state.

## Mission

CodeGraph provides compact, verified codebase context for one Codex-style
coding agent. It does this by building a deterministic typed program graph,
using compressed retrieval only to suggest candidates, and requiring exact
graph/source verification before producing a context packet.

## Source Of Truth

The source of truth is the local graph plus source spans. Embeddings, binary
signatures, relation priors, Bayesian scores, and rerankers can help rank or
compress candidates, but they do not prove facts.

Core rule:

```text
vectors suggest; graph verifies; packet proves
```

## Major Layers

1. `codegraph-core` defines the serializable domain model for entities,
   relations, source spans, provenance, exactness, path evidence, derived edges,
   context packets, stable IDs, and broad endpoint validation.
2. `codegraph-store` persists exact graph facts locally behind `GraphStore`,
   with SQLite via `rusqlite` implemented first.
3. `codegraph-parser` parses source into syntax metadata and now extracts
   TypeScript/JavaScript file/module/declaration/import/export facts,
   conservative core static execution/data/mutation relations, best-effort
   heuristic auth/security/event/db/test relations, broader language frontend
   facts, and conservative Python/Go/Rust parser-level calls.
4. `codegraph-query` verifies edges and relation paths over exact graph facts,
   extracts Stage 0 prompt seeds, converts paths into PathEvidence, derives
   explainable closure edges, builds graph-only context packets, orchestrates
   the integrated runtime funnel while preserving exact seeds, and applies a
   deterministic Bayesian/logistic ranker with uncertainty metadata after graph
   verification. It also owns the ranked `SymbolSearchIndex` used by the CLI.
5. `codegraph-vector` implements the Stage 1 local binary-vector sieve and the
   Stage 2 compressed rerank interface with deterministic local reranking,
   int8/PQ/Matryoshka placeholder vectors, and optional backend stubs.
6. `codegraph-mcp-server` exposes read-mostly evidence tools to Codex through
   a local stdio JSON-RPC MCP server with input/output schemas, safety
   annotations, resources, prompt templates, pagination, resource links, and
   explain-missing output.
7. `codegraph-cli` provides local commands for indexing, status, querying,
   impact analysis, context packs, bundles, MCP serving, UI serving, Codex
   template installation, optional live watching, the local Proof-Path UI HTTP
   server, local benchmark execution, hardened caller/callee/chain query
   commands, diagnostics, shell completions, release metadata, and profiled
   indexing-speed fixtures, real-repo corpus manifests, and final parity report
   generation.
8. `codegraph-bench` validates extraction, retrieval, compression, path recall,
   security/auth, async/event flow, test-impact, and agent-patch outcomes with
   reproducible synthetic repos, baseline modes, metrics, replay plans, optional
   black-box CodeGraphContext comparison, pinned real-repo maturity corpus, and
   reports.
9. `codegraph-ui` contains bundled static assets for the local Proof-Path UI
   served by `codegraph-mcp serve-ui`, including proof/neighborhood/impact
   graph modes, exactness legends, source-span preview, export/copy controls,
   and large-graph guardrails.

## Phase Ordering

The graph/domain model comes before storage. Storage comes before parsing.
Parsing comes before extraction. Exact graph queries and graph-only context
packets come before Stage 0 BM25/FTS. Stage 0 comes before the binary-vector
sieve. Stage 1 comes before compressed reranking. The integrated runtime funnel
comes before Bayesian calibration. Bayesian calibration comes before MCP. MCP
comes before CLI completion. CLI completion comes before Skills, Hooks, and
`AGENTS.md` templates. Templates come before live watching. Watching comes
before the local Proof-Path UI. The UI comes before benchmarks. Benchmarks,
language breadth, query hardening, indexing speed, installability, UI/MCP
ergonomics, and real-repo parity reports stay evidence-gated. Future research
proposals remain deferred until benchmark evidence shows remaining gaps.

Post-MVP features such as eBPF/OpenTelemetry tracing, SMT/Z3 verification, LSP
memory-buffer overlays, and reinforcement-learning-style optimization are not
started until the full MVP is accepted.

## Single-Agent Workflow

No subagents. The product behavior, docs, skills, hooks, and implementation
prompts must preserve one linear Codex-style agent workflow. Internal Rust code
may use deterministic parallelism for indexing/query execution, but agent
reasoning and implementation stay single-agent only.
