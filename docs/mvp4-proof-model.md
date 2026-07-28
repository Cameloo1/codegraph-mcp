# MVP4 Proof Model Contract

Status: current proof contract through MVP4.3L all-language static-flow
readiness. The release-binary representative gate accepts source-aware,
same-file intraprocedural local-flow behavior for all 13 canonical production
frontend adapters:

- JavaScript
- JSX
- TypeScript
- TSX
- Python
- Go
- Rust
- Java
- C#
- C
- C++
- Ruby
- PHP

The live sources of truth are codegraph-mcp languages --json,
codegraph://languages, and the Scoped Tier 5 MVP4 Readiness section of
docs/language-frontends.md. The promotion contract is
mvp4_3l_all_language_static_flow_readiness_v3, run by
mvp4_language_readiness against
fixtures/mvp4_micro_flow_oracles/manifest.json.

This is a bounded readiness statement. It does not claim whole-language,
cross-file, runtime, dynamic, framework, compiler, LSP, project-resolution,
macro/preprocessor, build-database, or global-program completeness. It also
does not start MVP4.4 AST skeleton compression, mutation proof, route/bridge
proof, context-entry command activation, or distribution publication.
public_claim=false and real_agent_patch_quality_claim=false.

## What Tier 5 Means

Tier 5 is backward-compatible frontend classification metadata. It says the
frontend exposes the dataflow/security/test-impact tier of the registry. It is
not, by itself, proof of every Tier 0 through Tier 5 behavior and it does not
activate linter blocking.

The authoritative per-language truth is the combination of:

- frontends[].capabilities for broad frontend capability status
- frontends[].scoped_readiness for narrower accepted proof boundaries
- the named representative promotion gate for release readiness

A conservative or unsupported language_frontend capability row does not erase
a supported same_file_intraprocedural row. Conversely, a supported scoped row
must never be promoted into a claim of broad compiler, runtime, framework,
project, cross-file, or whole-language support. Every capability status remains
meaningful: supported_exact, supported_derived_with_provenance,
supported_parser_only, supported_heuristic, warning_only, diagnostic_only,
unknown, unsupported, not_applicable, not_implemented, requires_compiler,
requires_lsp, requires_runtime, requires_macro_expansion,
requires_preprocessor, and requires_build_database.

For each canonical adapter, the accepted same_file_intraprocedural contract is:

| Capability | Accepted status | Meaning |
|---|---|---|
| local_binding_resolved | supported_exact | Binding identity is exact only inside the accepted local scope. |
| read_write_extracted | supported_exact | Source-spanned local reads and writes are extracted inside that scope. |
| local_dataflow_derived | supported_derived_with_provenance | Local flow derivation is accepted only with explicit provenance. |
| local_flow_packet_supported | supported_exact | A bounded persisted packet may carry claimable local flow when every packet invariant passes. |

TypeScript has an additional extension boundary. The representative readiness
fixture executes .mts. Production .mts and .cts use ParserFactsV1 for this
contract; ordinary .ts remains on the bounded legacy v1 path, and .d.ts is
inactive. This extension boundary must not be rewritten as whole-TypeScript
compiler or project-resolution support.

## Existing Names

MVP4 reuses the existing public/domain vocabulary instead of replacing it.

- Exactness is defined in crates/codegraph-core/src/kinds.rs with exact,
  compiler_verified, lsp_verified, parser_verified, static_heuristic,
  dynamic_trace, inferred, and derived_from_verified_edges.
- EvidenceRole is defined in crates/codegraph-core/src/kinds.rs with
  production, test, mock, mixed, and unknown. Public docs mention additional
  text labels such as generated or stub, but MVP4 must not add active role
  variants without a separate schema/version decision.
- SourceSpan is defined in crates/codegraph-core/src/model.rs as repo-relative
  path plus line/column range. A claimable local source fact must carry a source
  span, except for explicitly labeled non-graph source-text facts.
- Edge provenance already exists as derived, provenance_edges, and metadata on
  Edge. Derived proof requires provenance.
- PathEvidence carries relation path, source spans, aggregate exactness,
  confidence, and metadata. It is graph relation evidence, not automatic proof
  of mutation or flow.
- NormalizedClaimabilityMetadata defines current claimability as graph_proof,
  claimable, proof_status, and reason.
- ValidationProofStatus, ValidationEvidenceKind, and ValidationFinding define
  validation classifications, proof status, source-span/provenance
  requirements, and packet proof fields.
- Sparse sidecar tables ast_micro_nodes, ast_micro_edges, and
  local_flow_packets are active for accepted production ParserFactsV1 slices
  across the canonical adapters. Their availability remains separate from core
  graph claimability. A frontend name, Tier 5 label, capability row, handle, or
  table row never creates proof by itself.

## Canonical Terms

**AST Quantization** is the roadmap family for deterministic extraction of
local AST structure into sparse micro-nodes, micro-edges, local micro-flow
packets, and eventually compressed AST skeletons. The active MVP4.1-MVP4.3L
portion ends at persisted local-flow packets. MVP4.4 AST skeleton compression
is not active.

