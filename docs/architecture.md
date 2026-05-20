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

Current runtime shape:

```text
task/query
  -> lifecycle preflight
  -> prompt intent + exact seeds
  -> Stage 0 text evidence and lexical candidates
  -> optional vector / binary / nuance-rescue candidates
  -> graph-neighborhood and PathEvidence candidates
  -> union, dedup, rank
  -> bounded graph/source verification
  -> compact packet or no-proof source-text fallback
```

The candidate merge preserves source labels, matched seeds, evidence roles,
proof status, and candidate provenance. It does not prove anything by itself.

## Current Evidence Boundary

The current public evidence is in the stable report summaries, not in raw run
payloads:

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

Raw DBs, WAL/SHM files, raw logs, diagnostic payloads, and temporary competitor
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
   the integrated runtime funnel while preserving exact seeds, handles
   candidate provenance for exact/text/lexical/vector/binary/nuance/graph/path
   lanes, and applies deterministic ranking with uncertainty metadata after
   graph verification. It also owns the ranked `SymbolSearchIndex` used by the
   CLI.
5. `codegraph-vector` implements the Stage 1 local binary-vector sieve and the
   Stage 2 compressed rerank interface with deterministic local reranking,
   int8/PQ/Matryoshka placeholder vectors, and optional backend stubs.
6. `codegraph-mcp-server` exposes read-mostly evidence tools through a local
   stdio JSON-RPC MCP server with input/output schemas, safety
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

Use the profiles in [operational-profiles.md](operational-profiles.md) to keep
temporary development or benchmark DBs separate from the graph used by a coding
agent:

- The development profile uses local diagnostic DBs for testing CodeGraph
  changes.
- The agent-use profile uses a release binary and a DB outside the source tree
  for routine coding-agent context.

Agent-use reads should answer only after the DB lifecycle preflight says the DB
is valid, matching, claimable, and not contaminated. Development and benchmark
DBs are never superiority evidence by themselves.

## Roadmap Boundary

The current architecture is deliberately local, deterministic, and
proof-first. Future research features such as dynamic tracing, solver-backed
verification, LSP memory-buffer overlays, or learned graph priors should remain
evidence-gated additions. They should not weaken the runtime contract that
vectors suggest, the graph verifies, and source spans support final context.

Stage 0 text evidence is part of the current runtime for scoped non-parser
planning files such as Makefiles, Kconfig/Config.in, docs, and support scripts.
It is source-text evidence only, not typed graph proof.

Vector, binary, and nuance-rescue lanes are opt-in or bounded candidate recall
surfaces. They can route attention, but graph/source verification still decides
whether a packet is proof, fallback text evidence, or unknown.

## Agent Workflow

The product is designed for a single linear coding-agent workflow. Internal
Rust code may use deterministic parallelism for indexing and query execution,
but the exposed context contract should remain inspectable and easy to audit.

Agent-facing packets should be compact by default and may include bounded
planning data such as source roles, risks, validation hints, and
`follow_up_queries`. Those fields are meant to guide the next inspection step;
they are not shell commands and are not automatic tool execution.
