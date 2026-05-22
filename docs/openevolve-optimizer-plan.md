# OpenEvolve Optimizer Plan For CodeGraph

Status: experimental design note.

This document captures a proposed way to use OpenEvolve as a controlled optimization tool for CodeGraph. It is not a shipped behavior claim, not public benchmark evidence, not an official SWE-bench claim, and not a CodeGraph-over-rg/CGC claim.

The intended use is narrow: OpenEvolve can help search policy choices inside a strict CodeGraph evaluator. It should not be treated as a general autonomous agent that safely rewrites the whole CodeGraph repository.

## Why This Exists

CodeGraph recently split vector-related artifacts into:

- a finished graph DB, which remains the proof source of truth;
- a candidate spool, which can provide early candidate-only context while indexing is incomplete;
- a compact runtime vector sidecar, which provides selected semantic candidates after indexing;
- an optional audit artifact, which is diagnostic-only.

The split worked for the runtime vector sidecar. The remaining problem is the candidate spool.

Local diagnostic evidence from the SymPy focused run showed:

| Item | Measured value |
|---|---:|
| SymPy graph DB | 328,818,688 B |
| Runtime vector sidecar | 6,257,614 B |
| Candidate spool | 410,718,819 B |
| Candidate records | 217,514 |
| Average bytes per candidate | 1,888.24 B |
| Candidate-spool query wall time | about 20.7 s |
| Status with candidate spool wall time | about 22.4 s |

The graph DB remained claimable and the runtime sidecar remained ready. The issue was not DB corruption and not vector-sidecar failure. The issue was that the candidate spool currently behaves like an uncapped JSONL firehose: too many records, too much repeated metadata, one giant file, and linear scans for query/status.

That makes the problem suitable for a policy optimizer:

```text
Given a fixed corpus and fixed benchmark tasks,
find a better policy for selecting, grouping, ranking, and storing candidate context.
```

This is exactly the kind of search problem OpenEvolve can help with, if the evaluator is strict enough.

## OpenEvolve Fit

OpenEvolve is an evolutionary coding loop around LLMs. Its native loop is:

```text
initial program
-> LLM proposes diff or rewrite
-> evaluator runs candidate
-> metrics returned
-> candidate stored in population/checkpoint database
-> MAP-Elites/islands select future parents
-> repeat
```

The inspected upstream repo was:

- Repository: `algorithmicsuperintelligence/openevolve`
- Pinned inspected commit: `80945ed82886d5c4ff2f3d22436765d50cb61266`
- Relevant inspected areas: controller, evaluator, database, iteration loop, default config, and Rust example evaluator.

The useful part for CodeGraph is OpenEvolve's mutation/search engine:

- LLM-based candidate generation;
- population storage;
- checkpointing;
- MAP-Elites/island search;
- iterative scoring;
- candidate replay.

The unsafe part is assuming OpenEvolve knows CodeGraph's product boundaries. It does not.

## What OpenEvolve Should Not Do

OpenEvolve should not directly rewrite the whole CodeGraph repo.

It should not own:

- full cold-index architecture rewrites;
- multi-file schema migrations;
- production agent-use profile durability;
- real-time delta sync;
- claimability/proof semantics across many Rust modules;
- automatic staging, committing, pushing, or PR creation.

Those are normal engineering work. OpenEvolve is useful only when the target can be reduced to a small policy surface and scored deterministically.

## Why Not Use OpenEvolve Out Of The Box

Out of the box, OpenEvolve optimizes a program. CodeGraph needs it to optimize a product behavior safely.

OpenEvolve can do:

```text
candidate file
-> mutate code
-> run evaluator
-> score candidate
-> keep best variants
```

CodeGraph also needs:

```text
do not mutate the real repo
do not create normal .codegraph
do not weaken graph-proof boundaries
do not pass because of fake or partial context
do not accept giant artifacts
do not count speed wins when recall collapsed
do not leak logs, DBs, patches, or secrets
```

The evaluator is the safety boundary. If the evaluator is naive, OpenEvolve can discover bad shortcuts.

Examples:

