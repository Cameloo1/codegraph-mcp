# Architecture Notes

These notes describe the current public CodeGraph architecture and the proof
boundaries that keep agent-facing context trustworthy.

## Mission

CodeGraph provides compact, verified codebase context for one Codex-style
coding agent. It builds a deterministic typed program graph, uses retrieval
only to suggest candidates, and requires graph/source verification before a
packet can claim graph proof.

## Source Of Truth

The source of truth is the lifecycle-valid local graph plus source spans.
Embeddings, binary signatures, relation priors, Bayesian scores, rankers, and
candidate spools can help find or order candidates, but they do not prove facts.

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

## What This Is

CodeGraph is built as four practical layers:

1. **Typed program graph.** Tree-sitter frontends extract entities, relations,
   source spans, source roles, exactness labels, and provenance into SQLite.
2. **Candidate retrieval funnel.** Exact seeds, text evidence, graph-neighborhood
   paths, binary/vector lanes, and nuance rescue can find likely context.
3. **Graph/source verification.** Candidate lanes are not answers. The graph and
   source spans decide whether a relation is proven, source-text-only, or still
   unknown.
4. **Agent-use and MCP surfaces.** The CLI and MCP server expose bounded,
   lifecycle-checked packets for one linear coding-agent workflow.

The short rule:

```text
vectors suggest; graph verifies; source spans support the packet
```

## Architecture Overview

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
                         |   codegraph-mcp      |
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

## What Ships Today

- Local SQLite graph indexing with DB passport/lifecycle preflight.
- A production `agent-use` profile that stores the agent-facing DB outside the
  source tree.
- Symbol, text, file, relation, caller/callee, path, impact, and context-pack
  commands.
- Read-mostly MCP serving over the same proof-oriented context surface.
- Source spans, exactness labels, source roles, proof/no-proof labels, omitted
  counts, warnings, and recovery commands.
- Bounded candidate lanes for harder recall tasks, kept non-proof until
  graph/source verification succeeds.
- `agent-use watch --once --changed` plus persistent watch scheduling over the
  same changed-file update primitive.
- A local loopback Proof-Path UI.

## What's Inside

| Surface | Count / status | Reference |
|---|---:|---|
| Entity kinds | 55 | [crates/codegraph-core/src/kinds.rs](../crates/codegraph-core/src/kinds.rs) |
| Relation kinds | 67 | [crates/codegraph-core/src/kinds.rs](../crates/codegraph-core/src/kinds.rs) |
| Exactness labels | 8 | [crates/codegraph-core/src/kinds.rs](../crates/codegraph-core/src/kinds.rs) |
| Tree-sitter frontends | 13 | [Language Frontends](language-frontends.md) |
| Compression formats | binary 1-bit, int8 SQ, PQ, Matryoshka prefixes | [crates/codegraph-vector/src/lib.rs](../crates/codegraph-vector/src/lib.rs) |
| Storage | SQLite plus FTS5; default CLI uses `.codegraph/`, production `agent-use` uses an external profile DB | [crates/codegraph-store/src/sqlite.rs](../crates/codegraph-store/src/sqlite.rs) |
| Interfaces | CLI, MCP server, local Proof-Path UI | [MCP Reference](mcp-reference.md) |

## Runtime Funnel

CodeGraph is a staged runtime funnel sitting on top of a typed program graph.
Each stage does one specific job; downstream stages cannot fabricate facts not
present upstream.

The important boundary is the candidate merge. It happens after independent
candidate lanes have produced source-spanned candidates and before graph/source
verification turns any of them into claimable context. The merge is not part of
Tree-sitter parsing and it is not a vector answer. It is the `context-pack`
retrieval assembly step: candidates are unioned by stable path/span/entity keys,
their `candidate_sources`, matched seeds, source labels, ranking features, and
verification status are preserved, exact seeds are protected across caps, and
mixed evidence stays role-labeled.

Candidate lanes are explicit: exact symbol/file/path seeds, Stage 0
lexical/text-evidence matches, graph-neighborhood and PathEvidence candidates,
vector semantic candidates when explicitly enabled, binary/1-bit candidates with
deterministic overfetch/rerank, and nuance-rescue candidates for rare
identifiers, config keys, route literals, test names, negation terms, and
no-extension support scripts. None is graph proof by itself.

```text
Query/context-pack flow
-----------------------

agent task + optional seed
  |
  v
DB lifecycle read gate
  |
  v
prompt intent + seed extraction
  |
  +--> exact symbol/file/path seeds
  +--> Stage 0 lexical/FTS/text-evidence candidates
  +--> graph-neighborhood/path-evidence candidates
  +--> Stage 1 binary/1-bit candidates when available
  +--> nuance-rescue candidates when enabled
  +--> Stage 2 compressed-rerank candidates when available
  |
  v
UNION / DEDUP / RANK
stable key = path + span + entity when available
preserve candidate_sources, matched_seeds, evidence_role, proof_status
  |
  v
exact graph/source verification
  |
  +--> graph path found
  |      -> PathEvidence with typed relations and source spans
  |
  +--> no graph path found
         -> bounded source-text fallback with no_proof_path_found
            text evidence is claimable as source text, not graph proof
  |
  v
compact context packet
proof paths + snippets + risks + recommended tests + omitted counts
  |
  v
agent-safe output
--agent-json / --concise / explicit verbose-audit modes
```

