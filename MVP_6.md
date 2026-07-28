# MVP_6.md — CodeGraph Transformation And Verification Layer For Frontier Models

Generated: 2026-07-11
Status: locked future roadmap; not implemented behavior or training-result claim
Source-of-truth relationship: `MVP.md` remains the core system specification. `MVP_4.md` owns verified compressed and named repository flows. `MVP_5.md` owns speculative overlays, plan validation, editor workflows, and planned-versus-implemented reconciliation. This document owns the later frontier-model integration roadmap.

---

## 0. Locked Mission

Turn CodeGraph into a transformation and verification layer used while frontier models are trained and run.

The first objective is not to replace a frontier model's transformer architecture. The first objective is to make CodeGraph the structured repository environment in which a coding model:

```text
observes verified compact repository state
requests targeted expansion
proposes a plan or patch
receives graph- and source-grounded validation
revises its action
implements through the normal source workflow
reconciles planned and actual behavior
produces a reproducible training trajectory
```

MVP6 begins only after MVP5 provides stable named observations, speculative overlays, revisioned validation, and implementation reconciliation.

---

## 1. Product And Research Thesis

The highest-value integration is CodeGraph as a compiler, simulator, and verifier around the model:

```text
Repository
  -> verified CodeGraph
  -> compressed named observations
  -> model plan or patch
  -> speculative overlay
  -> CodeGraph validation
  -> source implementation
  -> re-index and reconcile
  -> correction, reward, preference, or training trajectory
```

This allows the model to learn when to query, what to inspect, how to test a plan before coding, how to distinguish proof from candidates, and how to recover from invalid assumptions.

It also improves inference-time agents before any model retraining occurs.

---

## 2. Integration Levels

### 2.1 Level 1 — Inference-Time Tool Environment

This is the first production integration and the prerequisite for later training.

Representative model actions:

```text
repo.observe
flow.find
flow.expand
symbol.context
impact.predict
overlay.create
overlay.validate
overlay.revise
patch.validate
implementation.reconcile
```

The model receives compact, named, proof-labeled observations instead of broad source dumps.

Example observation:

```json
{
  "goal": "add account lockout",
  "relevant_flows": [
    "AuthController.login -> AuthService.authenticate -> SessionFactory.create"
  ],
  "constraints": [
    "controllers cannot access UserRepository directly"
  ],
  "likely_edit_surfaces": [
    "AuthService.authenticate",
    "LoginFailureTracker"
  ],
  "unresolved": [
    "lockout persistence policy"
  ],
  "token_cost": 612
}
```

### 2.2 Level 2 — Tool-Use Supervised Fine-Tuning

Successful and failed tool-use episodes become training examples.

An episode records:

```text
user request
graph observations requested
speculative overlay operations
validation findings
overlay revisions
source patch
compile/test results
planned-vs-actual reconciliation
acceptance or rejection
```

This teaches the model when to query instead of guess, which graph operation to use, how much context to request, how to correct a plan, and how to minimize irrelevant edits.

### 2.3 Level 3 — Preference Data And Verifier Rewards

CodeGraph produces grounded comparisons and reward components for rejection sampling, preference optimization, and reinforcement learning.

Examples:

```text
rejected: directly modify UserRepository from AuthController
preferred: route the change through AuthService

rejected observation strategy: read 18 complete files
preferred observation strategy: retrieve named login flow, expand one boundary,
                                inspect two source spans
```

### 2.4 Level 4 — Distillation Into Model Behavior

A stronger tool-using teacher can generate verified trajectories that train smaller or cheaper models to:

```text
select observations efficiently
follow repository architecture
predict likely edit surfaces
avoid nonexistent symbols
recognize uncertainty and unsupported boundaries
produce compact valid plans
```

Distillation does not make the student graph-proof-capable without the verifier. The model's internal prediction remains a prediction until source/graph checks verify it.

