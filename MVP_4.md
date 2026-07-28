# MVP_4.md — CodeGraph MCP AST Quantization / Micro-flow Proof Packets

Generated: 2026-05-20
Source of truth relationship: `MVP.md` remains the core system specification. `MVP_2.md` defines the active MVP2 roadmap and prompt contract. `MVP_3.md` defines continuous graph validation. `MVP_4.md` defines the future AST Quantization roadmap: deterministic extraction and compression of local AST mechanics into compact, source-spanned micro-flow proof packets.

---

## Current Local Boundary (2026-07-14)

MVP4 local implementation is current through MVP4.3 for scoped Tier 5/5 production semantics across all 13 registered canonical frontends: MVP4.1 micro-node extraction, MVP4.2/MVP4.2b local micro-edge relations, and MVP4.3 local micro-flow packets.

Current MVP4.3 packet behavior is local and bounded: packets are built only from persisted MVP4.1/MVP4.2 facts, persisted in `local_flow_packets`, encoded as compact `dict_v1` by default, and expanded to verbose `ordered_steps` only through explain/audit paths. `flow_proof` is restricted to complete eligible function-local packet paths and is downgraded for partial, stale, corrupt, truncated, unsupported, parser-recovery, or gap-bearing states.

The pre-MVP4.4 frontend, packet, product-surface, and linter lanes are complete
for JavaScript, JSX, TypeScript, TSX, Python, Go, Rust, Java, C#, C, C++, Ruby,
and PHP. The authoritative fresh production release gate passed all 13
canonical rows with zero unmet rows and zero runner errors.

Scoped Tier 5/5 means `local_binding_resolved=supported_exact`,
`read_write_extracted=supported_exact`,
`local_dataflow_derived=supported_derived_with_provenance`, and
`local_flow_packet_supported=supported_exact` at the same-file
intraprocedural production boundary, with claimable source spans, the complete
13-node/11-edge packet contract, `dict_v1` lossless expansion, zero packet
gaps, and zero cap omissions in the canonical gate. It is not a blanket claim
for project-wide, cross-file, dynamic, runtime, macro, framework, compiler, or
security semantics.

TypeScript preserves an explicit extension boundary: ordinary `.ts` uses the
bounded legacy-v1 local adapter, `.mts` and `.cts` use ParserFactsV1, and
`.d.ts` remains inactive. The representative TypeScript readiness source is
`.mts`; the canonical language gate is not itself per-extension certification.

Capability flags remain the source of truth, exact facts remain fixture-backed,
resolver/compiler facts require provenance, and unsupported dynamic, macro,
runtime, preprocessor, framework, text, candidate, vector, and nuance surfaces
remain non-proof unless a later exact gate proves otherwise.

MVP4 still does not inherit any public benchmark, real-agent patch-quality, official SWE-bench, CodeGraph-over-rg/CGC, production semantic-quality, final intended-performance, route/bridge precision, dynamic/macro, runtime, or framework semantic-support claim. `mutation_proof`, route/bridge proof, context-entry command/tool activation, distribution publication, public benchmark claims, and real-agent patch-quality claims remain inactive.

Next local phase: planning and human approval for MVP4.4 AST Skeleton Compression. No MVP4.4 implementation has started.

---

## 0. Executive Summary

MVP4 turns CodeGraph from a file/function/relation graph into a deeper **local structural proof system**.

MVP2 answers:

```text
Before editing, where should the agent look, and what is claimable?
```

MVP3 answers:

```text
After editing, did the agent break structural graph contracts?
```

MVP4 answers:

```text
Inside this function/file, what exact local mechanics did the developer write, and how can that be delivered as a compact proof packet?
```

MVP4 introduces **AST Quantization**:

```text
AST Quantization = deterministic extraction and compression of AST-level local structure into compact typed packets that preserve source-spanned proof of developer-authored links.
```

It is not AI inference. It is not lexical proximity. It is not a learned semantic guess. It is deterministic extraction of local program mechanics that already exist in source code.

Updated MVP4 phase list:

```text
1. Micro-node extraction
2. Micro-edge extraction
3. Local micro-flow packets
4. Sparse sidecar storage
5. Dynamic dispatch and macro boundaries
6. Mathematical proof delivery
7. Implementation Trace enrichment
8. Evidence Proof Ladder mapping
9. Runtime/artifact boundary handling
10. Framework-aware route proof edges
11. Cross-language bridge edges
12. One-call context entry packet
13. Distribution and zero-friction onboarding
```

Phases 10–13 are competitive-parity features adapted (as concepts, not code) from
TypeScript code-graph tools that ship framework routing, cross-language bridging,
one-call context, and a zero-build installer. They are folded into MVP4 because
routing and bridging are deterministic AST/edge extraction (the MVP4 theme), the
one-call packet is an aggregation over existing proof surfaces, and the installer
is a distribution track. None of them may weaken the proof boundary: a route or
bridge edge is `exact` only with a source-spanned, statically-resolved binding;
everything else is `heuristic`, `unsupported`, or `unknown`, never proof.

