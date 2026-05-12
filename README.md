# CodeGraph MCP

Local code-graph evidence layer for AI coding agents.

CodeGraph MCP indexes a repository into a deterministic typed program graph, verifies facts against source spans and provenance, and returns compact context packets that a coding agent can actually trust.

```text
repository
  -> typed program graph
  -> exact graph/source verification
  -> compact evidence packet
  -> grounded model edits
```

## Why It Exists

Coding agents are strongest when they have current, compact, verifiable project memory. They are weakest when they have to infer APIs, schemas, call paths, data flow, tests, or security behavior from scattered text search results.

Embeddings, BM25, binary signatures, and ranking help find candidates quickly. They do not prove correctness. Final context should come from graph facts, exactness labels, source spans, provenance, and stored path evidence.

## What It Does

- Indexes a local repository into a typed program graph.
- Tracks entities, calls, reads, writes, flows, mutations, tests, mocks, assertions, auth checks, source spans, and provenance where extractor support exists.
- Separates exact proof facts from heuristic/debug evidence.
- Builds context packets around verified paths instead of shallow text retrieval.
- Exposes the graph through a CLI, local MCP server, and loopback Proof-Path UI.

## Current Evidence

The latest comprehensive benchmark publishes 14 gates, with 11 passing cleanly and 2 known open performance gates.

| Gate | Current result | Status |
|---|---|---|
| Graph Truth fixtures | 11 / 11 passed | pass |
| Context Packet fixtures | 11 / 11 passed | pass |
| Forbidden edge/path hits | 0 | pass |
| Proof source-span coverage | 100% | pass |
| Exact-labeled unresolved facts | 0 | pass |
| Derived facts without provenance | 0 | pass |
| Test/mock production leakage | 0 | pass |
| DB integrity | ok | pass |
| Repeat unchanged index | 1.674s | pass |
| Single-file update | 336ms | pass |
| context_pack p95 | 852ms | pass |
| Unresolved-calls page p95 | 243ms | pass |
| Proof DB size | 320.63 MiB vs 250 MiB target | open |
| Cold proof build | 50.02 min vs 60s target | open |
| CodeGraphContext comparison | CGC timed out | incomplete |

Storage size and cold proof-build time are the two open performance gates. Both are addressable — storage through compact-proof-DB work, cold build through extractor parallelization. Correctness, context quality, latency on warm operations, and update performance all pass.

## CodeGraph vs CodeGraphContext

A fair comparison needs both systems to complete comparable indexing and query artifacts.

| Comparison item | Result |
|---|---|
| CGC available | yes, version 0.4.7 |
| CGC completed current comparable run | no |
| CGC timeout | yes |
| CodeGraph vs CGC speed | unknown |
| CodeGraph vs CGC storage | unknown |
| CodeGraph vs CGC quality | unknown |
| Verdict | incomplete |

CGC timed out on the comparable indexing run at version 0.4.7. A clean head-to-head requires CGC to complete; until then this isn't reported as a CodeGraph win. CodeGraph internal gates are tracked separately from competitor claims — a timeout, skipped run, partial DB, or fake-agent dry run is not counted as superiority evidence.

## Quickstart

Build:

```bash
cargo build --workspace
```

Index a repository:

```bash
codegraph-mcp index .
```

Query evidence:

```bash
codegraph-mcp context-pack --task "Trace auth and mutation impact" --seed profileRoute --budget 1600
```

For the full CLI surface, see `cli-reference.md`.

## Architecture

CodeGraph is split into three practical layers:

- **Extract and store:** parse source files, assign stable identities, record source spans, and persist exact/heuristic facts in SQLite.
- **Verify and rank:** use retrieval only to suggest candidates, then verify graph paths, exactness, provenance, and production/test/mock context.
- **Package evidence:** return compact context packets with proof paths, snippets, expected tests, and labels a coding agent can use.

The retrieval funnel is:

```text
exact seeds, symbols, BM25, current files
  -> binary/vector candidate narrowing
  -> compressed rerank
  -> exact graph/path verification
  -> proof-oriented context packet
```

## Language Support

Tree-sitter extraction across 13 languages including TS/JS, Python, Go, Rust, Java, C/C++, C#, Ruby, and PHP, with relation support varying by language and extractor.

## Interfaces

- `codegraph-mcp index` builds the local graph.
- `codegraph-mcp query ...` searches symbols, text, relations, paths, callers, callees, impact, and unresolved calls.
- `codegraph-mcp context-pack ...` emits agent-facing proof context.
- `codegraph-mcp serve-mcp` exposes local read-mostly MCP tools.
- `codegraph-mcp serve-ui` opens the local Proof-Path UI.
- `codegraph-mcp bench comprehensive` writes the master correctness, context, storage, latency, update, and comparison gate.

## Reports

Benchmark and audit artifacts

- `comprehensive_benchmark_latest.md` — current master gate.
- `compact_proof_db_gate.md` — compact proof DB gate.
- `CODEGRAPH_VS_CGC_LATEST.md` — latest CGC comparison status.
- `architecture.md` — architecture details.
- `benchmark-guide.md` — benchmark workflow.
- `mcp-reference.md` — MCP tools and schemas.
- `language-frontends.md` — language frontend coverage.
