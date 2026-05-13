# Relation Source Span Audit

Date: 2026-05-10

Database: `target/codegraph-bench-report/sweep-20260509-231456/autoresearch-codegraph-attempt3/codegraph.sqlite`

Baseline context:

- `reports/baselines/BASELINE_LATEST.md` identifies this as the Autoresearch-scale baseline DB: 865,896 entities, 2,050,123 edges, and 2,916,019 source spans.
- `reports/audit/04_graph_truth_gate_initial_run.md` shows the graph truth gate is still failing 8 of 10 fixtures, with known gaps around roles, barrel/default exports, derived provenance, dynamic import, dataflow, and test/mock handling.

## Commands Added

- `codegraph-mcp audit relation-counts --db <path> --json <path> --markdown <path>`
- `codegraph-mcp audit sample-edges --db <path> --relation <RELATION> --limit <N> --seed <N> --json <path> --markdown <path> --include-snippets`
- `codegraph-mcp audit sample-paths --db <path> --limit <N> --seed <N> --json <path> --markdown <path> --include-snippets`

## Artifacts

- `reports/audit/artifacts/relation_counts.json`
- `reports/audit/artifacts/relation_counts.md`
- `reports/audit/artifacts/sample_CALLS.json`
- `reports/audit/artifacts/sample_CALLS.md`
- `reports/audit/artifacts/sample_READS.json`
- `reports/audit/artifacts/sample_READS.md`
- `reports/audit/artifacts/sample_WRITES.json`
- `reports/audit/artifacts/sample_WRITES.md`
- `reports/audit/artifacts/sample_FLOWS_TO.json`
- `reports/audit/artifacts/sample_FLOWS_TO.md`
- `reports/audit/artifacts/sample_MUTATES.json`
- `reports/audit/artifacts/sample_MUTATES.md`
- `reports/audit/artifacts/sample_PathEvidence.json`
- `reports/audit/artifacts/sample_PathEvidence.md`
- Low-count security/test full samples: `SOURCE_OF_TAINT`, `VALIDATES`, `SANITIZES`, `SINKS_TO`, `CHECKS_ROLE`, `MOCKS`, `CHECKS_PERMISSION`, `FIXTURES_FOR`, `TRUST_BOUNDARY`.

Each edge sample includes endpoint IDs/names, relation, direction, source span, snippet when loadable, exactness, confidence, provenance, derived flag, fact classification, extractor, inferred production/test/mock context, and explicit missing metadata. Markdown samples include blank manual classification fields:

- `true_positive`
- `false_positive`
- `wrong_direction`
- `wrong_target`
- `wrong_span`
- `stale`
- `duplicate`
- `unresolved_mislabeled_exact`
- `test_mock_leaked`
- `derived_missing_provenance`
- `unsure`

## Performance Notes

| Command | Result |
| --- | ---: |
| `relation-counts` release build | 11.307s |
| `sample-edges CALLS`, first release run after sampler change | 2.108s |
| `sample-edges CALLS`, warm release run | 0.118s |
| `sample-edges READS` | 0.331s |
| `sample-edges WRITES` | 0.177s |
| `sample-edges FLOWS_TO` | 0.415s |
| `sample-paths` | 0.218s |

`relation-counts` is still above the intended 5s target on the Autoresearch-scale DB. The report uses an explicit fast path that scans `edges` plus `relation_kind_dict` only, and marks exactness counts, duplicate counts, source span row joins, and top entity-type grouping as `not_measured_fast_path` instead of silently implying they were checked. A relation-only index or persisted relation stats would be needed to make this exact count consistently sub-5s without reintroducing expensive joins.

## Top Relation Counts

| Relation | Edges |
| --- | ---: |
| `CONTAINS` | 477,866 |
| `DEFINED_IN` | 473,033 |
| `CALLEE` | 276,611 |
| `CALLS` | 271,509 |
| `ARGUMENT_0` | 175,879 |
| `DECLARES` | 112,930 |
| `FLOWS_TO` | 94,236 |
| `ARGUMENT_1` | 39,569 |
| `DEFINES` | 38,056 |
| `IMPORTS` | 37,779 |
| `ARGUMENT_N` | 32,020 |
| `EXPORTS` | 8,361 |
| `READS` | 5,335 |
| `ASSIGNED_FROM` | 1,969 |
| `WRITES` | 1,247 |