Post-MVP4 product goals:

1. Full-codebase freshness.
   - Maintain continuous, lifecycle-safe indexing across the full codebase so the agent works from current repository state instead of stale prompt memory.
   - Delta sync, stale-state refusal, source bindings, and passport checks must make old or mismatched evidence non-claimable.

2. Proof over retrieval.
   - Preserve the proof ladder in every agent-facing packet: candidate evidence, text evidence, source-navigation evidence, graph relation proof, mutation proof, and flow proof must not collapse into one generic context bucket.
   - Unsupported codebase claims should become non-claimable, blocked, or explicitly unknown before they become trusted agent context or accepted edits.

3. Lower time and token cost.
   - Return compact, high-signal routing and proof packets that let agents diagnose complex, intermingled production-level systems with fewer blind searches, fewer retries, and less prompt volume than raw full-repo exploration.
   - CodeGraph should feel effortless for an agent to use: as close as possible to a developer's instant mental map of a codebase, but continuously refreshed and source-verified.

4. Patch-quality lift under controlled evaluation.
   - Prove that the same rg-using agent/model/scaffold, on the same tasks and budgets, produces fewer wrong-file edits, fewer nonexistent-symbol references, fewer unsupported claims, and more resolved patches when CodeGraph is added as the context/trust layer.
   - This is a required product goal, not a current public claim.

MVP4 is expected to support the SWE-bench Pro stage of the benchmark spine by
turning local implementation mechanics into deterministic micro-flow evidence.
SWE-bench Pro remains a diagnostic north star until real external-agent
predictions are evaluated with an official-compatible harness and complete
run metadata.

---

## 1. MVP4 Core Principle

MVP4 must preserve the same proof boundary as MVP2/MVP3:

```text
Text evidence, vector recall, binary recall, and nuance rescue may suggest.
Graph/source verification decides graph proof.
AST micro-flow packets are claimable only for deterministic, source-spanned local structures.
```

MVP4 should increase granularity without increasing hallucination risk.

MVP4 also preserves the MVP2 Evidence Proof Ladder. It does not collapse text
evidence, symbol evidence, candidate evidence, source-navigation evidence,
graph relation proof, mutation proof, and flow proof into one generic
"context" bucket.

MVP2 `implementation_trace` packets provide bounded source-navigation evidence for implementation walkthroughs. MVP4 enriches this mode with AST Quantization: local variables, assignments, returns, call sites, mutation sites, and formula-like micro-flows become deterministic, source-spanned micro-flow proof packets when exact extraction is possible.

Policy optimization can tune retrieval order, caps, packet selection, and
candidate routing. MVP4 proof strength still comes only from deterministic
micro-node/micro-edge extraction with source spans and provenance.

OpenEvolve-style policy search may tune retrieval over future micro-flow packet
handles, caps, and ranking. It must not generate proof-bearing micro-flow facts
or raise candidate evidence into proof. Candidate packet and query-index
patterns from MVP2 are the model for bounded micro-flow retrieval, but MVP4
must avoid repeating the old candidate-spool firehose shape.

Examples of MVP4 enrichment:

```text
local variable accounting formulas
return-value flow
persisted-size calculations
DB row/count aggregation paths
vector chunk summary construction
source-spanned local mutation/call chains
```

Boundary:

```text
MVP4 micro-flow packets can prove exact local mechanics only when extracted deterministically from source spans.
Artifact math that depends on runtime DB rows or JSON artifacts still requires artifact/DB inspection evidence.
MVP4 does not prove runtime DB contents, artifact values, framework routing, dynamic dispatch, or macro-generated behavior unless those are also verified by appropriate evidence.
```

### Investigation Micro-Flow Enrichment

MVP4 can strengthen MVP2 Agent Investigation Layer packets with deterministic
local micro-flow proof when exact AST extraction supports it.

Examples:

```text
gold_symbols -> query_terms
query_terms -> provider command argv
stderr -> wrapper output
Docker exit code -> PowerShell exit
user input -> SQL call
auth check -> route handler
cache freshness -> report readiness
```

Investigation enrichment fields:

```text
flow_proof
mutation_proof
micro_flow_path
source_spanned_assignments
call_argument_chain
return_value_chain
proof_limitations
```

Boundaries:

```text
MVP4 proves local mechanics only when deterministic extraction supports it.
Runtime behavior, external systems, model behavior, Docker state, network state, and generated artifacts require separate evidence.
MVP4 does not prove global interprocedural behavior unless graph/source verification supports it.
```

Proof-ladder enrichment:

```text
source_navigation_evidence -> source-spanned implementation walkthrough context
mutation_proof             -> deterministic local write/mutation mechanics
flow_proof                 -> deterministic source-spanned local flow chain
```

---

## 2. MVP4 Non-Goals

