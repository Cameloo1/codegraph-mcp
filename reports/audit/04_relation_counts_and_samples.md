# 04 Relation Counts and Edge Samples

Verdict: the 2.05M edges are structurally explainable, but the largest proof-sensitive relations need manual review before aggregate metrics are trusted.

This phase uses the audit commands added for human inspection. It does not change extraction behavior, hide noisy relations, deduplicate edges, or normalize benchmark scores.

## Commands

Relation counts:

```powershell
.\target\debug\codegraph-mcp.exe audit relation-counts --db <path> --json <out.json> --markdown <out.md>
```

Edge sampling:

```powershell
.\target\debug\codegraph-mcp.exe audit sample-edges --db <path> --relation <RELATION> --limit 50 --seed 20260510 --json <out.json> --markdown <out.md> --include-snippets
```

For the large latest DB, samples were generated without snippets because the DB can be audited independently from the full source checkout:

```powershell
.\target\debug\codegraph-mcp.exe audit sample-edges --db target\codegraph-bench-report\sweep-20260509-231456\autoresearch-codegraph-attempt3\codegraph.sqlite --relation CALLS --limit 50 --seed 20260510 --json reports\audit\artifacts\edge_samples\CALLS.json --markdown reports\audit\artifacts\edge_samples\CALLS.md
```

Snippet loading was verified on a tiny fixture DB:

- `reports/audit/artifacts/sample_edges_fixture_CALLS.json`
- `reports/audit/artifacts/sample_edges_fixture_CALLS.md`

## Artifacts

- `reports/audit/artifacts/04_relation_counts_latest.json`
- `reports/audit/artifacts/04_relation_counts_latest.md`
- `reports/audit/artifacts/04_relation_counts_fixture.json`
- `reports/audit/artifacts/04_relation_counts_fixture.md`
- `reports/audit/artifacts/edge_samples/*.json`
- `reports/audit/artifacts/edge_samples/*.md`

## Top 10 Largest Relations

Latest DB: `target/codegraph-bench-report/sweep-20260509-231456/autoresearch-codegraph-attempt3/codegraph.sqlite`.

| Rank | Relation | Edges | Exactness summary | Missing standalone span rows | Notes |
| ---: | --- | ---: | --- | ---: | --- |
| 1 | `CONTAINS` | 477866 | parser_verified: 477866 | 477866 | Structural ownership dominates. |
| 2 | `DEFINED_IN` | 473033 | parser_verified: 473033 | 473033 | Inverse/dual structural edge stored as base rows. |
| 3 | `CALLEE` | 276611 | parser_verified: 20938; static_heuristic: 255673 | 276611 | Callsite target relation is mostly heuristic. |
| 4 | `CALLS` | 271509 | parser_verified: 20764; static_heuristic: 250745 | 271509 | High-risk proof relation; mostly heuristic. |
| 5 | `ARGUMENT_0` | 175879 | parser_verified: 175879 | 175879 | Callsite argument reification. |
| 6 | `DECLARES` | 112930 | parser_verified: 112930 | 112930 | Local declaration structure. |
| 7 | `FLOWS_TO` | 94236 | parser_verified: 94236 | 94236 | Dataflow-ish relation; needs manual span correctness review. |
| 8 | `ARGUMENT_1` | 39569 | parser_verified: 39569 | 39569 | Callsite argument reification. |
| 9 | `DEFINES` | 38056 | parser_verified: 38056 | 38056 | Module/file definition structure. |
| 10 | `IMPORTS` | 37779 | parser_verified: 37779 | 37779 | Import structure. |

These top 10 relations account for 1997468 of 2050123 edges, about 97.4 percent of the graph. That makes the total edge count structurally explainable: most volume comes from ownership, callsite, calls, arguments, declarations, dataflow, and imports.

## High-Risk Relation Counts

