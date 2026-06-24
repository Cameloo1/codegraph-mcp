# MVP4 Proof Model Contract

Status: pre-MVP4 preparation contract. This document freezes terminology,
exactness, claimability, and prohibited proof transitions before MVP4 storage or
parser design proceeds. It does not activate MVP4 extraction, packet fields, or
production behavior.

Current gate: `reports/final/pre_mvp4_prerequisite_start_policy_gate.json`
allows design/preparation and additive inactive scaffolding, but blocks
production MVP4.1 implementation while Benchmark v1 real external patch tasks
remain unrun and the human start policy is unchanged.

## Existing Names

MVP4 reuses the existing public/domain vocabulary instead of replacing it.

- `Exactness` is defined in `crates/codegraph-core/src/kinds.rs` with
  `exact`, `compiler_verified`, `lsp_verified`, `parser_verified`,
  `static_heuristic`, `dynamic_trace`, `inferred`, and
  `derived_from_verified_edges`.
- `EvidenceRole` is defined in `crates/codegraph-core/src/kinds.rs` with
  `production`, `test`, `mock`, `mixed`, and `unknown`. Public docs mention
  additional text labels such as generated/stub, but MVP4 must not add active
  role variants without a separate schema/version decision.
- `SourceSpan` is defined in `crates/codegraph-core/src/model.rs` as
  repo-relative path plus line/column range. A claimable local source fact must
  carry a source span, except for explicitly labeled non-graph source-text
  facts.
- Edge provenance already exists as `derived`, `provenance_edges`, and
  metadata on `Edge`. Derived proof requires provenance.
- `PathEvidence` carries relation path, source spans, aggregate exactness,
  confidence, and metadata. It is graph relation evidence, not automatic proof
  of mutation or flow.
- `NormalizedClaimabilityMetadata` defines current claimability as
  `graph_proof`, `claimable`, `proof_status`, and `reason`.
- `ValidationProofStatus`, `ValidationEvidenceKind`, and
  `ValidationFinding` define validation classifications, proof status,
  source-span/provenance requirements, and packet proof fields.
- Dormant sparse sidecar tables already reserve `ast_micro_nodes`,
  `ast_micro_edges`, and `local_flow_packets` with source-span/provenance
  checks. These tables are inactive preparation, not MVP4 behavior.

## Canonical Terms

`AST Quantization`: deterministic extraction of local AST structure into sparse
micro-nodes, micro-edges, local micro-flow packets, and compressed AST
skeletons. It is a source-structure proof model, not learned semantic proof.

`micro_node`: a source-spanned local AST unit such as a binding, literal,
assignment, call site, return site, mutation site, branch guard, or local value
slot. Existence may be claimable when deterministic extraction and the source
span are present. Binding identity is claimable only when the current resolver
proves the binding. A literal's existence does not prove route, auth,
sanitizer, or security semantics.

`micro_edge`: a bounded local relation between two micro-nodes. Both endpoints
must be source-spanned. Direct AST linkage may be exact. Locally derived
sequence or assignment linkage must carry provenance. Pattern-based linkage is
heuristic. Runtime, dynamic, macro, reflection, and framework convention remain
unsupported or unknown unless separately resolved.

`local_micro_flow`: an ordered, bounded packet of exact or
derived-with-provenance micro-edges inside a local scope, usually a function or
method. It proves only the local mechanics represented by its steps. It does
not imply complete interprocedural behavior.

`AST skeleton`: a compressed local source representation that keeps names,
bindings, calls, reads/writes, assignments, returns, mutations, literal keys,
branch guards, and source spans while dropping syntactic noise. A skeleton is
not proof unless every proof-bearing step maps back to source-spanned
micro-nodes/micro-edges.

`mutation_proof`: future proof level for deterministic local write/mutation
mechanics. It remains inactive until the MVP4 gate activates packet semantics.

`flow_proof`: future proof level for deterministic source-spanned local flow
chains. It remains inactive until the MVP4 gate activates packet semantics.

`exact`: current `Exactness` values `exact`, `compiler_verified`,
`lsp_verified`, or `parser_verified` when all required source spans,
source-role, lifecycle, and resolver/provenance checks are satisfied.

`derived_with_provenance`: MVP4 label for current
`derived_from_verified_edges` when the fact is derived from verified local
facts and carries explicit provenance (`provenance_edges` or sidecar
`provenance_id`).

`heuristic`: current `static_heuristic` or `inferred`, plus pattern or
convention matches without resolver proof. Heuristic facts can orient an agent
but do not prove graph, mutation, or flow claims.

`unsupported`: the extractor or runtime surface does not support the required
relation. Unsupported facts must stay non-claimable or unknown; they cannot be
papered over by text/candidate evidence.

`unknown`: current `unknown` proof/status role when the indexed evidence does
not decide the question. Unknown is a stable answer, not a failure to invent a
claim.

`unindexed_side`: sentinel for a bridge or route target whose counterpart is
outside indexed scope. It produces `unknown`, not an exact edge.

`runtime_or_artifact_required`: label for claims that depend on DB rows,
generated JSON, generated binaries, Docker/SWE-bench state, network services,
model behavior, macro expansion output, or runtime reflection. Source proof
does not satisfy this label.

`claimable local fact`: a current-lifecycle, source-spanned local fact whose
exactness and role meet the relevant graph/source requirement. Claimability is
bounded to the stated fact.

`claimable local flow`: a future local micro-flow whose every proof-bearing
step is exact or derived-with-provenance, source-spanned, lifecycle-current, and
bounded. It does not claim runtime values or global program behavior.