MVP4 must not claim:

```text
full runtime behavior
complete typechecker replacement
complete interprocedural dataflow
complete macro expansion unless expansion is available
full framework semantics
security vulnerability proof by default
SMT/formal proof for arbitrary patches
learned semantic proof
```

MVP4 may prepare later SMT/Z3 or runtime tracing branches, but its first version is deterministic local AST structure.

---

## 3. MVP4 Prerequisites

MVP4 should begin only after:

```text
MVP2 Agent Routing Packet is working
DB lifecycle and claimability are stable
Chaos/Durability gate is green
Real-time delta sync is usable
Language coverage matrix is stable
MVP3 validate-edit can update and diff changed files
source spans/provenance are reliable
bounded graph walking is stable
demand-driven graph context hydration over indexed relation facts is stable or explicitly bounded in agent packets
bounded candidate/context sidecar patterns are stable
candidate packet query/status indexes are bounded and lifecycle-safe
```

Schema extensibility prep from MVP2 should already exist:

```text
sparse sidecar contracts
schema versioning
compatibility tests
storage-budget tests
```

---

## 4. Storage Strategy

### 4.1 Sparse Sidecars, Not Core Table Bloat

Do not add bulky JSON payloads to every entity or edge.

Prefer sparse sidecar tables:

```text
ast_micro_nodes
ast_micro_edges
local_flow_packets
entity_features
edge_features
routing_packet_handles
validation_findings
```

Core tables stay compact:

```text
entities
edges
source_spans
files
path_evidence
```

MVP4 should build on demand-driven graph context hydration rather than treating
precomputed `path_evidence` rows as the full semantic map. `local_flow_packets`
are sparse proof packets for exact local mechanics; they complement normalized
graph facts and on-demand relation traversal, and should not become another
global precomputed packet cap that limits what the agent can retrieve.

### 4.2 Sidecar Row Requirements

Each sidecar row should include:

```text
stable id
file_id
function_entity_id if applicable
entity_id or edge_id if applicable
source_span_id
feature_kind
payload_version
extraction_version
schema_version
compact_payload or normalized fields
claimability
```

### 4.3 Storage Bounds

MVP4 must enforce:

```text
max micro-nodes per function
max micro-edges per function
max packet bytes
max local-flow steps returned
max snippets returned
max candidate packet/index bytes when micro-flow packets become retrieval candidates
no full source body storage
no redundant snippet storage
no optimizer-generated packet format can bypass sidecar caps
query/status over packet indexes must remain bounded
```

### 4.4 Compatibility

```text
old DBs without sidecars remain readable
commands can ignore sidecars if unsupported
schema mismatch is explicit
micro-flow data cannot weaken existing graph proof boundaries
```

---

## 5. MVP4.1 — Micro-node Extraction

### Purpose

Extract local AST units that can support deterministic micro-flow packets.

### Micro-node Types

```text
FunctionFrame
Parameter
LocalBinding
PropertyAccess
CallSite
ReturnSite
AssignmentSite
MutationSite
ConditionSite
LiteralKey
ImportBinding
ExportBinding
TestAssertion
RouteLiteral
AuthLiteral
SanitizerCall
```

### Required Fields

```text
micro_node_id
node_kind
file_id
function_entity_id
source_span_id
name_or_literal
symbol_binding_id if known
source_role
language
exactness
```

### Claim Rules

```text
micro-node existence is claimable if source-spanned
binding resolution is claimable only if exact resolver proves it
literal keys are source text evidence unless relation semantics are implemented
```

### Tests

```text
function parameters
local variables
property access
callsite
return site
assignment site
mutation site
condition site
literal route key
auth role literal
assertion call
source span bounds
```

---

## 6. MVP4.2 — Micro-edge Extraction

### Purpose

Extract local relationships between micro-nodes.

### Micro-edge Types

```text
LOCAL_READS
LOCAL_WRITES
LOCAL_FLOWS_TO
LOCAL_CALLS
LOCAL_RETURNS_TO
LOCAL_MUTATES
LOCAL_CHECKS
LOCAL_SANITIZES
LOCAL_GUARDS
LOCAL_ASSERTS
LOCAL_BRANCHES_TO
```

### Required Fields

```text
micro_edge_id
micro_edge_kind
head_micro_node_id
tail_micro_node_id
file_id
function_entity_id
source_span_id
provenance
exactness
claimability
```

### Exactness Rules

```text
exact when both endpoints are source-spanned and structurally linked in the AST
derived_with_provenance when inferred from a local sequence or assignment chain
heuristic when resolution is pattern-based
unsupported when runtime/dynamic/macro behavior is not statically proven
```

### Forbidden Behavior

```text
no edge from comments alone
no security relation from string proximity alone
no dynamic dispatch exactness without resolver proof
no macro-generated hidden call unless expansion is available
no relation without source span/provenance where required
```

---

## 7. MVP4.3 — Local Micro-flow Packets

