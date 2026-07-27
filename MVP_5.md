# MVP_5.md — Named Semantic Projections And Speculative Graph Planning

Generated: 2026-07-11
Status: locked future roadmap; not implemented behavior
Source-of-truth relationship: `MVP.md` remains the core system specification. `MVP_4.md` owns deterministic AST skeleton compression and the named projection foundation. This document owns the product and agent workflow between MVP4 and MVP6: named flow consumption, speculative graph overlays, plan validation, editor interaction, and planned-versus-implemented reconciliation.

---

## 0. Locked Boundary

MVP5 begins only after the MVP4.4 named compressed projection acceptance gate is complete.

MVP5 does not train a frontier model and does not add a neural Graph Transformer. Those belong to `MVP_6.md`.

MVP5 turns CodeGraph into a repository-aware planning environment in which a human or coding agent can:

```text
observe a compact named flow
propose future symbols and relations without editing source
validate the proposal against the current verified graph
revise or discard the proposal transactionally
freeze an accepted plan revision
implement the plan through the normal source workflow
re-index the resulting source
reconcile the actual graph with the accepted plan
```

The current indexed repository graph remains the immutable baseline for every planning session. Planned facts are never source proof.

---

## 1. Product Thesis

The useful CodeGraph planning editor is not a separate drawing application and not a replacement source-code editor.

It is a speculative graph transaction with one contract shared by:

```text
CLI
MCP
agent APIs
local visual editor
validation and linting
implementation reconciliation
```

The visual editor is a client of the overlay contract. It is not the source of truth.

The primary agent benefit is the ability to test architecture and flow assumptions before spending tokens generating or reading large amounts of source.

---

## 2. Named Semantic Flow Consumption

MVP5 consumes the named compressed projection established by MVP4.4. Canonical identity and displayed names remain separate:

```text
canonical identity  stable node/edge/path ids and proof metadata
display identity    compact repository names selected for the current view
```

### 2.1 Default Agent View

The preferred default is `compact_named`:

```text
AuthService.login(rawToken)

main:
  rawToken -> verifyJwt -> claims -> findActiveUser -> createSession -> return session

guards:
  invalid JWT -> AuthError
  inactive user -> AccessDenied

mutations:
  session.lastSeen = now

proof:
  8 exact facts, 4 derived flows, 0 unresolved dynamic calls
```

This view must be bounded and progressively expandable. It must not silently become a full graph or full-source dump.

### 2.2 Required Expansion Operations

The implementation may refine exact names, but must provide equivalent operations:

```text
flow.get_named
flow.expand_path
flow.expand_branch
flow.expand_node
flow.show_provenance
flow.show_source
flow.compare
```

Each expansion accepts stable handles from the prior response and returns only requested detail.

### 2.3 Named Entity Rules

Functions, classes, methods, objects, variables, and properties have distinct semantics:

```text
functions carry stable qualified identity and call role
methods retain owning type and dispatch boundary
classes provide ownership/context unless the type itself participates in flow
object bindings remain distinct from declared or inferred types
variables remain scoped to their binding and function domain
properties retain their complete access path where proven
same-name symbols never merge because display strings match
```

Example compact node representation:

```json
{
  "display": "user.accountId",
  "symbol_id": "binding://AuthService/login/user",
  "type_display": "User",
  "access_path": ["user", "accountId"],
  "source_span": "...",
  "proof_strength": "source_verified"
}
```

The default may render only `user.accountId`, but stable identity and proof metadata remain available through expansion.

---

## 3. Speculative Graph Overlay

### 3.1 Meaning

A speculative overlay is a temporary, revisioned graph layered over one exact baseline repository snapshot.

It may contain planned files, modules, functions, methods, classes, types, bindings, objects, calls, reads, writes, flows, guards, branches, mutations, sanitizers, assertions, dependencies, removals, replacements, assumptions, risks, and open decisions.

It does not edit source and does not mutate the production graph.

### 3.2 Required Views

Every overlay-aware query supports an explicit view:

```text
base      verified repository graph only
overlay   proposed-only facts and annotations
combined  base plus overlay, with evidence states preserved
diff      additions, removals, replacements, conflicts, unresolved references
```

Switching views must not rebuild or rewrite the baseline database.

### 3.3 Evidence States

Every result distinguishes at least:

