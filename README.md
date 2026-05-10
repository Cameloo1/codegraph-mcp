# CodeGraph MCP - WIP, Not published here yet.

CodeGraph MCP is a local, Rust-first codebase intelligence layer for AI coding
agents. It is designed to give models fast, proof-oriented context so they stop
guessing about APIs, schemas, call paths, data flow, tests, and repository
structure.

The project indexes a repository into a deterministic typed program graph, ranks
candidate context quickly, verifies facts against exact graph and source-span
evidence, and exposes the result through a CLI, a local MCP server, and a
loopback Proof-Path UI.

```text
repository
  -> typed program graph
  -> fast retrieval funnel
  -> exact graph/source verification
  -> compact evidence packet
  -> grounded model edits
```

## Status

CodeGraph MCP is under active private development and is not public yet. Message
me if you want to help.

This checkout is complete through Phase 30 of the project plan. Phase 31+
research work remains intentionally untouched unless benchmark evidence justifies
it. The authoritative product contract, phase order, schema definitions, and
acceptance criteria live in [MVP.md](MVP.md).

## Why It Exists

Coding models are strongest when they can rely on compact, current, verifiable
project memory. They are weakest when they have to infer schemas, relations, or
call chains from partial text search results.

CodeGraph MCP is built around one rule:

```text
vectors suggest; graph verifies; packets prove
```

Embeddings, binary signatures, BM25, relation priors, and Bayesian ranking can
help find candidates quickly. They do not prove correctness. Final context must
come from graph facts, source spans, exactness labels, confidence metadata, and
provenance.

## Core Capabilities

- Local repository indexing into `.codegraph/codegraph.sqlite`.
- Stable entity, relation, source-span, provenance, exactness, and context
  packet models.
- Tree-sitter extraction for TypeScript, JavaScript, TSX, JSX, Python, Go,
  Rust, Java, C#, C, C++, Ruby, and PHP, with explicit support tiers.
- Conservative caller/callee, dataflow, mutation, auth/security, event,
  persistence, migration, and test-impact relations where extractor support
  exists.
- SQLite FTS5/BM25 search across files, entities, and snippets.
- Stage 1 bit-packed binary-vector sieve and Stage 2 compressed rerank
  interface for fast candidate narrowing.
- Stage 3 exact graph/path verification before context packet emission.
- Deterministic Bayesian/logistic ranking with uncertainty penalties and
  relation reliability priors.
- Read-mostly local MCP tools for status, indexing, search, impact analysis,
  path tracing, caller/callee lookup, context packs, and edge/path explanation.
- Local Proof-Path UI with graph modes, source-span previews, exactness legends,
  export/copy controls, and large-graph guardrails.
- Reproducible benchmark, audit, parity, and CodeGraphContext comparison
  harnesses that keep skipped or unknown data explicit.

## Quickstart

Build and test:

```powershell
cargo build --workspace
cargo test --workspace
```

Initialize and index a repository:

```powershell
codegraph-mcp init --with-templates --with-codex-config
codegraph-mcp index .
codegraph-mcp status
```

Query evidence:

```powershell
codegraph-mcp query symbols profileRoute
codegraph-mcp query callers saveProfile
codegraph-mcp query chain profileRoute saveProfile
codegraph-mcp impact profileRoute
codegraph-mcp context-pack --task "Trace profileRoute auth and mutation impact" --seed profileRoute --budget 1600
```

Serve the local MCP server:

```powershell
codegraph-mcp serve-mcp
```

Serve the loopback Proof-Path UI:

```powershell
codegraph-mcp serve-ui --port 7878
```

Run benchmarks:

```powershell
codegraph-mcp bench --output target\codegraph-benchmark-report.json
codegraph-mcp bench parity-report --output-dir target\phase30-parity
```

For the full command set, see [docs/cli-reference.md](docs/cli-reference.md).

## Architecture

CodeGraph is intentionally split into narrow layers:

| Layer | Responsibility |
| --- | --- |
| `codegraph-core` | Serializable graph domain model: entities, relations, spans, exactness, provenance, stable IDs, path evidence, and context packets. |
| `codegraph-store` | Local graph persistence through SQLite, migrations, FTS/BM25, read helpers, and retrieval traces. |
| `codegraph-parser` | Language parsing and extractor logic for syntax, declarations, imports, calls, data/mutation facts, auth/security, events, persistence, and tests. |
| `codegraph-vector` | Stage 1 binary-vector sieve and Stage 2 compressed rerank abstractions. |
| `codegraph-query` | Exact graph traversal, impact analysis, path evidence, context packets, retrieval funnel orchestration, and Bayesian ranking. |
| `codegraph-index` | Shared compact repository indexer used by the CLI and MCP server, including batching, dedupe, stale pruning, profiling, and binary-signature refresh. |
| `codegraph-mcp-server` | Local stdio JSON-RPC MCP server with schemas, resources, prompts, pagination, and read-mostly proof tools. |
| `codegraph-cli` | `codegraph-mcp` CLI, templates, watcher, UI server, diagnostics, benchmarks, config, and release metadata. |
| `codegraph-ui` | Static assets for the local Proof-Path UI served by the CLI. |
| `codegraph-bench` | Benchmark schemas, fixtures, baselines, metrics, real-repo corpus manifests, replay plans, and parity reports. |
| `codegraph-trace` | Agent/MCP trace event support for replayable evidence. |

The retrieval funnel is:

```text
Stage 0: exact seeds, symbols, BM25, stack traces, current files
Stage 1: bit-packed binary sieve
Stage 2: compressed rerank interface
Stage 3: exact graph/path verification
Stage 4: compact proof-oriented context packet
```

Exact seeds bypass vector filtering. Heuristic facts must remain explicitly
labeled with exactness, confidence, extractor metadata, and provenance.

## Deep Documentation

| Document | Purpose |
| --- | --- |
| [MVP.md](MVP.md) | Authoritative mission, schemas, phase order, non-goals, acceptance criteria, and implementation prompt queue. |
| [TODO_PHASES.md](TODO_PHASES.md) | Compact phase checklist for the MVP and post-MVP sequence. |
| [docs/architecture.md](docs/architecture.md) | Final MVP architecture summary, major layers, source-of-truth rules, and phase ordering. |
| [docs/guardrails.md](docs/guardrails.md) | Implementation boundaries, product stance, and retrieval funnel contract. |
| [docs/mcp-reference.md](docs/mcp-reference.md) | MCP tools, resources, prompts, schemas, output contract, and safety model. |
| [docs/cli-reference.md](docs/cli-reference.md) | CLI commands, global flags, output behavior, SQLite tuning, and installability notes. |
| [docs/language-frontends.md](docs/language-frontends.md) | Language support tiers, current frontend coverage, exactness rules, and limitations. |
| [docs/benchmark-guide.md](docs/benchmark-guide.md) | Benchmark families, baselines, metrics, reports, real-repo corpus, and CGC comparison flow. |
| [docs/codegraphcontext-comparison.md](docs/codegraphcontext-comparison.md) | Black-box CodeGraphContext comparison setup, fairness rules, fixtures, and outputs. |
| [docs/mvp-acceptance.md](docs/mvp-acceptance.md) | MVP acceptance checklist, profile report expectations, and known non-blocking notes. |
| [docs/quality-gates.md](docs/quality-gates.md) | Local checks, CI expectations, smoke coverage, and Phase 30 acceptance commands. |
| [docs/quickstart.md](docs/quickstart.md) | Short build, index, query, MCP, watcher, UI, and benchmark walkthrough. |
| [docs/install.md](docs/install.md) | Install paths, dry runs, release metadata, and distribution targets. |
| [docs/troubleshooting.md](docs/troubleshooting.md) | Common indexing, SQLite, UI, MCP, watcher, and benchmark issues. |
| [reports/audit/README.md](reports/audit/README.md) | Audit report contract for benchmark validity, schema taxonomy, storage forensics, relation samples, stale updates, and source-span proof gates. |
| [reports/audit/AUDIT_STATUS.md](reports/audit/AUDIT_STATUS.md) | Current audit phase status. |

## Safety And Scope

- Local first: graph state is written under `.codegraph/`.
- Read-mostly MCP: source-editing and destructive tools are not exposed.
- Exact graph first: retrieval shortcuts cannot prove facts by themselves.
- Single-agent workflow: the product is designed for one linear Codex-style
  coding agent, not parallel subagent delegation.
- Honest measurement: unsupported, skipped, or unavailable benchmark and
  competitor data stays `unknown` or `skipped`.
- No public-release claims yet: install, release, and parity artifacts are being
  developed and verified before public launch.

## Development Notes

The workspace is a Rust Cargo workspace with `unsafe_code = "forbid"` and
workspace lints for `dbg_macro`, `todo`, and `unwrap_used`. Use
[docs/quality-gates.md](docs/quality-gates.md) before publishing changes, and
start architecture-affecting work by reading [MVP.md](MVP.md).