- If the score only minimizes spool bytes, it can win by dropping almost every candidate.
- If the score only minimizes query latency, it can win by returning fewer results.
- If broad Rust edits are allowed, it can win by deleting audit metadata, weakening validation, or skipping expensive checks.

So the correct framing is:

```text
OpenEvolve = candidate idea generator
CodeGraph evaluator = truth and safety gate
Human/Codex engineering = product integration
```

## Safety Architecture

The main CodeGraph repo should stay untouched during evolution.

Recommended layout:

```text
benchmarks/openevolve/
  README.md
  configs/
    candidate_spool_policy.yaml
    runtime_vector_selection.yaml
    retrieval_planner_policy.yaml
  targets/
    candidate_spool_policy.py
    vector_selection_policy.py
    retrieval_planner_policy.py
  evaluators/
    evaluate_candidate_spool_policy.py
    evaluate_vector_selection_policy.py
    evaluate_retrieval_planner_policy.py
  fixtures/
    candidate_inventory/
    internal_gold_subset/
    sympy_focused/
  reports/
    README.md

benchmarks/workspaces/openevolve_runs/
  ignored generated runs
  disposable candidate dirs
  copied workspaces or git worktrees for promoted candidates
  raw logs, DBs, sidecars, patches, and metrics
```

Generated runs, candidate workspaces, DBs, sidecars, patches, raw model outputs, and logs must remain ignored/local.

## Isolation Levels

OpenEvolve does not think in GitHub fork terms. It evolves candidate versions internally. CodeGraph decides how much isolation each candidate receives.

### Level 1: Temp Single-File Evaluation

Fastest path.

OpenEvolve mutates one policy file. The evaluator applies that policy to a frozen candidate inventory and computes metrics without rebuilding the whole repo.

Best for:

- candidate keep/drop scoring;
- packet grouping;
- cap tuning;
- query ranking;
- serialization threshold exploration.

### Level 2: Disposable Copied Workspace

Medium-cost path.

The evaluator copies the repo into an ignored workspace, injects a candidate policy or narrow patch, and runs targeted checks.

Best for:

- validating that a policy can survive real CodeGraph command behavior;
- release-binary smokes;
- artifact and claimability checks.

### Level 3: Git Worktree For Serious Candidates

Slowest path.

Only top candidates are applied to clean git worktrees. This should be used before human review, not for every mutation.

Best for:

- final candidate replay;
- real Rust patch checks;
- comparing top policies under clean source control.

## Workflow 1: Fast Policy Lab

This is the first workflow to build.

Goal:

```text
Find better candidate spool caps, grouping, ranking, and packet rules.
```

OpenEvolve edits:

```text
one small policy file only
```

CodeGraph repo:

```text
not directly modified
```

Evaluation:

```text
deterministic, cheap, repeatable
```

End-to-end flow:

1. Freeze the problem.

   Example:

   ```text
   Reduce the SymPy candidate spool from about 410 MB while still finding
   sympy/core/_print_helpers.py quickly.
   ```

2. Create a small evolvable target.

   Example:

   ```text
   benchmarks/openevolve/targets/candidate_spool_policy.py
   ```

   The target exposes simple policy functions:

   ```text
   score_candidate(candidate)
   choose_packet_bucket(candidate)
   should_keep_candidate(candidate, current_counts)
   max_candidates_for_file(file_kind)
   max_candidates_for_kind(candidate_kind)
   rank_packet(packet, query)
   ```

3. Prepare fixed input data.

   Do not make OpenEvolve re-index SymPy for every mutation. Instead, capture a deterministic candidate inventory once:

   ```text
   file/path candidates
   text evidence candidates
   symbol candidates
   import/export source-navigation candidates
   relation-neighborhood candidates
   ```

4. Let OpenEvolve mutate the policy file.

   It can try variations in:

   ```text
   per-file caps
   per-kind caps
   per-directory caps
   ranking weights
   aggregation rules
   packet size limits
   text truncation thresholds
   metadata compaction choices
   ```

