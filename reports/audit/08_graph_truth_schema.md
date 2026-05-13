# 08 Graph Truth Schema

Source of truth: `MVP.md`.

## Verdict

The strict graph-truth schema now exists, but it is not a benchmark runner yet. This phase defines the proof obligations future benchmark code must enforce: expected facts, forbidden facts, source-span provenance, exactness, production/test/mock context, derived-edge provenance, stale-update failures, identity collision failures, and context-packet distractor thresholds.

No production extraction, scoring, retrieval, storage, or benchmark-result claims were changed.

## Inputs Read

| Input | What It Contributed |
| --- | --- |
| `MVP.md` | Graph-first proof model, relation/exactness vocabulary, source-span provenance requirement, and benchmark phase direction. |
| `crates/codegraph-bench/src/lib.rs::GroundTruth` | Existing internal benchmark schema; positive-only expected files/symbols/relations/tests. |
| `crates/codegraph-bench/src/lib.rs::ExpectedRelation` | Current relation truth uses relation plus head/tail substring and optional file path. |
| `crates/codegraph-bench/src/competitors/codegraphcontext.rs::ExternalGroundTruth` | CGC fixture truth has files/symbols/path symbols/source spans, but no forbidden graph facts. |
| `reports/audit/01_benchmark_validity.md` | Existing pass numbers are not trustworthy for graph correctness; next schema needs expected and forbidden facts. |
| `reports/audit/07_source_span_proof_gate.md` | Proof-grade edges and paths must require source spans and must not label missing-span evidence as proof. |

## Created Schema

Schema file: `benchmarks/graph_truth/schemas/graph_truth_case.schema.json`.

Required top-level case fields:

- `case_id`
- `description`
- `repo_fixture_path`
- `task_prompt`
- `expected_entities`
- `expected_edges`
- `forbidden_edges`
- `expected_paths`
- `forbidden_paths`
- `expected_source_spans`
- `expected_context_symbols`
- `forbidden_context_symbols`
- `expected_tests`
- `forbidden_tests`
- `notes`

Optional support fields:

- `schema_version`
- `failure_rules`
- `distractor_policy`

## Edge Format

Each expected or forbidden edge must include:

| Field | Purpose |
| --- | --- |
| `head` | Exact entity selector for the source endpoint. |
| `relation` | CodeGraph relation enum from `codegraph-core`. |
| `tail` | Exact entity selector for the target endpoint. |
| `source_file` | Repository-relative file path for the fact. |
| `source_span` | Line/column span, optional expected text, and syntax role. |
| `exactness` | Allowed exactness labels, minimum exactness, proof-grade requirement, and confidence floor. |
| `context` | `production`, `test`, `mock`, `fixture`, `generated`, or `unknown`. |
| `resolution` | `resolved`, `unresolved`, `unknown`, or `not_applicable`. |
| `derived` | Whether the edge is derived/cache/closure evidence. |
| `provenance_edges` | Required when `derived` is true. |

The schema rejects derived edges without provenance and rejects unresolved edges that claim exact/compiler/LSP/parser proof-grade exactness.

## Path Format

Each expected or forbidden path must include:

| Field | Purpose |
| --- | --- |
| `source` / `target` | Exact endpoint selectors. |
| `ordered_edges` | Ordered edge expectations, not only a relation bag. |
| `relation_sequence` | Expected relation sequence. |
| `max_length` | Maximum allowed path length. |
| `required_source_spans` | Spans that must resolve to source text for proof-grade paths. |
| `context` | Path context, including production/test/mock separation. |
| `allow_test_mock_edges` | Must be false for production proof paths. |
| `derived_edges_require_provenance` | Must be true. |

## Failure Rules

The schema defines these default failure rules for the future runner:

| Rule | Required Behavior |
| --- | --- |
| Missing required edge fails | Any `expected_edges` item not observed exactly is a failure. |
| Forbidden edge appearing fails | Any `forbidden_edges` item observed is a failure. |
| Wrong direction fails | `head -> relation -> tail` must not pass as the inverse direction. |
| Missing source span fails for proof-grade edge | Proof-grade edges must have a valid source span. |
| Unresolved name labeled exact fails | Unresolved textual evidence cannot be labeled exact/proof-grade. |
| Mock/test edge in production proof path fails | Production paths cannot be proven through mock or test facts. |
| Derived edge without provenance fails | Derived/cache/closure edges need provenance edges. |
| Stale edge after edit fails | Facts from deleted or changed source must not remain valid. |
| Same-name symbol collision fails | Same-name symbols must resolve to distinct identities when files/classes differ. |
| Critical symbol missing from context packet fails | Required context symbols must appear in generated context packets. |
| Too many distractors fail if threshold specified | `distractor_policy` thresholds are hard limits when present. |

## Validation Tests

Added tests in `crates/codegraph-bench/src/lib.rs`:

- `graph_truth_case_schema_accepts_strict_case`
- `graph_truth_case_schema_rejects_malformed_cases`
- `graph_truth_case_schema_defines_failure_rules`

The focused validation test covers:

- valid strict case with expected and forbidden edges/paths
- malformed edge missing `source_span`
- unexpected top-level field rejection
- unresolved edge mislabeled exact rejection
- derived edge without provenance rejection
- production proof path allowing test/mock edges rejection

## Runner Work Not Done

This phase intentionally did not build a graph-truth runner. The schema is ready for the next phase, but no benchmark scoring code consumes it yet.

## Next Phase

Build a runner that loads these JSON cases, indexes the fixture repo, evaluates exact expected and forbidden facts, validates proof-grade source spans, checks stale-update and identity scenarios, scores context symbols/distractors, and emits a machine-readable failure report without changing benchmark scores to improve appearances.