```text
source_verified
planned
planned_resolves_to_source
planned_unresolved
planned_conflicts_with_source
implemented_matches_plan
implemented_drifted_from_plan
implemented_extra
planned_missing_from_implementation
```

`planned` is never a graph-proof label.

### 3.4 Overlay Lifecycle

Required lifecycle operations:

```text
create
inspect
revise
validate
compare revisions
freeze accepted revision
pause
resume
discard
rebase onto a newer repository snapshot
reconcile after implementation
archive local evidence
```

All destructive lifecycle actions require explicit overlay identity and must not affect source or the base graph.

---

## 4. Agent-Native Planning Contract

The initial implementation favors a small structured API rather than a large custom UI.

Representative operations:

```text
overlay.create
overlay.add_symbol
overlay.add_relation
overlay.remove_relation
overlay.annotate
overlay.validate
overlay.query
overlay.diff
overlay.revise
overlay.freeze
overlay.discard
overlay.rebase
overlay.reconcile
```

### 4.1 Compact Plan Submission

An agent can propose a feature without restating the repository:

```text
Extend flow auth-login after findActiveUser:
  add RiskService.evaluate(user, request.ip)
  branch highRisk -> MfaChallenge
  branch normal -> existing createSession
```

CodeGraph resolves existing symbols, creates proposed identities for new symbols, and returns explicit ambiguity or unresolved states.

### 4.2 Validation Response

Validation is actionable and repository-specific:

```text
Proposed call:
  CheckoutService -> InventoryStore.reserve

Conflict:
  application services may not access InventoryStore directly

Existing valid path:
  CheckoutService -> InventoryService -> InventoryStore
```

The response identifies the proposed fact, violated contract, exact baseline evidence, recommended existing primitive where known, and proof boundary.

### 4.3 Token Discipline

Overlay state lives in CodeGraph and is referenced by stable id and revision:

```text
Validate overlay feature/checkout-retry revision 4.
Return only unresolved violations and affected public APIs.
```

Agents do not resend the full plan on each turn.

---

## 5. Planning Validation And Linting

Required validation families:

```text
missing, ambiguous, or conflicting symbol identities
forbidden dependency directions
layer and ownership violations
invalid directional relation endpoint pairs
unresolved calls/imports/exports
new architectural cycles
unexpected public API changes
unbounded or unjustified impact radius
branch/return-path contradictions
mutation without a proven binding
sanitizer/assertion claims without required evidence
source-role leakage
stale baseline or overlay revision
dynamic/runtime/compiler/framework boundaries
security-sensitive dataflow changes
planned removal of still-required relations
```

Validation reuses existing core capability and proof contracts. It must not create a second, looser planned-graph relation registry.

### 5.1 Finding State And Recovery

```text
blocking_conflict
warning
unknown
unsupported
not_applicable
diagnostic
```

Recovery includes retry after identity resolution, revise, explicitly skip unsupported validation, rebase, discard, or abort.

### 5.2 No Fake Proof

Validation success means the plan is coherent against current known contracts. It does not prove that future source exists or is correct.

---

## 6. Revisioned Overlay Storage

Use boring, inspectable primitives:

```text
SQLite-backed overlay metadata and revisions
append-only operation log per revision
stable ids derived from overlay, revision, and local identity
explicit base repository identity and graph/schema passport
bounded JSON import/export
transactional revision creation
no silent fallback to repo-local production databases
```

Required metadata includes overlay id/name, base repository/worktree state, graph passport, schema/capability versions, current and frozen revisions, creator metadata when supplied, timestamps, assumptions, open decisions, validation summary, and reconciliation state.

Overlay databases and raw operation logs remain local artifacts unless explicitly promoted.

---

## 7. Custom Editor

The custom editor is a thin local client over the overlay and named-flow APIs.

Required modes:

```text
base graph
overlay only
combined graph
base-vs-overlay diff
accepted-plan-vs-implementation diff
```

Required interactions:

```text
search existing symbols
add planned symbol/relation
connect to an existing flow
inspect proof/source span
view validation finding
accept or revise recommendation
compare revisions
freeze revision
disable overlay instantly
discard overlay with confirmation
export/import bounded plan JSON
copy compact plan context for an agent
```

Guardrails include visible graph caps, expansion on demand, explicit truncation, distinct planned/verified styling, no source editing in the first release, no automatic plan application, no hidden baseline mutation, and a keyboard-accessible non-canvas view.

The editor is implemented only after CLI/MCP can use the same contract without it.