5. Score each candidate policy.

   Score dimensions:

   ```text
   output spool bytes
   packet count
   metadata repetition
   query latency
   whether _print_helpers.py is found
   gold file recall@5
   MRR
   candidate kind diversity
   claimability violations
   unsupported-claim violations
   ```

6. Hard-fail unsafe candidates.

   A candidate gets score zero if:

   ```text
   graph proof is claimed from spool/vector evidence
   required gold files disappear
   output is unparsable
   policy is nondeterministic
   spool exceeds the hard artifact budget
   normal .codegraph is created
   ```

7. Store winners.

   OpenEvolve stores top policies in its run directory.

8. Replay the top policies.

   Replay the top 5 policies with the same evaluator, fresh process, and fixed seed. No cherry-picking.

9. Port the winning idea manually.

   The winning policy is a design suggestion, not an automatic product patch.

10. Run real CodeGraph gates.

   Required checks:

   ```text
   targeted Rust tests
   release-binary smoke
   SymPy focused run
   internal retrieval smoke
   no normal .codegraph mutation
   ```

## Workflow 2: Product Promotion

Use this only after Workflow 1 produces a promising policy.

Goal:

```text
Convert the winning policy into durable CodeGraph behavior.
```

OpenEvolve edits:

```text
still preferably one narrow file or generated patch
```

CodeGraph repo:

```text
evaluated in disposable copied workspaces or git worktrees
```

Evaluation:

```text
real build, tests, release binary, and artifact checks
```

End-to-end flow:

1. Take a top policy from Workflow 1.

2. Generate a narrow implementation patch.

   Likely target surface:

   ```text
   crates/codegraph-index/src/lib.rs
   crates/codegraph-cli/src/lib.rs
   crates/codegraph-mcp-server/src/lib.rs
   crates/codegraph-cli/src/audit.rs
   ```

   Keep the patch restricted to:

   ```text
   candidate spool caps
   packet aggregation
   query index/shard behavior
   spool manifest metrics
   budget-grace behavior
   inspector compatibility
   ```

3. Create a disposable workspace.

   Example:

   ```text
   benchmarks/workspaces/openevolve_runs/<run_id>/worktree
   ```

4. Apply the candidate patch in that workspace.

   The main repo remains untouched.

5. Run fast product checks.

   ```text
   cargo check -p codegraph-index
   candidate_spool targeted tests
   context-pack/query targeted tests
   claimability targeted tests
   ```

6. Run release smoke.

   Required smoke cases:

   ```text
   Buildroot mini index
   forced partial spool
   query against partial spool
   context-pack against partial spool
   runtime sidecar still works
   audit artifact remains diagnostic-only
   stale/corrupt spool cases
   ```

7. Run real-size SymPy check.

   Required measurements:

   ```text
   candidate spool bytes
   candidate record count
   query latency
   status latency
   _print_helpers.py found
   DB claimable
   runtime sidecar size
   graph proof boundaries preserved
   ```

8. Score the candidate patch.

   Example weighting:

   | Area | Weight |
   |---|---:|
   | Correctness and claimability | 40% |
   | Spool size reduction | 25% |
   | Query/status latency | 20% |
   | Recall preservation | 10% |
   | Implementation simplicity | 5% |

9. Reject unsafe patches automatically.

   Reject if:

   ```text
   tests fail
   graph-proof boundary weakens
   normal .codegraph appears
   output schema breaks
   SymPy gold file is not found
   spool remains huge
   runtime sidecar regresses
   graph DB claimability regresses
   ```

10. Promote one candidate.

   Human/Codex reviews the patch and applies it intentionally to the real branch.

## First Five Tasks

These are the first tasks to assign once the custom OpenEvolve harness exists.

### 1. Bound Candidate Spool Size Without Losing Gold Hits

Goal:

```text
Reduce SymPy candidate spool size from about 410 MB while still finding:
- sympy/core/_print_helpers.py
- relevant __slots__ / __dict__ surfaces
- relevant Symbol / Printable context
```

Evaluator metrics:

```text
spool bytes
query latency
status latency
gold file recall@5
MRR
candidate kind diversity
claimability violations == 0
unsupported-claim violations == 0
```

This is the first task because it is measurable, isolated, and directly tied to the current blocker.