### 2.5 Level 5 — Optional Neural Graph Encoder / Graph Transformer

A literal Graph Transformer is a later research branch, not the first MVP6 deliverable.

It may encode typed graph nodes, relations, ownership, calls, reads/writes, branches, guards, mutations, sanitizers, assertions, source spans, and proof state for cross-attention or adapters in a language model.

This branch proceeds only if controlled evaluations show that tool use, retrieval, and distillation leave a material gap that graph-native neural encoding closes.

---

## 3. Model-Facing Environment Protocol

The training and inference protocol should expose one stable state/action/observation contract.

### 3.1 Episode State

```text
repository identity and exact snapshot
graph/schema/capability passport
task id and goal
active overlay id and revision
accepted overlay revision if any
observation budget
token and latency budget
allowed side effects
approval state
```

### 3.2 Observation Contract

Every observation carries:

```text
stable observation id
query/tool action and normalized arguments
bounded result
proof strength and claimability
source spans and source roles
graph and overlay versions
truncation/omission metadata
token count and latency
unresolved, unknown, unsupported, and stale states
```

### 3.3 Action Contract

Actions distinguish:

```text
read/query
overlay mutation
source patch proposal
validation request
compile/test request
reconciliation request
external/destructive request requiring approval
```

No training harness may silently broaden action authority.

### 3.4 Environment Step

Each step records:

```json
{
  "repository_state": "...",
  "task_id": "...",
  "observation_id": "...",
  "tool_action": "...",
  "arguments": {},
  "result": {},
  "proof_strength": "...",
  "source_spans": [],
  "graph_version": "...",
  "overlay_revision": 4,
  "token_cost": 412,
  "latency_ms": 83,
  "accepted": true
}
```

---

## 4. Transformation Layer

CodeGraph transforms a repository and task into model-usable representations while preserving evidence boundaries.

Required transformations:

```text
source repository -> typed verified graph
verified graph -> compressed semantic skeleton
canonical skeleton -> named bounded projection
task -> relevant flow and edit-surface observations
free-form plan -> speculative overlay operations
overlay -> validation findings and recovery actions
source patch -> graph delta and proof-state changes
accepted plan + actual graph -> reconciliation packet
trajectory -> normalized training record
```

The transformation layer must be deterministic for a fixed repository, graph passport, schema version, request, and budget.

It must never erase the difference between source proof, derived-with-provenance facts, candidates, planned facts, or model guesses.

---

## 5. Verification Layer

The verifier evaluates plans and patches using composable evidence rather than one opaque score.

Required dimensions:

```text
symbol and endpoint validity
architecture and dependency boundaries
call/dataflow/branch/mutation consistency
source-role and lifecycle safety
proof ladder compliance
planned-vs-actual alignment
compile and test results
unexpected impact outside accepted scope
unsupported-claim count
nonexistent-symbol count
unnecessary observation and edit cost
recovery quality after a failed attempt
```

Verifier output must retain raw findings and components so researchers can change reward composition without regenerating the underlying episode.

---

## 6. Reward And Preference Contract

Illustrative reward record:

```json
{
  "task_success": 1.0,
  "compile": 1.0,
  "tests": 1.0,
  "graph_constraints": 0.96,
  "plan_actual_alignment": 0.91,
  "unsupported_claim_penalty": 0.0,
  "unnecessary_edit_penalty": 0.08,
  "observation_token_cost": 1840
}
```

Required principles:

```text
keep component rewards separate from aggregate reward
store verifier and schema version
exact evidence may receive stronger weight than derived/candidate evidence
unknown and unsupported are not automatic failures
never reward fabricated certainty
never make test pass the sole semantic reward
penalize broad irrelevant reads/edits only when task success is preserved
measure recovery and revision quality
retain human override/annotation provenance
```

Reward design must be evaluated for Goodhart behavior, especially models learning to avoid reporting gaps, overfit validators, or minimize edits at the expense of correctness.

