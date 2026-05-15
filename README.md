# CodeGraph MCP

[![CI](https://img.shields.io/github/actions/workflow/status/Cameloo1/codegraph-mcp/ci.yml?branch=main&style=flat-square&label=CI)](https://github.com/Cameloo1/codegraph-mcp/actions/workflows/ci.yml)
[![Primary Language](https://img.shields.io/github/languages/top/Cameloo1/codegraph-mcp?style=flat-square&label=primary%20language)](https://github.com/Cameloo1/codegraph-mcp)
[![MCP](https://img.shields.io/badge/MCP-local%20read--mostly-informational?style=flat-square)](docs/mcp-reference.md)

**Local proof-grounded context for AI coding agents on large repositories. Vectors suggest, the typed program graph proves, source spans verify.**

CodeGraph MCP indexes a repository into a deterministic typed program graph,
verifies facts against source spans and provenance, and returns compact evidence
packets that a coding agent can actually trust. Graph facts carry source spans,
file hashes, extractors, provenance where available, and exactness labels from
compiler-verified down to static-heuristic.

- **Exact graph is the source of truth.** Embeddings route; the graph proves;
  tests validate.
- **5-stage compressed funnel built for large repos.** Exact seeds -> 1-bit
  sieve -> compressed rerank -> typed-path verification -> minimal context
  packet.
- **Read-mostly, local-first.** SQLite under `.codegraph/`, no managed service,
  no data leaves the machine.

```text
repository
  -> typed program graph (entities + typed relations + source spans + exactness)
  -> compressed retrieval funnel (exact seeds -> binary sieve -> compressed rerank)
  -> exact graph/path verification
  -> compact evidence packet
  -> grounded model edits
```

## Agent-Impact Benchmarks

| Large-Repo Improvement | Evidence Reliability | Warm Agent Loop |
|---|---|---|
| ![Large-Repo Improvement](docs/assets/readme/agent_visual_01_context_quality.png) | ![Evidence Reliability](docs/assets/readme/agent_visual_02_trusted_relations.png) | ![Warm Agent Loop](docs/assets/readme/agent_visual_03_agent_loop_readiness.png) |

Current status: semantic-proof and context-packet gates are green, compact-proof
storage is under the intended 250 MiB target, and stale or mismatched DB reuse is
guarded by DB passport preflight. The published Intended Tool Quality Gate is
still `FAIL` because the stable report records proof-build timing over target;
CGC comparison remains diagnostic/incomplete, with no superiority claim.

See: [Intended Tool Quality Gate](reports/final/intended_tool_quality_gate.md)
and [Manual Relation Precision](reports/final/manual_relation_precision.md).

## Why It Exists

Coding agents are strongest when they have current, compact, verifiable project
memory. They are weakest when they have to infer APIs, schemas, call paths, data
flow, tests, or security behavior from scattered text search results.

Embeddings, BM25, binary signatures, and ranking help find candidates quickly.
They do not prove correctness. Final context should come from graph facts,
exactness labels, source spans, provenance, and stored path evidence, not from
"top-k similar chunks."

## Quickstart

Build:

```bash
cargo build --workspace
```

Index this repository from a checkout:

```bash
cargo run --bin codegraph-mcp -- index .
```

Query evidence with a symbol that exists in this repo:

```bash
cargo run --bin codegraph-mcp -- query symbols index_repo_to_db
cargo run --bin codegraph-mcp -- context-pack \
    --task "Trace indexing entry point" \
    --seed index_repo_to_db \
    --budget 1600
```

If `codegraph-mcp` is on your `PATH`, drop the
`cargo run --bin codegraph-mcp --` prefix. Full CLI surface:
[docs/cli-reference.md](docs/cli-reference.md).

## How It Works

CodeGraph is a 5-stage runtime funnel sitting on top of a typed program graph.
Each stage does one specific job; downstream stages cannot fabricate facts not
present upstream.

### 1. Parse -> Typed Program Graph

Tree-sitter parses 11 language families through 13 frontends: JavaScript, JSX,
TypeScript, TSX, Python, Go, Rust, Java, C#, C, C++, Ruby, and PHP. Each AST
construct becomes a typed entity such as `Function`, `Method`, `Class`,
`Interface`, `Field`, `CallSite`, `ReturnSite`, `Route`, `Middleware`,
`AuthPolicy`, `Migration`, `TestCase`, `Mock`, or `ConfigKey`.

The graph model currently defines 55 entity kinds, 67 relation kinds, and 8
exactness labels in [crates/codegraph-core/src/kinds.rs](crates/codegraph-core/src/kinds.rs).
Relation coverage varies by language and extractor, and unsupported proof-mode
relations do not receive precision claims.

Relation groups include:

- **Structural:** `CONTAINS`, `DEFINED_IN`, `IMPORTS`, `EXPORTS`,
  `BELONGS_TO`, `CONFIGURES`
- **Type/object:** `TYPE_OF`, `RETURNS`, `IMPLEMENTS`, `EXTENDS`,
  `OVERRIDES`, `INSTANTIATES`, `INJECTS`
- **Execution:** `CALLS`, `CALLED_BY`, `CALLEE`, `ARGUMENT_0`, `ARGUMENT_1`,
  `ARGUMENT_N`, `RETURNS_TO`, `SPAWNS`, `AWAITS`, `LISTENS_TO`
- **Data flow:** `READS`, `WRITES`, `MUTATES`, `FLOWS_TO`, `REACHING_DEF`,
  `CONTROL_DEPENDS_ON`, `DATA_DEPENDS_ON`
- **Security:** `AUTHORIZES`, `CHECKS_ROLE`, `CHECKS_PERMISSION`,
  `SANITIZES`, `VALIDATES`, `EXPOSES`, `TRUST_BOUNDARY`, `SOURCE_OF_TAINT`,
  `SINKS_TO`
- **Async/event:** `PUBLISHES`, `EMITS`, `CONSUMES`, `SUBSCRIBES_TO`,
  `HANDLES`
- **Persistence:** `MIGRATES`, `READS_TABLE`, `WRITES_TABLE`,
  `ALTERS_COLUMN`, `DEPENDS_ON_SCHEMA`
- **Testing:** `TESTS`, `ASSERTS`, `MOCKS`, `STUBS`, `COVERS`,
  `FIXTURES_FOR`
- **Derived, with provenance:** `MAY_MUTATE`, `MAY_READ`, `API_REACHES`,
  `ASYNC_REACHES`, `SCHEMA_IMPACT`

Exactness labels include `exact`, `compiler_verified`, `lsp_verified`,
`parser_verified`, `static_heuristic`, `dynamic_trace`, `inferred`, and
`derived_from_verified_edges`.

### 2. Stage 0 - Exact Seeds

Before vector retrieval runs, exact signals are extracted and pinned: symbol
names, file paths, stack-trace frames, failing test names, current open file,
and BM25/FTS5 matches over source. For code tasks, false negatives at the
retrieval layer are expensive, so exact seeds are unioned with candidate
retrieval rather than intersected away.

### 3. Stage 1 - 1-Bit Binary Sieve

Each indexed entity can carry a deterministic bit-packed signature. Stage 1
reduces large candidate sets via Hamming distance:

```text
sim(x, y) = d - 2 * popcount(x XOR y)
```

This is a cheap narrowing pass: XOR, popcount, no floating point, no full-vector
decompression.

### 4. Stage 2 - Compressed Rerank

Surviving candidates are rescored against the query in compressed forms:

- **int8 scalar quantization:** compact vector storage with a scale factor.
- **Product Quantization (PQ):** subvector codebooks with compact u8 codes.
- **Matryoshka prefixes:** one embedding usable at multiple prefix dimensions.

The reranker is deterministic: same query and same index commit produce the same
ranking. Exact seeds, text scores, compressed-vector scores, and uncertainty can
be combined, but the result is still only a candidate set.

### 5. Stage 3 - Exact Graph Verification

The top candidates are not treated as answers. Stage 3 walks the typed program
graph between seed entities and candidates:

- bounded BFS/DFS with relation filters
- weighted path search over relation costs
- k-shortest paths for multiple proof routes
- relation-pattern queries for dataflow and impact questions
- derived closure edges that retain provenance to base edges

Every retained step carries source-span and exactness evidence. Heuristic edges
can participate, but the packet labels the evidence accordingly.

### 6. Stage 4 - Compact Context Packet

Verified paths are assembled into a `PathEvidence` packet: the smallest useful
set of source spans, relation paths, recommended tests, and risk notes that
supports the agent's task. Redundant spans are deduplicated, packet size is
budgeted, and heuristic-only evidence is labeled separately.

A packet shape looks like:

```json
{
  "task": "Change User.email normalization without breaking auth",
  "verified_paths": [
    {
      "summary": "User.email flows into token subject during login",
      "edges": [
        ["User.email", "READS", "normalizeEmail"],
        ["normalizeEmail", "CALLED_BY", "AuthService.login"],
        ["AuthService.login", "WRITES", "TokenPayload.sub"],
        ["TokenPayload.sub", "ASSERTED_BY", "auth.spec.ts"]
      ],
      "source_spans": [
        "src/user.ts:37-45",
        "src/auth.ts:82-101",
        "tests/auth.spec.ts:44-61"
      ],
      "exactness": "verified_static_graph"
    }
  ],
  "risks": ["Changing normalization can break token.sub assertion."],
  "recommended_tests": ["npm test -- auth.spec.ts"]
}
```

## Why Not Just Embeddings

A pure vector pipeline (`text -> embedding -> cosine -> top-k`) works for
single-hop similarity retrieval. It does not prove long chains, cross-cutting
impact, auth behavior, data flow, or migration effects, because the answer is a
typed path, not a similar chunk. Vectors produce plausible candidates; the graph
stage refuses candidates that do not resolve to real evidence.

The design is an information-bottleneck tradeoff: keep context small while
preserving the facts most likely to affect task success.

## What's Inside

| Surface | Count / status | Reference |
|---|---:|---|
| Entity kinds | 55 | [crates/codegraph-core/src/kinds.rs](crates/codegraph-core/src/kinds.rs) |
| Relation kinds | 67 | [crates/codegraph-core/src/kinds.rs](crates/codegraph-core/src/kinds.rs) |
| Exactness labels | 8 | [crates/codegraph-core/src/kinds.rs](crates/codegraph-core/src/kinds.rs) |
| Tree-sitter language families | 11 | [crates/codegraph-parser/Cargo.toml](crates/codegraph-parser/Cargo.toml) |
| Frontends | 13 including JSX/TSX | [docs/language-frontends.md](docs/language-frontends.md) |
| Compression formats | binary 1-bit, int8 SQ, PQ, Matryoshka prefixes | [crates/codegraph-vector/src/lib.rs](crates/codegraph-vector/src/lib.rs) |
| Storage | SQLite under `.codegraph/` plus FTS5 | [crates/codegraph-store/src/sqlite.rs](crates/codegraph-store/src/sqlite.rs) |
| Interfaces | CLI, MCP server, local Proof-Path UI | [docs/mcp-reference.md](docs/mcp-reference.md) |

## Current Evidence

The stable public reports preserve the evidence below. They are status reports,
not a superiority claim.

| Gate | Result | Status |
|---|---:|---|
| Graph Truth fixtures | 11 / 11 | pass |
| Context Packet fixtures | 11 / 11 | pass |
| DB integrity | ok | pass |
| Proof source-span coverage | 100% | pass |
| Forbidden edge/path hits | 0 | pass |
| Derived facts without provenance | 0 | pass |
| Test/mock production leakage | 0 | pass |
| Repeat unchanged index | 1.674s | pass |
| Single-file update | 336ms | pass |
| context_pack p95 | 852ms | pass |
| Unresolved-calls page p95 | 243ms | pass |
| Proof DB size | 171.184 MiB vs 250 MiB target | pass |
| Intended Tool Quality Gate | `FAIL` in stable report | open |
| CodeGraphContext comparison | timeout/incomplete | diagnostic |

The README does not claim final intended-performance readiness. Use the report
links below for exact numbers and the current gate verdict.

## CodeGraph vs CodeGraphContext

A fair comparison needs both systems to complete comparable indexing and query
artifacts.

| Comparison item | Result |
|---|---|
| CGC available | yes, version 0.4.7 in the preserved diagnostic |
| CGC completed current comparable run | no |
| CGC timeout | yes |
| CodeGraph vs CGC speed | unknown |
| CodeGraph vs CGC storage | unknown |
| CodeGraph vs CGC quality | unknown |
| Verdict | incomplete |

CGC timed out on the comparable indexing run. Until CGC completes comparable
artifacts, this is not reported as a CodeGraph win. A timeout, skipped run,
partial DB, or fake-agent dry run is never counted as superiority evidence.

## Architecture

Three practical layers, one funnel:

```text
                         +----------------------+
                         |       Codex/agent    |
                         |  CLI / IDE / app UI  |
                         +----------+-----------+
                                    |
                                    | MCP
                                    v
                         +----------------------+
                         |   CodeGraph MCP      |
                         |  context_pack API    |
                         +----------+-----------+
        +---------------------------+---------------------------+
        v                           v                           v
+-----------------+       +---------------------+      +------------------+
| Exact graph     |       | Compressed retrieval |      | Ranker           |
| AST/CFG/DFG/    |       | binary/int8/PQ/MRL   |      | + uncertainty    |
| types/auth/test |       |                      |      |                  |
+--------+--------+       +----------+----------+      +--------+---------+
         +---------------------------+---------------------------+
                                     v
                         +----------------------+
                         |  Exact verification  |
                         |  paths + spans       |
                         +----------+-----------+
                                    v
                         +----------------------+
                         |  Compact context     |
                         |  proof packet        |
                         +----------------------+
```

- **Extract and store:** parse source files, assign stable identities, record
  source spans, and persist exact/heuristic facts in SQLite.
- **Verify and rank:** use retrieval to suggest candidates, then verify graph
  paths, exactness, provenance, and production/test/mock context.
- **Package evidence:** return compact context packets with proof paths,
  snippets, expected tests, and labels a coding agent can use.

## Language Support

Tree-sitter extraction covers JavaScript, JSX, TypeScript, TSX, Python, Go,
Rust, Java, C#, C, C++, Ruby, and PHP. Support varies by language and extractor:
JS/TS has the richest relation coverage, Python/Go/Rust have conservative
caller/callee support, and several languages are syntax/entity-first. See
[docs/language-frontends.md](docs/language-frontends.md) for the tiered support
matrix, exactness labels, and known limitations.

## Interfaces

- `codegraph-mcp index` - build the local graph.
- `codegraph-mcp query ...` - search symbols, text, relations, paths, callers,
  callees, impact, and unresolved calls.
- `codegraph-mcp context-pack ...` - emit agent-facing proof context.
- `codegraph-mcp serve-mcp` - expose local read-mostly MCP tools.
- `codegraph-mcp serve-ui` - open the local Proof-Path UI.
- `codegraph-mcp bench comprehensive` - write the correctness/context/storage/
  latency/update/comparison gate.

Indexing uses DB passport preflight. Valid matching DBs can reuse
incrementally; stale, mismatched, corrupt, or unknown default DBs are rebuilt
safely instead of silently reused.

## Platform Support

| Platform | Status | Verification |
|---|---|---|
| Windows | Supported and tested | PowerShell fresh-clone and index smoke scripts |
| Linux via Docker | Supported and tested | `Dockerfile` and `scripts/smoke_docker.sh`; requires Docker daemon |
| WSL2 | Supported | Use Linux scripts inside an Ubuntu/Debian WSL2 distro |
| macOS | Coming soon | Not currently tested, no CI coverage, not claimed as supported |

## Verification

Core local checks:

```bash
cargo build --workspace
cargo test --workspace
python scripts/check_readme_artifacts.py
python scripts/check_markdown_links.py
```

Smoke checks:

```bash
# Linux / WSL2 / Git Bash
./scripts/smoke_fresh_clone.sh
./scripts/smoke_index.sh
./scripts/smoke_docker.sh

# Windows PowerShell
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke_fresh_clone.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\smoke_index.ps1
```

The deterministic fixture at [fixtures/smoke/basic_repo](fixtures/smoke/basic_repo)
is the mandatory CI-sized smoke. Full-repo `index .` is local-only by default
unless a CI job explicitly opts into it.

## Reports

Stable report summaries:

- [comprehensive_benchmark_latest.md](reports/final/comprehensive_benchmark_latest.md) / [json](reports/final/comprehensive_benchmark_latest.json) - latest preserved comprehensive gate.
- [intended_tool_quality_gate.md](reports/final/intended_tool_quality_gate.md) / [json](reports/final/intended_tool_quality_gate.json) - Intended Tool Quality Gate.
- [manual_relation_precision.md](reports/final/manual_relation_precision.md) / [json](reports/final/manual_relation_precision.json) - manual sampled precision boundary.
- [codegraph_vs_cgc_latest.md](reports/comparison/codegraph_vs_cgc_latest.md) / [json](reports/comparison/codegraph_vs_cgc_latest.json) - CGC comparison status.

Reference docs:

- [docs/architecture.md](docs/architecture.md)
- [docs/benchmark-guide.md](docs/benchmark-guide.md)
- [docs/mcp-reference.md](docs/mcp-reference.md)
- [docs/guardrails.md](docs/guardrails.md)
- [docs/cli-reference.md](docs/cli-reference.md)
- [docs/quality-gates.md](docs/quality-gates.md)
- [docs/install.md](docs/install.md)
- [docs/troubleshooting.md](docs/troubleshooting.md)

The README intentionally links only durable report summaries. Temporary run
outputs and local evidence directories are excluded.

## Manual Precision Status

Manual precision evidence is sampled precision only:

- 320 labeled samples total.
- Recall is unknown because there is no false-negative gold denominator.
- No precision claim is made for absent proof-mode relations, including
  `AUTHORIZES`, `CHECKS_ROLE`, `SANITIZES`, `EXPOSES`, `TESTS`, `ASSERTS`,
  `MOCKS`, and `STUBS`.

## Known Limitations

- Intended Tool Quality Gate is not fully green in the stable published report.
- CGC comparison is diagnostic, blocked/incomplete, and does not support a
  CodeGraph superiority claim.
- Manual precision is sampled precision only; recall is unknown.
- Relation coverage varies by language and extractor.
- macOS is coming soon; it is not currently tested or supported by this
  baseline.
- Full-repo indexing is local-only smoke unless a CI job explicitly opts into
  it.
- Knowledge-graph embedding methods such as TransE, RotatE, ComplEx, TuckER,
  hyperbolic relation embeddings, and tensor decomposition are research
  directions for offline prior learning. They are not required for the runtime
  path.

## Safety and Scope

- **Local first.** Graph state is written under `.codegraph/`.
- **Read-mostly MCP.** Source-editing and destructive tools are not exposed.
- **Exact graph first.** Retrieval shortcuts cannot prove facts by themselves.
- **Single-agent workflow.** Designed for one linear Codex-style coding agent,
  not parallel subagent delegation.
- **Honest measurement.** Unsupported, skipped, unavailable, or diagnostic data
  stays `unknown`, `skipped`, or `diagnostic`. A timeout or partial run is never
  counted as a win.

## References

The compression and retrieval design draws on published work; the typed-graph
foundation is closer to program-analysis literature than to embedding-only
retrieval.

- Yamaguchi et al., *Modeling and Discovering Vulnerabilities with Code
  Property Graphs* (S&P 2014) - AST + CFG + PDG unified into one graph for
  static analysis.
- Guo et al., *GraphCodeBERT: Pre-training Code Representations with Data Flow*
  (ICLR 2021) - data flow as "where the value comes from" relations between
  variables.
- Jegou, Douze, Schmid, *Product Quantization for Nearest Neighbor Search*
  (TPAMI 2011) - subvector-decomposed codebooks.
- Kusupati et al., *Matryoshka Representation Learning* (NeurIPS 2022) - a
  single embedding usable at multiple prefix dimensions.
- Bordes et al., *TransE* (NeurIPS 2013); Trouillon et al., *ComplEx* (ICML
  2016); Sun et al., *RotatE* (ICLR 2019); Balazevic et al., *TuckER* (EMNLP
  2019) - knowledge-graph embedding families for offline candidate/path prior
  learning.
- Nickel and Kiela, *Poincare Embeddings for Learning Hierarchical
  Representations* (NeurIPS 2017); Balazevic et al., *MuRP* (NeurIPS 2019) -
  hyperbolic embeddings for hierarchy-heavy structure.
- Tishby and Zaslavsky, *Deep Learning and the Information Bottleneck Principle*
  (ITW 2015) - context-packet sizing as a preserve-the-useful-information
  bottleneck.