### Purpose

Return compact deterministic skeletons for complex functions.

### Packet Shape

Compact packet bodies should use dictionary compression by default. The packet
stores repeated spans, node refs, edge refs, provenance records, labels, branch
ids, and return-path ids once, then represents each ordered path as a small
program over those dictionary entries. Verbose `ordered_steps` are an
explain/audit expansion, not the default context or routing payload.

```json
{
  "packet_kind": "function_local_flow_packet",
  "packet_id": "micro-packet://...",
  "encoding": "dict_v1",
  "function": {
    "name": "AuthService.login",
    "file": "src/auth.ts",
    "span": "s0"
  },
  "claimability": "claimable_local_flow",
  "proof_status": "micro_flow_found",
  "proof_strength": "flow_proof",
  "packet_body": {
    "dictionary": {
      "spans": {
        "s0": ["src/auth.ts", 10, 1, 28, 2],
        "s1": ["src/auth.ts", 12, 9, 12, 17]
      },
      "nodes": {
        "n1": ["Parameter", "password", "s1", "exact", "claimable"],
        "n2": ["LocalBinding", "password_hash", "s2", "exact", "claimable"]
      },
      "edges": {
        "e1": ["LOCAL_READS", "n1", "n3", "s3", "exact", "p1"],
        "e2": ["LOCAL_FLOWS_TO", "n3", "n2", "s4", "derived_with_provenance", "p2"]
      },
      "provenance": {
        "p1": ["resolver_backed_binding", ["n1", "n3"], ["s1", "s3"]],
        "p2": ["local_assignment_chain_derivation", ["e1"], ["s3", "s4"]]
      }
    },
    "paths": [
      {
        "path_id": "return_path_0",
        "steps": [["edge", "e1"], ["edge", "e2"]]
      }
    ],
    "compression_contract": {
      "lossless_to_audit_ordered_steps": true,
      "source_spans_preserved": true,
      "provenance_preserved": true,
      "full_source_bodies_allowed": false
    }
  },
  "unknowns": [],
  "risks": [],
  "budget": { "omitted_count": 0, "truncation_reason": "not_truncated" }
}
```

### Packet Requirements

```text
ordered local steps
source spans for every step
provenance for derived steps
dictionary-compressed compact body by default
verbose ordered_steps only in explain/audit expansion
handles instead of full packet bodies in routing/context-entry surfaces
bounded size
omitted_count when truncated
claimability per step
proof_strength per packet or step where relevant
unknowns for unsupported dynamic parts
branch identity and return-path identity preserved
shadowed bindings not collapsed
exactness/provenance/source-role never compressed away
```

### Use Cases

```text
explain a complex function
trace local auth/dataflow
inspect mutation logic
understand callback binding
verify sanitizer path inside one function
summarize test assertion mechanics
```

---

## 8. MVP4.4 — AST Skeleton Compression

### Purpose

Strip syntactic noise and project verified function-local packet mechanics into deterministic semantic skeletons. Cross-file graph references may be attached as handles, but they are not function-local flow proof.

### Compression Rules

Keep:

```text
names
bindings
calls
reads/writes
assignments
returns
mutations
literal keys
branch guards
source spans
```

Drop or compress:

```text
formatting
comments except doc/test text evidence
syntactic sugar when equivalent structure is preserved
repeated boilerplate
repeated span/node/edge/provenance labels through packet dictionaries
unneeded full source bodies
```

Do not compress away:

```text
branch distinctions
return path distinctions
shadowed binding identity
unresolved or dynamic gaps
source roles
exactness
provenance
cap omissions
```

### Named Compressed Projection Foundation (Locked MVP5 Prerequisite)

MVP4.4 must establish the named compressed projection before MVP5 introduces
speculative planning overlays or a custom editor. The canonical compressed
skeleton and its displayed source names are separate contracts:

```text
canonical skeleton = stable semantic node/edge identities, proof state, spans,
                     provenance, branch identity, return-path identity, and gaps

named projection   = a bounded rendering of that skeleton using repository
                     function, class, method, object, variable, property, and
                     access-path names
```

Renaming a local variable may change a display label, but must not silently
change the semantic meaning, proof strength, or stable identity of an otherwise
equivalent skeleton. Names are never a replacement for graph identity.

Required named projection modes:

```text
semantic       generic semantic node kinds only
source_named   exact source-level names where verified
qualified      owning file/module/type/function plus source name
compact_named  shortest unambiguous source name; preferred agent default
role_named     source name plus semantic role
mixed          verified source name with explicit semantic fallback
```

Functions, classes, methods, objects, variables, and properties must not be
flattened into unscoped strings. A projected name must remain connected to:

```text
stable symbol or micro-node id
owning scope and function domain
declared or inferred type when claimable
property/access path when applicable
source span
source role
exactness and claimability
derivation provenance
```