**micro_node** is a source-spanned local AST unit such as a binding, literal,
assignment, call site, return site, mutation site, branch guard, or local value
slot. Existence may be claimable when deterministic extraction and the source
span are present. Binding identity is claimable only when the accepted local
resolver contract proves the binding. A literal's existence does not prove
route, auth, sanitizer, or security semantics.

**micro_edge** is a bounded local relation between two micro-nodes. Both
endpoints must be source-spanned. Direct AST linkage may be exact. Locally
derived sequence or assignment linkage must carry provenance. Pattern-based
linkage is heuristic. Runtime, dynamic, macro, reflection, and framework
convention remain unsupported or unknown unless separately resolved.

**local_micro_flow** is an ordered, bounded packet of exact or
derived-with-provenance micro-edges inside an accepted local scope, usually a
function or method. It proves only the local mechanics represented by its
steps. It does not imply complete interprocedural behavior.

**dict_v1 packet representation** is the active lossless dictionary
representation for local micro-flow packet bodies. Source spans, micro-node
refs, micro-edge refs, provenance, branch identities, return-path identities,
labels, and repeated proof metadata may be interned once and referenced by
ordered path steps. It must preserve source spans, provenance, exactness,
source roles, shadowed binding identity, unknown/unsupported gaps, branch
distinctions, return-path distinctions, and cap omissions. Verbose
ordered_steps are an explain/audit expansion of the same facts, not a stronger
proof source.

dict_v1 is packet representation only. It is not the MVP4.4 AST skeleton
compression feature and must not be used to claim that MVP4.4 has started.

**AST skeleton** is the future compressed local source representation intended
to keep names, bindings, calls, reads/writes, assignments, returns, mutations,
literal keys, branch guards, and source spans while dropping syntactic noise.
It remains inactive. A future skeleton will not be proof unless every
proof-bearing step maps back to source-spanned micro-nodes and micro-edges.

**mutation_proof** is the future proof level for deterministic local
write/mutation mechanics. It remains inactive. Current packets may report
local write/mutation relations as graph or local-flow evidence, but they do not
activate mutation proof.

**flow_proof** is active for a canonical production adapter only inside its
accepted representative, source-aware, same-file intraprocedural contract, and
only when every proof-bearing packet step is exact or
derived-with-provenance, source-spanned, lifecycle-current, role-eligible,
bounded, and complete. It is downgraded for partial, stale, foreign, corrupt,
schema-mismatched, truncated, unsupported, parser-recovery, non-production,
dynamic, macro, runtime, gap-bearing, or omission-bearing states.

**exact** means current Exactness values exact, compiler_verified,
lsp_verified, or parser_verified only when all required source-span,
source-role, lifecycle, scope, and resolver/provenance checks are satisfied.

**derived_with_provenance** is the MVP4 label for
derived_from_verified_edges when a fact is derived from verified local facts
and carries explicit provenance through provenance_edges or a sidecar
provenance_id.

**heuristic** means current static_heuristic or inferred, plus pattern or
convention matches without resolver proof. Heuristic facts can orient an agent
but do not prove graph, mutation, or flow claims.

**unsupported** means the extractor or runtime surface does not support the
required relation. Unsupported facts must stay non-claimable or unknown; they
cannot be papered over by text or candidate evidence.

**unknown** means the indexed evidence does not decide the question. Unknown is
a stable answer, not a failure to invent a claim.

**unindexed_side** is a sentinel for a bridge or route target whose counterpart
is outside indexed scope. It produces unknown, not an exact edge.

**runtime_or_artifact_required** labels claims that depend on DB rows,
generated JSON or binaries, Docker/SWE-bench state, network services, model
behavior, macro expansion output, or runtime reflection. Source proof does not
satisfy this label.

**claimable local fact** is a current-lifecycle, source-spanned local fact whose
exactness, role, and scope meet the relevant graph/source requirement.
Claimability is bounded to the stated fact.

**claimable local flow** is a production local micro-flow for one of the 13
canonical adapters whose every proof-bearing step is exact or
derived-with-provenance, source-spanned, lifecycle-current, role-eligible,
bounded, and complete under the adapter's accepted
same_file_intraprocedural contract. It does not claim runtime values, framework
semantics, cross-file behavior, or global program behavior.

**candidate micro-flow retrieval** is retrieval over micro-flow handles or
features. It remains candidate evidence until graph/source verification opens
and verifies a claimable persisted packet. Handles do not create proof.

## Exactness Decision Table

| MVP4 label | Existing code names | Claimable when | Must not claim |
|---|---|---|---|
| exact | exact, compiler_verified, lsp_verified, parser_verified | Source span is present, lifecycle is claimable, source role is allowed, and the direct AST/resolver relation proves the binding or local relation inside its advertised scope. | Runtime values, framework convention, generated artifact values, complete interprocedural behavior, or route/auth/security semantics from literals alone. |
| derived_with_provenance | derived_from_verified_edges with derived=true and provenance | Every base fact is proof-grade, provenance ids are present, source spans cover the derived step, and the derivation is local and deterministic. | Direct AST exactness, global dataflow, or any fact with missing provenance. |
| heuristic | static_heuristic, inferred, pattern/convention match | Agent orientation, candidate ranking, warnings, or unknown/degraded validation. | Graph relation proof, mutation proof, flow proof, or blocking proof unless separately reverified by graph/source evidence. |
| unsupported | Capability or validation status | The system can state that the relation is not statically supported. | An inferred proof or silent pass. |
| unknown | Unknown proof/status role | The system can honestly state that evidence does not decide the claim. | Any proof upgrade without new resolver/source/artifact evidence. |