---

## 7. Training Data Flywheel

```text
real coding task
  -> model uses CodeGraph
  -> observations/actions/validation are logged
  -> implementation and reconciliation occur
  -> human or automated acceptance is recorded
  -> successful and failed trajectories are normalized
  -> supervised, preference, or RL datasets are built
  -> improved model uses CodeGraph more efficiently
```

Failed trajectories are first-class data:

```text
guessed nonexistent symbol
violated dependency boundary
treated candidate evidence as graph proof
read excessive context
missed a branch or return path
edited unrelated files
planned one flow and implemented another
failed to recover after a validator finding
```

Datasets must keep failure, correction, and final acceptance linked to the exact repository snapshot and tool versions.

---

## 8. Dataset And Provenance Requirements

Every released or partner dataset requires:

```text
repository license and data-use status
exact commit/snapshot identity
source and artifact provenance
graph/schema/capability/tool versions
model/provider/version where permitted
prompt and tool policy versions
task source and acceptance method
human annotations and overrides
secret/PII scanning state
redaction and retention policy
train/eval contamination controls
unknown and missing fields represented explicitly
```

Secrets, private source, raw customer repositories, credentials, and proprietary prompts cannot enter training corpora by default. Local trajectory logging and export are separate approval boundaries.

Raw logs and generated payloads remain non-public artifacts unless intentionally reviewed and promoted.

---

## 9. Evaluation Program

Frontier-model integration must begin by proving value with existing models rather than asking a model provider to change pretraining architecture.

Required comparison arms:

```text
base coding agent with ordinary source search
agent with CodeGraph retrieval
agent with named compressed flows
agent with speculative overlay validation
agent fine-tuned on CodeGraph trajectories
agent trained with CodeGraph verifier rewards
```

Required metrics:

```text
task resolution and accepted patch rate
compile and test success
semantic/architecture violation rate
tokens consumed
files and source bytes read
irrelevant files read
unnecessary files edited
nonexistent-symbol references
unsupported semantic claims
plan-to-implementation drift
time to first valid plan and patch
number and quality of recovery attempts
tool latency and failure rate
human review time where measured
```

Evaluations require exact model/tool/repository versions, per-task records, raw component metrics, skipped/unknown states, and reproducible harnesses.

No public percentage improvement may be claimed until a promoted report supports it.

---

## 10. Frontier Lab Integration Path

The credible adoption sequence is:

```text
1. deliver stable inference-time CodeGraph environment APIs
2. publish or privately share reproducible evaluations
3. provide trajectory export and verifier interfaces
4. run partner tool-use fine-tuning experiments
5. run preference/rejection-sampling experiments
6. run component-reward RL experiments with safety review
7. distill efficient tool-use behavior
8. evaluate optional graph-native neural architecture
```

The partner surface must support:

```text
local/offline repositories
disposable workspaces
strict side-effect policies
deterministic replay
batched training environments
bounded observations
structured tool schemas
raw verifier findings
versioned reward adapters
privacy-preserving trajectory export
```

---

## 11. Optional Graph Transformer Research Branch

### 11.1 Candidate Architecture

```text
source token encoder --------------------+
                                          +-> language-model decoder
typed CodeGraph -> graph transformer -----+
```

Possible integrations:

```text
cross-attention from token decoder to graph embeddings
graph-prefix or memory tokens
adapter layers
retrieval over graph embeddings
mixture-of-experts graph component
```

### 11.2 Required Inputs

```text
typed stable node identities
directional relation kinds
ownership and scope
source-token alignment
source spans and roles
exact/derived/candidate/planned state
provenance links
incremental graph updates
language/frontend metadata
```

### 11.3 Research Gate

This branch requires controlled ablations against:

```text
ordinary text retrieval
CodeGraph structured retrieval
named compressed projections
tool-use fine-tuning
larger context windows
equivalent parameter/compute budgets
```