The default agent response must remain progressively expandable rather than a
full graph dump. A compact named skeleton should expose the critical path,
branches, mutations, sanitizer/assertion boundaries, gaps, and short source
references, with stable handles for targeted expansion:

```text
flow.get_named
flow.expand_path
flow.expand_node
flow.show_provenance
flow.show_source
```

The exact public command/tool names may change during implementation, but all
surfaces must share one underlying projection contract and deterministic
ordering. Compact output must preserve proof-critical fields under budget and
must report truncation or omitted detail explicitly.

MVP4.4 does not add speculative nodes, planned edges, source mutation, or a
planning editor. Those belong to `MVP_5.md`. MVP4.4 delivers the verified named
projection on which those later surfaces depend.

Named projection acceptance gate:

```text
canonical skeleton round-trips to audit expansion
stable ids are separate from display names
qualified names disambiguate same-name bindings and methods
branch and return-path distinctions survive projection
object/property access paths remain scoped
exactness, provenance, source role, gaps, and cap omissions remain visible
compact named output stays within declared budgets
progressive expansion returns only requested detail
renames do not fabricate proof or silently merge identities
no speculative MVP5 overlay is treated as source-verified graph truth
```

### Output Examples

From code:

```text
const token = req.headers.authorization;
const user = verifyToken(token);
return checkRole(user, "admin");
```

To skeleton:

```text
req.headers.authorization READS -> token
token FLOWS_TO -> verifyToken(token)
verifyToken RETURNS -> user
user + "admin" FLOWS_TO -> checkRole(...)
```

---

## 9. MVP4.5 — Dynamic Dispatch And Macro Boundaries

### Exact Only When Deterministic

Exact examples:

```text
literal route handler with direct symbol
simple event listener with literal event and direct handler
resolved import/export target
direct callback symbol
compiler/LSP-known symbol if integrated
macro expansion available and source-spanned
```

### Heuristic / Unsupported

Label as heuristic or unsupported:

```text
computed dynamic import
reflection
monkeypatching
runtime dependency injection
macro expansion not available
C preprocessor inactive branch
framework route inferred from string proximity
security relation inferred from comments
```

### Required Edge Cases

```text
dynamic import literal vs computed
DI literal registration vs runtime lookup
macro hidden call
C/C++ preprocessor branch
event emitter literal handler
computed callback
trait/interface dispatch
monkeypatch override
```

---

## 10. MVP4.6 — Mathematical Proof Delivery

### Meaning

Mathematical proof delivery in MVP4 does not mean full formal verification for arbitrary patches.

It means:

```text
deterministic local graph proof
source-spanned micro-edges
bounded PathEvidence
provenance for derived microedges
exactness labels
claimability labels
```

### Later Formal Proof Branches

Later work may add:

```text
SMT/Z3 checks for specific patch safety constraints
symbolic execution for narrow local flows
runtime trace validation for dynamic edges
typechecker/LSP proof integration
```

But MVP4 should first deliver deterministic source-spanned micro-flow proof packets.

---

## 11. MVP4.7 — Retrieval Over Micro-flow Packets

### Purpose

Micro-flow packets become a retrieval target.

Candidate sources:

```text
exact symbol
function name
local variable name
literal key
route string
auth role
sanitizer/source/sink term
test assertion name
micro-edge kind
```

Output rules:

```text
micro-flow packet can be claimable for exact local flow
micro-flow packet cannot imply global interprocedural proof unless verified
vector/binary search over micro-flow packets remains candidate recall
optimizer-tuned retrieval over micro-flow packets remains candidate recall until deterministic micro-flow extraction proves the relation
```

---

## 12. MVP4.8 — Language Rollout Strategy

Start with languages already strongest in MVP2 language matrix:

```text
TypeScript / JavaScript
Rust
Python
Go
```

Then expand to:

```text
C/C++
Java
C#
Ruby
PHP
```

Tiered rollout:

```text
local variables and assignments
calls and returns
property access
mutation sites
simple local flow
branch guards
async/callback local mechanics
test assertions
security/auth/sanitizer local mechanics
```

No language gets an exact micro-flow claim without fixtures.

---

## 13. MVP4.9 — Testing Strategy

Required fixtures:

```text
local assignment chain
return value flow
argument-to-call flow
object property flow
mutation method
branch guard
sanitizer on path
sanitizer distractor
literal route handler
auth role direct call
async direct callback
computed dynamic call unsupported
macro hidden call unsupported
partial broken code recovery
same-name local variable shadowing
```

Required negative tests:

```text
comment-only security proof forbidden
string-proximity sanitizer proof forbidden
dynamic import exactness forbidden without resolver
macro expansion exactness forbidden without expansion
full-source-body storage forbidden
unbounded micro-node growth forbidden
```

Gates:

```text
micro-node extraction gate
micro-edge extraction gate
local-flow packet gate
storage-budget gate
claimability gate
language rollout gate
MVP4 final gate
```

---