Other audited relations: `MUTATES` 199, `SOURCE_OF_TAINT` 86, `VALIDATES` 67, `SANITIZES` 61, `SINKS_TO` 60, `CHECKS_ROLE` 21, `MOCKS` 12, `CHECKS_PERMISSION` 4, `FIXTURES_FOR` 1, `TRUST_BOUNDARY` 1.

## Edge Sample Summary

| Relation | Samples | Spans loaded | Parser verified | Static heuristic | Production | Test/fixture | Mock/stub |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `CALLS` | 50 | 50 | 6 | 44 | 29 | 21 | 0 |
| `READS` | 50 | 50 | 19 | 31 | 45 | 5 | 0 |
| `WRITES` | 50 | 50 | 50 | 0 | 43 | 7 | 0 |
| `FLOWS_TO` | 50 | 50 | 50 | 0 | 26 | 24 | 0 |
| `MUTATES` | 50 | 50 | 50 | 0 | 46 | 4 | 0 |
| `SOURCE_OF_TAINT` | 86 | 86 | 0 | 86 | 23 | 63 | 0 |
| `VALIDATES` | 67 | 67 | 0 | 67 | 41 | 26 | 0 |
| `SANITIZES` | 61 | 61 | 0 | 61 | 61 | 0 | 0 |
| `SINKS_TO` | 60 | 60 | 0 | 60 | 40 | 20 | 0 |
| `CHECKS_ROLE` | 21 | 21 | 0 | 21 | 19 | 2 | 0 |
| `MOCKS` | 12 | 12 | 0 | 12 | 0 | 0 | 12 |
| `CHECKS_PERMISSION` | 4 | 4 | 0 | 4 | 3 | 1 | 0 |
| `FIXTURES_FOR` | 1 | 1 | 0 | 1 | 0 | 1 | 0 |
| `TRUST_BOUNDARY` | 1 | 1 | 0 | 1 | 1 | 0 | 0 |

Total audited edge samples: 563.

## Missing Metadata Problems

- `repo_commit_missing`: 563 of 563 audited edge samples.
- `metadata_empty`: 563 of 563 audited edge samples.
- `production_test_mock_context` is inferred by audit tooling from relation/path/id text. It is not first-class graph metadata.
- No audited relation has first-class metadata explaining why a heuristic security/test fact was emitted.
- No audited derived edges were found in these samples, so derived provenance could not be validated from real sampled derived facts.

## Source Span Quality Problems

- Source snippets loaded for 563 of 563 audited edge samples.
- No sampled edge had a snippet load failure.
- Source span row completeness is not measured in `relation-counts` fast mode because joining `source_spans` made the command too slow on this DB. The JSON marks that limitation explicitly with `source_span_row_status`.
- Spans are present and loadable, but span presence alone is not proof-grade: many security/dataflow/test relations are `static_heuristic` with empty metadata and no provenance rationale.

## PathEvidence Quality Problems

- Stored `path_evidence` rows: 0.
- Generated fallback PathEvidence records: 20.
- All 20 generated records loaded snippets.
- Relation sequence mix in fallback samples: `CONTAINS` 6, `DEFINED_IN` 5, `CALLS` 3, `ARGUMENT_N` 2, `CALLEE` 2, `ARGUMENT_0` 1, `DEFINES` 1.
- Fallback records are one-edge audit paths, not real proof paths for a task or query.
- `task_or_query` is null for generated fallback paths.
- 20 of 20 generated PathEvidence samples have `metadata_empty` and `repo_commit_missing`.

## Semantic Bugs To Fix Next

1. Stored PathEvidence is absent. The audit command can produce reviewable fallback path records, but the current implementation is not yet trustworthy for proof-path claims because no real path evidence is persisted.
2. Security/dataflow relations are mostly heuristic and provenance-empty. `SOURCE_OF_TAINT`, `VALIDATES`, `SANITIZES`, `SINKS_TO`, `CHECKS_ROLE`, `CHECKS_PERMISSION`, and `TRUST_BOUNDARY` need extractor-specific metadata and provenance before they can be treated as proof-grade.
3. Test/mock separation is not first-class. The audit can infer test and mock context, but production proof-path exclusion cannot be reliable until context is stored on edges or entities.
4. `CALLS` exactness is weak in the sample: 44 of 50 sampled `CALLS` are `static_heuristic`.
5. Relation-count performance is still above target without a relation-only index or persisted count table.

Trustworthiness verdict: useful for inspection, not yet trustworthy for benchmark correctness or security proof paths. The highest-priority semantic bug is missing stored PathEvidence with provenance-rich, test/mock-aware path edges.
