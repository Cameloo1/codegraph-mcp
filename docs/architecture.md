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

## Current Evidence Boundary

The current public evidence is in the stable report summaries, not in raw
development artifacts:

- `reports/final/comprehensive_benchmark_latest.md` / `.json` preserves the
  latest comprehensive gate. It is currently **fail** because
  `bytes_per_proof_edge`, `cold_proof_build_total_wall_ms`, and
  `proof_db_mib_stretch` miss target.
- `reports/final/intended_tool_quality_gate.md` / `.json` is currently
  **FAIL** because `proof_build_only_ms = 184,297 ms` exceeds the
  `<=60,000 ms` target.
- `reports/final/manual_relation_precision.md` / `.json` reports 320 labeled
  samples for present compact-proof relations. It is sampled precision only;
  recall is unknown.
- `reports/comparison/codegraph_vs_cgc_latest.md` / `.json` is incomplete and
  diagnostic. CGC recovered enough for smoke/fixture diagnostics, but the
  comparable run did not complete, so no CodeGraph superiority claim is made.

Raw DBs, WAL/SHM files, raw logs, diagnostic lab payloads, and temporary CGC
artifacts are evidence inputs, not public architecture claims.

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

## Operational Profiles

Use the two profiles in [operational-profiles.md](operational-profiles.md) to
keep CodeGraph's own development state separate from the graph used by a coding
agent:

- `DEVELOPMENT_SELF_TEST` uses the debug binary and repo-local diagnostic DBs
  for testing CodeGraph changes.
- `PRODUCTION_AGENT_USE` uses the release binary and a DB outside the source
  tree under LocalAppData for routine Codex context.

The production profile is allowed to answer agent-context questions only after
the DB lifecycle preflight says the DB is valid, matching, claimable, and not
contaminated. Development/self-test DBs are never benchmark or superiority
evidence by themselves.

## Phase Ordering

This section is a historical implementation map, not the current pass/fail
report. The graph/domain model comes before storage. Storage comes before parsing.
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