## 14. MVP4.10 — Storage And Performance Gates

Track:

```text
micro_nodes per file
micro_edges per file
local_flow_packets per function
sidecar bytes per file
sidecar bytes per entity
packet generation p95
snippet loading p95
candidate packet/index bytes
query/status latency over sidecar packet indexes
micro-flow retrieval candidate count
optimizer-tuned policy replay metrics if used
DB size delta vs MVP2
update invalidation time
```

Targets should be set after baseline measurement. Do not invent pass thresholds without evidence.

---

## 15. MVP4.11 — Agent Packet Integration

MVP4 micro-flow packets should plug into the Agent Routing Packet:

```text
routing packet critical symbol -> micro-flow packet handle
edit plan step -> micro-flow proof evidence
validation step -> micro-flow relation check
hard interrupt -> micro-flow broken relation
```

First packet should remain compact:

```text
include short micro-flow summary
provide expansion handle for full local skeleton
```

---

## 16. MVP4.13 — Framework-Aware Route Proof Edges

### Purpose

Make web-framework routes first-class, **proof-backed** graph edges:
`route pattern (+ HTTP method) -> handler symbol`. An agent asking "what handles
`POST /users`?" should get a source-spanned exact answer, not a text guess; and
`callers`/`impact` should treat a route as a real entry point into the handler.

This is the typed-graph answer to convenience-tool "framework routing": we add
the edge **and** the honesty layer those tools omit.

### Proof Boundary

```text
exact        route literal AND handler symbol both source-spanned, registration is
             static, binding is direct (literal path -> direct function/method)
heuristic    handler resolved by convention/name (e.g. Rails controller#action),
             regex/param route, or string-proximity registration
unsupported  computed/dynamic route string, runtime-registered handler, reflection
unknown      framework adapter not present for this language/file
```

A route edge NEVER becomes graph proof from a name match, a comment, or a docs
string alone.

### New Edge Kinds

```text
ROUTES_TO       route-literal node -> handler entity   (primary)
MOUNTS_ROUTER   parent router/app -> mounted sub-router/blueprint/prefix
```

Builds on MVP4.1 `RouteLiteral` plus a new `RouteBinding` micro-node (the
decorator / registration call / route-table row that ties literal to handler).

### Framework Adapters (pluggable, fixture-gated, tiered)

```text
Tier 1 (literal registration -> direct handler, exact-capable):
  FastAPI / Flask     @app.get("/x") / @app.route                  (Python)
  Django              urls.py path("x/", view)                     (Python)
  Express / Koa       app.get("/x", handler)                       (JS/TS)
  NestJS              @Get("/x") on a controller method            (TS)
  Spring              @GetMapping("/x")                            (Java)
  Actix / Axum        route!/.route("/x", get(handler))            (Rust)
Tier 2 (convention-based -> heuristic):
  Rails               routes.rb -> ControllerName#action
  Laravel             Route::get('/x', [C::class,'m'])
```

Each adapter is a self-contained module declaring: `framework_id`, detection
signal, route-literal extraction, handler-symbol resolution, and its exactness
rules. No framework gets an `exact` claim without fixtures.

### Required Fields (route fact / ROUTES_TO edge)

```text
route_edge_id
framework_id
http_method                  (or "unknown")
route_pattern                (literal text)
route_pattern_normalized
handler_entity_id            (if resolved)
handler_source_span_id
route_literal_source_span_id
binding_kind                 decorator|registration_call|macro|route_table
exactness                    exact|heuristic|unsupported
provenance
claimability
```

### Forbidden Behavior

```text
no route edge from comments or docs strings alone
no exact handler binding from name match without symbol resolution
no exact method (GET/POST) when the method is computed
no route edge across an unverified dynamic mount
```

### Agent Surfaces

```text
query route "POST /users"     -> exact handler+span, or candidate/unknown
query routes-of <handler>     -> routes mapped to a handler
callers/impact/route-aware    -> a route is an entry point into its handler
context-pack/context-entry    -> seeds may be route patterns
```

### Cross-MVP Hooks

```text
MVP3.10 validates route edges post-edit (dangling handler, removed route, method change)
MVP4.7  micro-flow may extend a route edge into the handler's local auth/sanitizer flow
MVP4.15 one-call context surfaces route entry points
```

### Fixtures & Gate

```text
literal route -> direct handler                 (exact)
renamed handler                                 (exact -> dangling, validated by MVP3.10)
convention-based controller#action              (heuristic)
regex/param route                               (heuristic)
computed/dynamic route string                   (unsupported)
nested router/blueprint mount                    (MOUNTS_ROUTER exact)
negative: comment/docs-only "route"             (forbidden, no edge)
```
Gate passes when: exactness labels are correct per fixture, spans are present on
exact edges, no exact claim exists for an unfixtured framework, and forbidden
cases emit no edge.

---

## 17. MVP4.14 — Cross-Language Bridge Edges

### Purpose