### 2. Design Candidate Packet Aggregation

Goal:

```text
Group related candidates into fewer packets instead of one JSON object per tiny clue.
```

Example file packet:

```text
file path/title
file kind
top symbols
top text snippets
import/export hints
source hash/mtime/size
proof labels
```

Evaluator metrics:

```text
packet count reduction
metadata repetition reduction
query latency
auditability preserved
gold hits preserved
```

This attacks the root of the 410 MB spool problem: repeated metadata and too many tiny records.

### 3. Tune Spool Query Ranking

Goal:

```text
Rank useful file/path/symbol hits before broad text noise.
```

Evaluator query tasks:

```text
_print_helpers
Symbol
Printable
__slots__
generic-package
Config.in
PathEvidence
```

Score:

```text
correct file appears early
MRR
Recall@5
latency
no graph-proof overclaim
```

This makes the spool useful as early context, not just smaller.

### 4. Optimize Runtime Vector Chunk Selection Weights

Goal:

```text
Improve the 4,096 selected runtime chunks without increasing artifact size.
```

Evaluator checks:

```text
internal gold Recall@5 / MRR
RepoBench-style diagnostic subset
CrossCodeEval-style diagnostic subset
SymPy focused task
chunk diversity by file/type/source
claimability violations
```

This is less urgent than spool size, because the runtime sidecar is already compact enough to be useful. It can still improve quality.

### 5. Find A Strong CodeGraph Retrieval Planner Policy

Goal:

```text
Given a task, choose the right CodeGraph primitive:
- query files
- query symbols
- query text
- context-pack
- vector candidates
- candidate spool
```

Evaluator compares:

```text
current codegraph_full
planned CodeGraph policy
strong rg baseline
```

Score:

```text
Recall@5
MRR
context bytes
tool calls
time to first useful context
claimability violations
unsupported-claim violations
```

This moves CodeGraph toward the product goal: first-call repo task routing, not just search replacement.

## First Task To Actually Run

The first concrete OpenEvolve task should be:

```text
Evolve candidate spool caps and keep/drop scoring so:
- SymPy candidate spool drops below 50 MB as an interim target;
- the bounded-contract target remains 8 MiB by default where budget allows;
- _print_helpers.py remains Recall@5;
- query latency drops below 2 seconds as an interim target;
- deterministic fixture query p95 targets 500 ms;
- status with spool targets below 1 second on fixture scale;
- claimability violations remain 0;
- graph-proof boundaries remain unchanged.
```

This gives a clean before/after against the known problem:

```text
Before:
  217,514 records
  about 410 MB
  about 20-22 s query/status scans

Target:
  bounded records
  under budget
  fast query/status
  same gold hits
  no proof-boundary regression
```

## Evaluator Contract

The evaluator should return structured metrics, not prose.

Minimum result shape:

```json
{
  "status": "pass|fail",
  "combined_score": 0.0,
  "hard_fail_reason": null,
  "spool": {
    "bytes": 0,
    "records": 0,
    "packets": 0,
    "truncated": false,
    "omitted_by_cap": 0,
    "omitted_by_budget": 0,
    "omitted_by_dedup": 0
  },
  "retrieval": {
    "gold_file_recall_at_5": 0.0,
    "mrr": 0.0,
    "required_files_found": []
  },
  "latency": {
    "query_ms": 0.0,
    "status_ms": 0.0
  },
  "trust": {
    "claimability_violations": 0,
    "unsupported_claim_violations": 0,
    "graph_proof_from_spool": false
  },
  "hygiene": {
    "normal_dot_codegraph_created": false,
    "generated_artifacts_ignored": true
  }
}
```

Hard gates:

```text
build/test failure -> score 0
unparseable output -> score 0
claimability violation -> score 0
graph proof from spool/vector evidence -> score 0
required gold file missing -> score 0
normal .codegraph mutation -> score 0
artifact budget exceeded in default mode -> score 0
```

Soft score:

```text
size reduction
latency reduction
recall/MRR preservation
candidate diversity
packet compactness
implementation simplicity
```

## Candidate Spool Policy Knobs