## Index-Time State

```text
repository
  |
  v
scope policy + DB lifecycle/passport preflight
  |
  +--> parser-backed files
  |      |
  |      v
  |   Tree-sitter frontends
  |      |
  |      v
  |   typed graph facts
  |   entities + relations + source spans + exactness/provenance
  |
  +--> scoped non-parser text files
         |
         v
      Stage 0 text evidence
      path/title/tokens/snippets/FTS rows
      evidence_role=text_evidence, graph_proof=false

Both lanes publish into SQLite with passported repo/scope/storage identity.
```

Indexing uses DB passport preflight. Valid matching DBs can reuse
incrementally; stale, mismatched, corrupt, or unknown default DBs are rebuilt
safely instead of silently reused. Explicit named DBs are more conservative and
must not be silently trusted when lifecycle checks fail.

## Stages

### 1. Parse -> Typed Program Graph

Tree-sitter parses 13 frontends: JavaScript, JSX, TypeScript, TSX, Python, Go,
Rust, Java, C#, C, C++, Ruby, and PHP. Each AST construct becomes a typed
entity such as `Function`, `Method`, `Class`, `Interface`, `Field`, `CallSite`,
`ReturnSite`, `Route`, `Middleware`, `AuthPolicy`, `Migration`, `TestCase`,
`Mock`, or `ConfigKey`.

The graph model currently defines 55 entity kinds, 67 relation kinds, and 8
exactness labels in [crates/codegraph-core/src/kinds.rs](../crates/codegraph-core/src/kinds.rs).
Relation coverage varies by language and extractor, and unsupported proof-mode
relations do not receive precision claims.

Relation groups include:

- **Structural:** `CONTAINS`, `DEFINED_IN`, `DEFINES`, `DECLARES`, `EXPORTS`,
  `IMPORTS`, `REEXPORTS`, `BELONGS_TO`, `CONFIGURES`
- **Type/object:** `TYPE_OF`, `RETURNS`, `IMPLEMENTS`, `EXTENDS`,
  `OVERRIDES`, `INSTANTIATES`, `INJECTS`, `ALIASED_BY`, `ALIAS_OF`
- **Execution:** `CALLS`, `CALLED_BY`, `CALLEE`, `ARGUMENT_0`, `ARGUMENT_1`,
  `ARGUMENT_N`, `RETURNS_TO`, `SPAWNS`, `AWAITS`, `LISTENS_TO`
- **Data flow:** `READS`, `WRITES`, `MUTATES`, `MUTATED_BY`, `FLOWS_TO`,
  `REACHING_DEF`, `ASSIGNED_FROM`, `CONTROL_DEPENDS_ON`, `DATA_DEPENDS_ON`
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

Stage 0 also includes scoped text evidence for important files that are not
parser-backed graph proof, such as build/config/docs/support files. Those rows
can make a file queryable and source-spanned, but they remain labeled as text
evidence rather than typed graph relations.

### 3. Stage 1 - 1-Bit Binary Sieve

Each indexed entity can carry a deterministic bit-packed signature. Stage 1 is a
candidate lane that reduces large candidate sets via Hamming distance:

```text
sim(x, y) = d - 2 * popcount(x XOR y)
```

This is a cheap narrowing pass: XOR, popcount, no floating point, no full-vector
decompression. It suggests candidates only; graph/source verification still
decides what is claimable.

### 4. Stage 2 - Compressed Rerank

Surviving candidates are rescored against the query in compressed forms:

- **int8 scalar quantization:** compact vector storage with a scale factor.
- **Product Quantization (PQ):** subvector codebooks with compact u8 codes.
- **Matryoshka prefixes:** one embedding usable at multiple prefix dimensions.

The reranker is deterministic: same query and same index commit produce the same
ranking. Exact seeds, text scores, graph-neighborhood signals,
compressed-vector scores, and uncertainty can be combined in the union/dedup/rank
step, but the result is still only a candidate set.

### 5. Stage 3 - Exact Graph Verification

The top candidates are not treated as answers. Stage 3 walks the typed program
graph between seed entities and candidates:

- bounded BFS/DFS with relation filters
- weighted path search over relation costs
- k-shortest paths for multiple proof routes
- relation-pattern queries for dataflow and impact questions
- derived closure edges that retain provenance to base edges