Synthesize call/reference edges across language boundaries **where a real,
declared binding exists**, so an agent tracing a call does not dead-end at the
language boundary (Swift↔Objective-C, React Native JS↔native, Expo/Tauri/Electron
IPC, C FFI / Rust `extern "C"`, JNI, WASM imports). This is the hardest feature
to keep honest, so the proof rules are strict.

### Proof Boundary (strict — most bridges are heuristic, and that is fine)

```text
exact        a declared, source-spanned binding ties BOTH sides:
             @objc / bridging-header / NS_SWIFT_NAME symbol on both sides;
             extern "C" symbol matching an FFI declaration;
             JNI native method <-> Java_<sig> C symbol with matching signature;
             WASM import/export name match
heuristic    name/convention match across sides with no compiler-enforced binding
             (RN NativeModules.X.method <-> RCT_EXPORT_METHOD; IPC channel string)
unsupported  dynamic string-keyed dispatch with no static registration; reflection;
             codegen not available
unknown      one side is outside indexed scope -> target = "unindexed_side"
```

A cross-language bridge NEVER upgrades to graph proof without a declared binding.
The product value is **honest navigation across boundaries with correct
confidence labels**, not a claim of complete cross-language call graphs.

### New Edge Kind

```text
BRIDGES_TO   source-side symbol -> target-side symbol, carrying bridge_kind
```

### Bridge Adapters (tiered)

```text
Tier 1 declared bindings (exact-capable):
  swift_objc   @objc / bridging header / NS_SWIFT_NAME
  ffi_extern   Rust extern "C" / C header symbol match
  jni          Java native method <-> Java_<package>_<class>_<method> C symbol
  wasm         import/export table name match
Tier 2 framework conventions (heuristic):
  rn_native_module   NativeModules.X.method <-> RCT_EXPORT_METHOD / @ReactMethod
  expo_module        Expo module function registration
  ipc_channel        Electron/Tauri channel string <-> registered handler
```

### Required Fields

```text
bridge_edge_id
bridge_kind          swift_objc|ffi_extern|jni|wasm|rn_native_module|expo_module|ipc_channel
source_lang / target_lang
source_entity_id / source_span_id
target_entity_id / target_span_id   (or target = unindexed_side)
binding_evidence     declared_binding|export_macro|manifest|name_convention
exactness            exact|heuristic|unsupported
provenance
claimability
```

### Forbidden Behavior

```text
no exact bridge from name match alone
no exact bridge across a string channel without a registered handler
no synthesized native call when the target side is not indexed (use unindexed_side/unknown)
no collapsing of distinct overloads across the boundary
```

### Cross-MVP Hooks

```text
MVP3.10 validates bridges post-edit (declared-binding side removed/renamed)
MVP4.15 one-call context may surface a bridge as a labeled neighbor
```

### Fixtures & Gate

```text
swift/objc @objc method                          (exact)
rust extern "C" matching header                  (exact)
JNI native <-> C symbol                          (exact)
RN NativeModules method name match               (heuristic)
IPC channel string -> handler                    (heuristic)
computed/string-keyed dispatch                   (unsupported)
target side outside indexed scope                (unknown / unindexed_side)
negative: name match across sides claimed exact  (forbidden)
```
Gate passes when: only declared, source-spanned bindings are `exact`; convention
matches are `heuristic`; unindexed targets are `unknown`; and the forbidden cases
do not produce exact edges.

---

## 18. MVP4.15 — One-Call Context Entry Packet

### Purpose

Match (and, with proof labels, exceed) the convenience-tool ergonomic of "one
tool call returns entry points + snippets, no exploration agent needed" — while
keeping the proof boundary. A single call (`context-entry` CLI / `codegraph_context`
MCP) returns resolved seeds, entry points, a bounded proof-labeled neighbor set,
micro-flow handles, and a candidate fallback — in one compact, byte-budgeted,
proof-labeled envelope.

This is an **aggregation / ergonomics layer** over MVP2 context-pack + MVP4
micro-flow + MVP4.13 routes. It adds **no new proof source**; proof strength
still comes from the underlying typed edges.

### Packet Shape

```json
{
  "packet_kind": "context_entry_packet",
  "seeds": [],
  "entry_points": [],
  "proof_neighbors": [],
  "candidate_neighbors": [],
  "micro_flow_handles": [],
  "no_proof_path_found": false,
  "budget": { "max_output_bytes": 16384, "omitted_count": 0 }
}
```

Field rules:

```text
seeds                resolved symbols/routes with source spans
entry_points         main/exported/route(MVP4.13)/test entry, each proof-labeled
proof_neighbors      bounded exact graph edges with spans
candidate_neighbors  text/vector recall, labeled candidate (never proof)
micro_flow_handles   MVP4.7 expansion handles (no inlined full bodies)
no_proof_path_found  true when no exact path exists, with labeled fallback
```

### Requirements