`candidate micro-flow retrieval`: retrieval over micro-flow handles or features.
It is candidate evidence until graph/source verification returns a claimable
micro-flow packet.

## Exactness Decision Table

| MVP4 label | Existing code names | Claimable when | Must not claim |
|---|---|---|---|
| `exact` | `exact`, `compiler_verified`, `lsp_verified`, `parser_verified` | Source span is present, lifecycle is claimable, source role is allowed, and the direct AST/resolver relation proves the binding or local relation. | Runtime values, framework convention, generated artifact values, complete interprocedural behavior, route/auth/security semantics from literals alone. |
| `derived_with_provenance` | `derived_from_verified_edges` with `derived=true` and provenance | Every base fact is exact/proof-grade, provenance ids are present, source spans cover the derived step, and the derivation is local and deterministic. | Direct AST exactness, global dataflow, or any fact with missing provenance. |
| `heuristic` | `static_heuristic`, `inferred`, pattern/convention match | Agent orientation, candidate ranking, warnings, or unknown/degraded validation. | Graph relation proof, mutation proof, flow proof, blocking proof unless separately reverified by graph/source evidence. |
| `unsupported` | Validation classification/status or dormant sidecar exactness value | The system can state that the relation is not statically supported. | An inferred proof or silent pass. |
| `unknown` | `EvidenceRole::Unknown`, `ValidationProofStatus::Unknown`, `RetrievalProofStatus::Unknown`, `EdgeClass::Unknown` | The system can honestly state that evidence does not decide the claim. | Any proof upgrade without new resolver/source/artifact evidence. |

## Evidence To Proof Transitions

| Evidence rung | Can become | Required transition evidence | Forbidden silent upgrade |
|---|---|---|---|
| `text_evidence` | source-text existence claim | bounded source span or text row plus lifecycle-current DB | graph relation proof, mutation proof, flow proof |
| `symbol_evidence` | candidate binding or claimable entity existence | resolver-confirmed source-spanned entity for binding identity | route/auth/sanitizer/security semantics |
| `candidate_evidence` | candidate for verification | graph/source verification over current DB | proof status by ranking, vector similarity, or candidate count |
| `source_navigation_evidence` | walkthrough context | explicit source spans and labels | local flow proof without micro-edge extraction |
| `graph_relation_proof` | current graph/source proof | proof-grade edges, source spans, lifecycle, allowed source role, provenance for derived edges | runtime/artifact values or complete flow semantics |
| `mutation_proof` | future local mutation proof | active MVP4 gate plus deterministic source-spanned local mutation micro-edges | any current packet field or candidate evidence |
| `flow_proof` | future local flow proof | active MVP4 gate plus bounded exact/derived local micro-flow packet | complete interprocedural/runtime behavior |
| `unknown` / `unsupported` | non-claimable status | explicit status and reason | proof by omission or fallback text |

No lower rung may silently upgrade itself.

## Runtime And Artifact Boundary

MVP4 mathematical proof delivery means deterministic local graph proof with
spans, provenance, exactness, bounded evidence, and claimability. It explicitly
excludes generic formal verification, SMT proof, symbolic execution, runtime
proof, and arbitrary patch correctness.

Local source proof does not prove runtime DB contents, generated artifacts,
external services, Docker state, network state, model behavior, macro expansion
output, framework routing, dynamic dispatch, reflection, or values produced at
runtime. Those require separately labeled artifact, runtime, expansion, or
external evidence.

## Forbidden Proof Transitions

- A literal's existence must not prove route, auth, sanitizer, or security
  semantics.
- Text evidence, comments, docs, ranker output, or vector/binary/nuance
  candidates must not become graph proof.
- Source navigation evidence must not become mutation or flow proof without
  source-spanned micro-edges.
- Derived edges without provenance must not be claimable.
- Missing source spans must block claimable graph facts except explicit
  source-text-only claims.
- Stale, foreign, corrupt, schema-mismatched, or diagnostic DB reads are
  non-claimable.
- Dynamic dispatch, reflection, runtime dependency injection, monkeypatching,
  macro/preprocessor expansion, or framework convention must remain heuristic,
  unsupported, or unknown unless a resolver or expansion source proves the
  relation.
- Route and bridge name matches alone are never exact.
- `unindexed_side` is unknown, not proof.
- Validation severity is an interpretation of evidence, not proof by itself.

## Terminology Mapping

| Surface | Contract |
|---|---|
| Parser | May eventually extract micro-node/micro-edge facts only when source-spanned and deterministic. Until MVP4 is authorized, no production parser emission of MVP4 facts. |
| Store | Existing sparse sidecar table contracts are inactive, additive, and backward-compatible. Old DB compatibility must remain safe. |
| Query | Candidate micro-flow retrieval remains candidate evidence until graph/source verification produces a bounded local micro-flow packet. |
| CLI | `proof_status`, `proof_strength`, `claimability`, and proof-ladder counts must keep text/symbol/candidate/source-navigation/graph/mutation/flow rungs distinct. |
| MCP | MCP surfaces mirror CLI proof boundaries and must not expose inactive MVP4 fields as active proof. |
| Docs | Public docs may describe this contract, but docs/text do not prove graph facts. |
| Tests | Contract tests may validate inactive JSON/schema artifacts and current proof-ladder mappings. They must not require active MVP4 packet fields before the implementation gate. |

## Activation Invariants

- `mutation_proof` remains inactive.
- `flow_proof` remains inactive.
- No production micro-node emission begins.
- No production micro-edge emission begins.
- No production local micro-flow packet emission begins.
- No route/bridge extractor activation begins.
- No context-entry command activation begins.
- No distribution publication begins.