It proceeds only if graph-native encoding provides material, reproducible value beyond the simpler tool/verifier system.

---

## 12. Locked Delivery Sequence

### MVP6.0 — Training-Grade Environment Contract

```text
versioned state/action/observation schemas
deterministic episode replay
side-effect and approval policy
token/latency accounting
```

### MVP6.1 — Trajectory Logging And Export

```text
inference-time tool trajectories
overlay revision history
validation and reconciliation records
privacy/provenance controls
bounded export format
```

### MVP6.2 — Baseline Evaluation Harness

```text
ordinary-search baseline
CodeGraph retrieval arm
named-flow arm
overlay-validation arm
real-repository task corpus
reproducible reports
```

### MVP6.3 — Supervised Tool-Use Data

```text
successful and failed trajectory normalization
tool-choice and observation-budget examples
plan-revision examples
recovery examples
train/eval split controls
```

### MVP6.4 — Preference And Verifier Data

```text
plan and patch preference pairs
raw component verifier findings
reward-version registry
rejection sampling experiments
Goodhart/adversarial evaluation
```

### MVP6.5 — Verifier-Based Training

```text
component-reward training environment
safe tool-policy enforcement
offline and online evaluation
human override audit
model/provider partner experiments
```

### MVP6.6 — Distillation And Efficiency

```text
teacher trajectory distillation
smaller-model tool-use evaluation
token/latency optimization
failure-recovery retention
```

### MVP6.7 — Optional Graph Transformer Research

```text
graph encoder prototype
source-token alignment
cross-attention/adapters
controlled ablations
compute and data governance review
```

### MVP6.8 — Final Gates

```text
privacy and provenance gate
deterministic replay gate
verifier honesty gate
evaluation reproducibility gate
partner integration gate
public documentation and non-claims
```

---

## 13. Required Tests And Adversarial Gates

```text
episode replay is deterministic for fixed versions
stale/foreign repository state is non-claimable
planned facts never become source proof
candidate/text/vector evidence cannot earn graph-proof reward
tool authority cannot silently broaden
secret and PII fixtures are excluded/redacted
token/latency accounting is exact and bounded
truncated observations remain explicit
reward components reproduce from raw findings
aggregate reward version changes do not rewrite raw evidence
model cannot gain reward by hiding unknowns/gaps
model cannot gain reward by avoiding necessary edits
model cannot gain reward through validator-specific no-op patches
plan-to-actual drift is measured semantically
failed and recovered episodes remain linked
train/eval repository contamination is detected
model/tool/schema versions are mandatory
unknown/skipped fields never become fabricated values
graph-transformer ablations use equivalent budgets
```

---

## 14. Acceptance Criteria

MVP6 is complete only when:

```text
CodeGraph exposes a stable training-grade environment protocol
inference-time agents use compact verified observations effectively
episodes can be replayed from exact repository and tool state
trajectory export preserves privacy, provenance, and proof boundaries
supervised and preference datasets include successful and failed reasoning paths
verifier rewards remain decomposed and reproducible
evaluation shows where CodeGraph helps, ties, regresses, or remains unknown
tool-use training improves behavior without fabricating graph proof
frontier-lab integration can run in disposable, auditable environments
optional graph-transformer work is gated by controlled evidence
no public performance or training-quality claim exceeds promoted evidence
```

---

## 15. Non-Goals And Non-Claims

MVP6 does not automatically claim:

```text
CodeGraph is itself a frontier foundation model
a neural graph encoder is required
tool output stored in model weights remains current
model-generated plans or patches are proof
passing tests proves semantic correctness
verifier reward equals human preference
private repositories may be used for training
public benchmark superiority
real-agent patch-quality superiority
production training gains
SOTA coding performance
```

The locked strategic position is: CodeGraph first becomes the structured transformation, planning, validation, and reconciliation environment around coding models. Neural graph integration remains an evidence-gated later branch.