Every retained step carries source-span and exactness evidence. Heuristic edges
can participate, but the packet labels the evidence accordingly. If no typed
path exists, context-pack can still return bounded source-text fallback evidence
with `no_proof_path_found`; that fallback is useful context, not relation proof.

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

The packet example illustrates the intended shape. Actual relation availability
depends on the indexed language frontend and extractor support.

## Major Crates

1. `codegraph-core` defines the serializable domain model for entities,
   relations, source spans, provenance, exactness, path evidence, derived edges,
   context packets, stable IDs, and broad endpoint validation.
2. `codegraph-store` persists exact graph facts locally behind `GraphStore`,
   with SQLite via `rusqlite` implemented first.
3. `codegraph-parser` parses source into syntax metadata and extracts
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
   stdio JSON-RPC MCP server with input/output schemas, safety annotations,
   resources, prompt templates, pagination, resource links, and explain-missing
   output.
7. `codegraph-cli` provides local commands for indexing, status, querying,
   impact analysis, context packs, bundles, MCP serving, UI serving, Codex
   template installation, optional live watching, the local Proof-Path UI HTTP
   server, hardened caller/callee/chain query commands, diagnostics, shell
   completions, release metadata, and profiled indexing-speed fixtures.
8. `codegraph-bench` contains developer and lab harnesses for extraction,
   retrieval, path recall, security/auth, async/event flow, test-impact, and
   patch-outcome experiments. Lab outputs must stay diagnostic unless promoted
   through the release documentation rules.
9. `codegraph-ui` contains bundled static assets for the local Proof-Path UI
   served by `codegraph-mcp serve-ui`, including proof/neighborhood/impact graph
   modes, exactness legends, source-span preview, export/copy controls, and
   large-graph guardrails.

## Why Not Just Embeddings

A pure vector pipeline (`text -> embedding -> cosine -> top-k`) works for
single-hop similarity retrieval. It does not prove long chains, cross-cutting
impact, auth behavior, data flow, or migration effects, because the answer is a
typed path, not a similar chunk. Vectors produce plausible candidates; the graph
stage refuses candidates that do not resolve to real evidence.

The design is an information-bottleneck tradeoff: keep context small while
preserving the facts most likely to affect task success.

## Operational Profiles

Use the profiles in [operational-profiles.md](operational-profiles.md) to keep
temporary development/test DBs separate from the graph used by a coding agent:

- The development profile uses local diagnostic DBs for testing CodeGraph
  changes.
- The agent-use profile uses a release binary and a DB outside the source tree
  for routine coding-agent context.
- The agent-use changed-file update path starts with the deterministic
  `watch --once --changed` primitive against that external DB. Persistent
  production watching is scheduling around the same lifecycle-gated update
  contract: it coalesces filesystem events, serializes writes, and refuses
  unsafe profile DB states instead of owning a separate update engine or
  falling back to repo-local `.codegraph`.

Agent-use reads should answer only after the DB lifecycle preflight says the DB
is valid, matching, claimable, and not contaminated. Development/test DBs are
never superiority evidence by themselves.

## Current Evidence Boundary

The public architecture contract is behavioral: graph/source verification is
the only graph-proof path, and candidate layers remain labeled until verified.

Raw DBs, WAL/SHM files, raw logs, diagnostic payloads, local run outputs, and
temporary comparison artifacts are evidence inputs for development. They are
not public architecture claims unless a small summary is intentionally promoted
and claim-reviewed.

## Roadmap Boundary

The current architecture is deliberately local, deterministic, and proof-first.
Future research features such as dynamic tracing, solver-backed verification,
LSP memory-buffer overlays, or learned graph priors should remain evidence-gated
additions. They should not weaken the runtime contract that retrieval suggests,
the graph verifies, and source spans support final context.

Stage 0 text evidence is part of the current runtime for scoped non-parser
planning files such as Makefiles, Kconfig/Config.in, docs, and support scripts.
It is source-text evidence only, not typed graph proof.

Real-Time Delta Sync is complete for the local production-profile gate:
one-shot changed-file updates, add/delete/rename lifecycle, dirty evidence
invalidation, bounded dependency closure, publish/read safety, local diagnostic
performance, persistent watcher scheduling, and MCP/agent freshness surfaces are
verified. The validate-edit bridge remains deferred to MVP3 rather than being
claimed as a compiler/test or complete dangling-edge validator.

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

## Related Docs

- [Language Frontends](language-frontends.md) lists the supported parser tiers,
  relation coverage, exactness labels, and known language limits.
- [Guardrails](guardrails.md) explains the proof boundary and claim discipline
  that the architecture must preserve.
- [Quality Gates](quality-gates.md) lists release-facing checks for DB
  lifecycle, proof labels, docs hygiene, and artifact boundaries.
- [Operational Profiles](operational-profiles.md) explains where each DB/profile
  is allowed to live.
- [MCP Reference](mcp-reference.md) documents the read-mostly agent interface
  built on top of the graph/context surfaces.