OpenEvolve can safely explore knobs like:

```text
global max records
global max bytes
per-file cap
per-directory cap
per-kind cap
per-source-kind share
snippet byte cap
max snippets per file packet
max symbols per file packet
path/title priority
symbol priority
text evidence priority
import/export priority
relation-neighborhood penalty
dedupe key shape
packet aggregation threshold
query ranking weights
```

It should not explore:

```text
whether graph_proof can be true
whether claimability labels can be omitted
whether stale artifacts can be treated as fresh
whether full source bodies can be stored by default
whether artifact budget checks can be removed
```

Those are not optimization knobs. They are product invariants.

## Success Criteria

The first successful OpenEvolve-assisted policy should prove:

```text
candidate spool is bounded by default
candidate spool remains candidate-only
query/status do not full-scan a giant JSONL file
SymPy required file remains discoverable
runtime vector sidecar stays compact
graph DB remains the proof source
audit artifact remains diagnostic-only
no normal .codegraph mutation occurs
```

Good first milestone:

```text
SymPy spool under 50 MB
_print_helpers.py Recall@5 preserved
spool query under 2 s
claimability violations 0
```

Final default target from the bounded spool contract:

```text
default bounded spool around 8 MiB where budget allows
about 12,000 records or fewer
query/status paths indexed or sharded
fixture query p95 under 500 ms
fixture status under 1 s
```

## Risks And Mitigations

| Risk | Mitigation |
|---|---|
| OpenEvolve optimizes away useful candidates | Gold recall/MRR hard gates |
| OpenEvolve makes output fast by returning too little | Required file/symbol checks |
| OpenEvolve weakens proof labels | Claimability hard gates |
| Candidate patches mutate the main repo | Disposable workspaces only |
| Candidate runs create normal `.codegraph` | Explicit no-mutation check |
| LLM/API use leaks secrets | Opt-in config only; never print env secrets |
| Candidate run produces large artifacts | Ignored workspaces and artifact budgets |
| Results are overclaimed | Label as local diagnostic optimization only |

## Relationship To Normal Engineering

OpenEvolve should not replace normal debugging and architecture work.

Correct split:

```text
Human/Codex engineering:
  defines product boundary
  chooses architecture
  writes durable implementation
  validates proof semantics

OpenEvolve:
  searches policy/ranking/cap/aggregation choices
  proposes candidate policies
  optimizes within fixed evaluator constraints

CodeGraph gates:
  decide whether anything ships
```

That keeps the project disciplined: OpenEvolve explores the scoring surface, but CodeGraph's own tests, release smokes, artifact budgets, and claimability boundaries remain the authority.

## Implementation Phases

### Phase 0: Harness Contract

Create the local experimental harness layout, ignored run paths, evaluator result schema, and fixed input inventory format.

No model/API call is required in this phase.

### Phase 1: Candidate Spool Policy Lab

Add the first evolvable policy target and deterministic evaluator.

Run with a small number of iterations only after API/model configuration is explicitly provided.

### Phase 2: Replay And Report

Replay top candidates under a fixed seed and generate a local diagnostic report:

```text
best policies
score table
spool bytes
latency
gold hits
claimability
failure cases
```

### Phase 3: Product Patch Promotion

Manually port the winning policy into CodeGraph behind normal product tests.

### Phase 4: Full CodeGraph Gate

Run:

```text
cargo targeted tests
release build
Buildroot mini smoke
SymPy focused smoke
internal retrieval smoke
claimability checks
artifact hygiene checks
no-normal-.codegraph check
```

Only after this should the idea be considered for durable product behavior.

## Final Recommendation

Build the OpenEvolve harness, but keep it experimental and off the critical path.

Use it first for:

```text
candidate_spool_bounded_packet_policy
```

Do not use it first for:

```text
full CodeGraph cold-index architecture rewrite
schema migration
production agent-use profile durability
real-time delta sync
```

The first concrete win should be simple and measurable:

```text
turn the current candidate spool from a 410 MB linear-scan firehose
into a bounded, indexed, candidate-only working set
without losing the files and symbols an agent needs.
```