```text
one call, no agent round-trips
everything proof-labeled; candidate never presented as proof
respects existing context budgets (default 16 KiB, same enforcer)
deterministic ordering
never fabricates an entry point or a neighbor
```

Boundary: this is a presentation/aggregation surface; it must reuse the existing
envelope compaction and proof-ladder labels, not redefine them.

---

## 19. MVP4.16 — Distribution & Zero-Friction Onboarding

> Distribution track, **not a proof feature**. It ships no new graph capability
> and makes no code-correctness claim. It exists to close the adoption gap vs
> zero-build (npx/Node) competitors without weakening the agent-use profile.

### Purpose

Remove the biggest adoption gap vs convenience competitors (a one-line install
vs `cargo build --release`): ship prebuilt, verifiable release binaries + a
one-command installer + automatic MCP wiring to the production agent-use profile.

### Scope

```text
prebuilt release binaries per platform/arch
  (win x64/arm64, macOS x64/arm64, linux x64/arm64)
  attached to GitHub Releases with checksums + signing
one-line installer (PowerShell + POSIX shell) that:
  fetches the correct binary, verifies checksum/signature, places it on PATH
`codegraph-mcp init` / `agent-use mcp-config` auto-writes MCP config for
  Claude Code / Cursor / Codex, bound to the production agent-use profile
  (external DB in LocalAppData, read-mostly, lifecycle/passport guards)
version / self-update CHECK only (report new version; never auto-replace)
release CI: build matrix + provenance/attestation so binaries are verifiable
```

### Non-Goals / Boundaries

```text
no telemetry / phone-home
no install-time code execution beyond placing a verified binary
installer must not weaken agent-use defaults (external DB, read-mostly, passport)
binary provenance is a supply-chain property, NOT a code-correctness/proof claim
this track is orthogonal to the proof model and ships no graph capability
```

### Gate

```text
checksum + signature verification tests
installer idempotency (re-run is safe, no PATH duplication)
MCP config correctness: points at agent-use profile, exposes no source-edit tool
cross-platform smoke test: --version, languages, agent-use status
```

---

## 20. MVP4.17 — MVP4 Final Gate

Required checks:

```text
cargo build --workspace
cargo build --release --bin codegraph-mcp
micro-node fixture tests
micro-edge fixture tests
local-flow packet tests
storage budget tests
claimability tests
negative no-false-proof tests
Stage 0 regression
routing packet regression
MVP3 validate-edit regression
language matrix regression
framework route proof-edge fixture tests
cross-language bridge fixture tests
one-call context entry packet tests
distribution installer + checksum/signature tests
JSON validation
PRODUCT_READINESS update if appropriate
```

Pass criteria:

```text
micro-node extraction is source-spanned
micro-edge extraction is source-spanned and provenance-backed
local micro-flow packets are compact and bounded
unsupported dynamic/macro behavior is not exact
core DB tables are not bloated
sidecar storage is bounded
routing packets can reference micro-flow expansion handles
no proof overclaim
```

---

## 21. MVP4 End State

MVP4 is complete when CodeGraph can honestly say:

```text
For a complex function or file, CodeGraph can return a compact, deterministic, source-spanned micro-flow packet showing the literal local mechanics the developer wrote, without relying on lexical search or AI inference.
```

The target loop becomes:

```text
agent asks about function behavior
  -> CodeGraph returns local micro-flow packet
  -> agent inspects exact steps/spans
  -> edit or validate with structural proof
```

---

## 22. Enterprise Shared MCP RBAC and Policy Controls

This is an enterprise/shared-server product surface, not a requirement for local
single-user or local multi-agent workflows.

Local multi-agent use remains profile/process isolation:

```text
separate repo
separate external profile DB
separate generated MCP config
lifecycle/passport guards
no cross-repo fallback
```

Enterprise RBAC begins only after local multi-repo profile isolation, production
agent-use profile durability, and Real-Time Delta Sync are stable.

Required capabilities:

```text
actor identity for users, sessions, service accounts, and agents
tenant/project/repo binding
repo allowlists
path allowlists
tool-level read/write/admin scopes
policy-denied structured errors
audit logs with actor, repo, profile, tool, action, and decision
secret/path redaction policy
cross-repo leakage prevention
admin recovery and break-glass path
policy simulation/dry-run mode
```

Non-goals:

```text
not required for local one-agent-per-repo use
not a replacement for DB lifecycle/passport claimability
no weakening of proof/evidence labels
no silent fallback from a denied repo/profile to another DB
no public enterprise-security claim without a dedicated gate
```

Acceptance gate:

```text
policy allow and deny tests for every MCP tool class
repo/path boundary tests
cross-profile leakage tests
audit-log completeness tests
redaction tests
locked/stale/missing DB behavior under policy
admin recovery tests
dry-run policy simulation tests
docs clearly distinguish local profile isolation from enterprise RBAC
PRODUCT_READINESS updated only after verified behavior exists
```