---

## 8. Implementation Reconciliation

After source implementation and re-indexing, CodeGraph compares the actual verified graph with the accepted overlay revision.

Required categories:

```text
implemented_matches_plan
implemented_with_equivalent_structure
planned_symbol_missing
planned_relation_missing
unexpected_symbol_added
unexpected_relation_added
endpoint_changed
ownership_changed
flow_or_branch_changed
proof_strength_changed
implementation_contains_unresolved_gap
baseline_changed_since_plan
```

Reconciliation compares semantic identities and relations, not only filenames or text diffs. It answers which planned behaviors exist, which were omitted, which extras appeared, whether architecture/public API drifted, whether proof strength changed, and whether impact escaped the accepted edit surface.

Reconciliation is review and future-training evidence. It does not replace compilation, tests, or human review.

---

## 9. Recovery And Concurrency

Overlay planning preserves the normal developer workflow:

```text
multiple overlays may exist without automatic merge
each overlay is bound to one baseline snapshot
stale overlays are explicit
rebase produces a new revision and conflict report
discard never touches source
validation can pause and resume
failed validation cannot corrupt the last valid revision
editor disconnect cannot lose committed overlay operations
```

Concurrent agents use separate overlay ids or explicit coordination. There is no last-writer-wins merge of planned graph facts.

---

## 10. Locked Delivery Sequence

### MVP5.0 — Named Projection Productization

```text
stabilize named flow API from MVP4.4
add compact/qualified/role display modes
add progressive expansion handles
prove token and source-span budgets
```

### MVP5.1 — Overlay IR And Transactions

```text
overlay identity and revision schema
base/overlay/combined/diff query modes
symbol and relation operations
pause/resume/discard/rebase
```

### MVP5.2 — Plan Validation

```text
capability-backed relation checks
architecture/dependency rules
identity resolution
impact and cycle checks
proof-state output
```

### MVP5.3 — Agent Planning Workflow

```text
compact plan submission
revision feedback loop
accepted revision freeze
token/latency telemetry
CLI and MCP parity
```

### MVP5.4 — Local Visual Editor

```text
shared API client
base/overlay/diff modes
finding inspection and revision tools
bounded import/export
```

### MVP5.5 — Implementation Reconciliation

```text
post-index planned-vs-actual comparison
semantic drift categories
review packet
trajectory-ready evidence record
```

### MVP5.6 — Final Gates

```text
failure and recovery matrix
concurrent overlay tests
stale/rebase tests
token-budget evaluation
real-repository planning corpus
honest product documentation
MVP6 handoff
```

---

## 11. Required Tests

```text
baseline graph is byte/state unchanged by overlay operations
base/overlay/combined/diff views are deterministic
planned facts never become source proof
overlay revisions are immutable after freeze
discard removes only the named overlay
stale baseline blocks claimable reconciliation
same-name symbols remain distinct
invalid endpoint direction is rejected
architecture cycle is detected
unsupported dynamic relation remains unknown/unsupported
compact named view stays within budget
progressive expansion returns requested detail only
editor and MCP produce equivalent overlay state
plan import/export round-trips
reconciliation detects missing and extra relations
equivalent semantic implementation may be equivalent without text equality
no source, production DB, secret, or external system mutates implicitly
```

---

## 12. Acceptance Criteria

MVP5 is complete only when:

```text
named projections are stable, scoped, and progressively expandable
a coding agent can create and revise a nontrivial plan without broad source rereads
the plan is a revisioned speculative graph
base truth and planned state are unmistakably separate
validation reuses current capability/proof contracts
the overlay can be disabled or discarded instantly
an accepted revision can be frozen and reviewed
the visual editor is optional and shares the same API
actual implementation can be reconciled against the accepted plan
token, latency, truncation, and evidence metadata are inspectable
real-repository evaluation shows usefulness without fabricated claims
```

---

## 13. Non-Goals And Non-Claims

MVP5 does not claim:

```text
planned graphs are source proof
automatic source generation is correct
the editor replaces an IDE
arbitrary architecture rules can be inferred automatically
dynamic/runtime behavior is known without evidence
formal verification of arbitrary programs
frontier-model training integration
public benchmark superiority
real-agent patch-quality superiority
```

The locked roadmap after MVP5 is `MVP_6.md`: CodeGraph as a transformation and verification layer used while frontier models are trained and run.