| Relation | Edges | Exactness summary | Inferred test/mock context | Samples generated |
| --- | ---: | --- | --- | ---: |
| `CALLS` | 271509 | parser_verified: 20764; static_heuristic: 250745 | test/fixture inferred: 110914 | 50 |
| `CALLEE` | 276611 | parser_verified: 20938; static_heuristic: 255673 | test/fixture inferred: 113249 | 50 |
| `READS` | 5335 | parser_verified: 2273; static_heuristic: 3062 | test/fixture inferred: 388 | 50 |
| `WRITES` | 1247 | parser_verified: 1247 | test/fixture inferred: 84 | 50 |
| `FLOWS_TO` | 94236 | parser_verified: 94236 | test/fixture inferred: 35404 | 50 |
| `MUTATES` | 199 | parser_verified: 199 | test/fixture inferred: 10 | 50 |
| `AUTHORIZES` | 0 | absent | absent | 0 |
| `CHECKS_ROLE` | 21 | static_heuristic: 21 | test/fixture inferred: 2 | 21 |
| `SANITIZES` | 61 | static_heuristic: 61 | unknown/not first-class: 61 | 50 |
| `EXPOSES` | 0 | absent | absent | 0 |
| `INJECTS` | 0 | absent | absent | 0 |
| `INSTANTIATES` | 0 | absent | absent | 0 |
| `PUBLISHES` | 1 | static_heuristic: 1 | unknown/not first-class: 1 | 1 |
| `EMITS` | 5 | static_heuristic: 5 | test/fixture inferred: 3 | 5 |
| `CONSUMES` | 80 | static_heuristic: 80 | test/fixture inferred: 24 | 50 |
| `LISTENS_TO` | 40 | static_heuristic: 40 | test/fixture inferred: 9 | 40 |
| `TESTS` | 121 | static_heuristic: 121 | test/fixture inferred: 121 | 50 |
| `MOCKS` | 12 | static_heuristic: 12 | mock/stub inferred: 12 | 12 |
| `STUBS` | 0 | absent | absent | 0 |
| `ASSERTS` | 558 | static_heuristic: 558 | test/fixture inferred: 558 | 50 |

The command reports `duplicate_edge_count = 0` for these high-risk relations under the exact duplicate definition `(head, relation, tail, span path, start/end coordinates)`. That does not prove semantic non-duplication; it only means byte-identical relation/span duplicates were not found.

## Missing Source Spans

Every relation reports `missing_source_span_rows = edge_count` in the latest DB. This is not because edge span coordinates are absent: `edges` stores inline `span_path_id`, `start_line`, `start_column`, `end_line`, and `end_column`.

The issue is that the standalone `source_spans` table has 0 rows in the latest DB, so there is no edge-id keyed row for relation-count auditing to join against. The sampler can still use inline edge spans, but MVP-style source-span auditability is weaker than the status number implies.

This finding should be investigated before treating `source_spans = 2916019` from status output as equivalent to persisted standalone source-span rows.

## Relations Most Likely Causing Noise

1. `CALLS` and `CALLEE`: both are dominated by `static_heuristic` rows. Manual review should start here.
2. `CONTAINS` and `DEFINED_IN`: massive structural volume, with `DEFINED_IN` appearing as ordinary base rows rather than clearly inverse-derived rows.
3. `FLOWS_TO`: large and proof-sensitive; parser-verified syntax does not automatically prove semantic dataflow correctness.
4. `ASSERTS`, `TESTS`, `MOCKS`: intentionally test/mock oriented, but runtime context is inferred rather than first-class.
5. `SANITIZES`, `CHECKS_ROLE`, `CONSUMES`, `LISTENS_TO`: small enough to manually inspect, but all sampled/current rows are static-heuristic.

## Human Classification Workflow

Use this shape for every high-risk relation:

```powershell
.\target\debug\codegraph-mcp.exe audit sample-edges --db target\codegraph-bench-report\sweep-20260509-231456\autoresearch-codegraph-attempt3\codegraph.sqlite --relation CALLS --limit 50 --seed 20260510 --json reports\audit\artifacts\edge_samples\CALLS.json --markdown reports\audit\artifacts\edge_samples\CALLS.md
```

Then fill the blank `classification:` line in the Markdown with one of:

```text
true_positive
false_positive
wrong_direction
wrong_target
wrong_span
stale
duplicate
unresolved_mislabeled_exact
test_mock_leaked
unsure
```

Recommended first manual passes:

- `CALLS`: classify 50 rows and separate parser-verified from static-heuristic.
- `CALLEE`: classify 50 rows even though it was not in the prompt's high-risk sample list; it is the third-largest relation and mostly heuristic.
- `FLOWS_TO`: classify 50 rows for span correctness and true flow semantics.
- `READS` and `WRITES`: classify 50 rows each for wrong target/wrong span.
- `ASSERTS`, `TESTS`, `MOCKS`: classify for test/mock leakage and usefulness in test-impact paths.
- `SANITIZES` and `CHECKS_ROLE`: classify all rows or at least 50 where available because they affect security/auth proof paths.

## Does the 2M Edge Count Look Structurally Explainable?

Yes as raw graph volume. The top relations match the current extractor shape: structural containment/definition, callsite reification, direct calls, arguments, declarations, imports, and parser-level flow edges.

No as proof volume. The highest-value relations are often heuristic, test/mock context is inferred, and standalone source-span rows are missing. Treat 2.05M as "facts emitted by extractors," not "2.05M proof-grade facts."