## Evidence To Proof Transitions

| Evidence rung | Can become | Required transition evidence | Forbidden silent upgrade |
|---|---|---|---|
| text_evidence | source-text existence claim | Bounded source span or text row plus lifecycle-current DB. | Graph relation proof, mutation proof, or flow proof. |
| symbol_evidence | candidate binding or claimable entity existence | Resolver-confirmed source-spanned entity for binding identity. | Route/auth/sanitizer/security semantics. |
| candidate_evidence | candidate for verification | Graph/source verification over the current DB. | Proof status from ranking, vector similarity, or candidate count. |
| source_navigation_evidence | walkthrough context | Explicit source spans and labels. | Local-flow proof without micro-edge extraction. |
| graph_relation_proof | current graph/source proof | Proof-grade edges, source spans, lifecycle, allowed source role, advertised scope, and provenance for derived edges. | Runtime/artifact values or complete-flow semantics. |
| mutation_proof | future local mutation proof | A future active gate plus deterministic source-spanned local mutation micro-edges. | Any current packet field or candidate evidence. |
| flow_proof | current bounded local-packet proof | An opened and verified production packet for a canonical adapter inside the accepted same-file scope, with complete exact/derived local steps and no proof gaps or cap omissions. | Complete interprocedural/runtime behavior, scope promotion, non-production roles, unsupported surfaces, or partial/gap-bearing packets. |
| unknown or unsupported | non-claimable status | Explicit status and reason. | Proof by omission or fallback text. |

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
- A frontend's Tier 5 label must not become proof or linter blocking.
- A narrower scoped_readiness row must not overwrite or broaden a conservative
  language_frontend capability row.
- A packet handle, packet row, or capability metadata must not create proof
  without opening and verifying the current persisted packet.
- Derived edges without provenance must not be claimable.
- Missing source spans must block claimable graph facts except explicit
  source-text-only claims.
- Stale, foreign, corrupt, schema-mismatched, unsafe, or diagnostic DB reads are
  non-claimable.
- Dynamic dispatch, reflection, runtime dependency injection, monkeypatching,
  macro/preprocessor expansion, or framework convention must remain heuristic,
  unsupported, or unknown unless a resolver or expansion source proves the
  relation.
- Route and bridge name matches alone are never exact.
- unindexed_side is unknown, not proof.
- Validation severity is an interpretation of evidence, not proof by itself.

## Terminology Mapping

| Surface | Contract |
|---|---|
| Parser | May extract source-spanned local micro-node/micro-edge facts for the canonical adapters only inside each adapter's implemented extension and same-file ParserFactsV1 boundary. Broad compiler/runtime/project semantics remain governed by capability status. |
| Store | Sparse sidecar tables hold accepted local packet state and remain an optional layer separate from core graph claimability. Old DB compatibility must remain safe. |
| Query | Candidate micro-flow retrieval remains candidate evidence until graph/source verification opens a bounded current packet. Compact query output is handle-first. |
| CLI | languages --json exposes the v3 capabilities-and-scoped-readiness truth; proof_status, proof_strength, claimability, and proof-ladder counts keep evidence rungs distinct. |
| MCP | codegraph://languages mirrors the registry. Packet query/open access is active, but the separate context-entry command remains inactive. |
| Docs | Public docs may describe this contract, but docs and generated reports do not prove graph facts. |
| Tests and gates | The representative release gate verifies each canonical adapter's accepted same-file contract and extension boundary. It does not establish broad whole-language support. |

## Activation Invariants

- All 13 canonical production adapters have accepted, source-aware,
  same_file_intraprocedural local-binding, read/write, derived-dataflow, and
  local-flow-packet rows under the v3 readiness contract.
- Broad frontends[].capabilities rows retain their own status and scope.
  scoped_readiness supplements them and never flattens or promotes them.
- flow_proof is limited to opened, current, complete, claimable production
  packets inside the advertised same-file scope. Test, generated, vendor,
  unknown, partial, stale, corrupt, gap-bearing, and omission-bearing states
  remain non-claimable or downgraded.
- Production micro-node, micro-edge, and packet emission is bounded by each
  adapter's implemented extension and ParserFactsV1 contract. Readiness is not
  a claim of cross-file or whole-program flow.
- TypeScript readiness uses .mts as the representative gate path. .mts/.cts use
  ParserFactsV1; ordinary .ts remains bounded legacy v1; .d.ts remains inactive.
- dict_v1 remains active packet representation only. MVP4.4 AST skeleton
  compression has not started.
- mutation_proof remains inactive.
- Route/bridge proof extraction remains inactive.
- MCP packet query/open access remains active; context-entry command activation
  remains inactive.
- Distribution publication does not begin under this contract.
