# Frozen Baseline 20260510_171911

Source of truth: `MVP.md`.

Benchmark artifact source: `target/codegraph-bench-report/sweep-20260509-231456/`.

This file freezes the current benchmark run as a diagnostic baseline only. It records observed behavior and known weaknesses. It is not proof of graph correctness, benchmark superiority, or real agent coding quality.

## 1. Executive Verdict

Verdict: `diagnostic_unknown`.

The current baseline proves that CodeGraph can complete the Autoresearch cold index within the observed run and can produce a large SQLite graph. It does not prove that the graph is correct, proof-grade, or better than CGC. CGC timed out on the Autoresearch run at the 180 second cap, so the comparison state is `incomplete` and superiority is `unknown`.

The benchmark final gate from `target/codegraph-bench-report/sweep-20260509-231456/final-gate/summary.json` reported:

| Field | Value |
| --- | --- |
| Final verdict | `unknown` |
| Internal verdict | `pass` |
| CGC status | `incomplete` |
| Superiority claim | `false` |

The agent-quality layer is a fake-agent dry run. It is useful as a trace/packaging diagnostic and must not be treated as evidence of real model patch success.

## 2. What This Baseline Proves

- CodeGraph indexed the Autoresearch corpus in the recorded run.
- The recorded run produced a SQLite DB and status artifacts.
- The run measured file, entity, edge, and source-span counts.
- The run measured coarse query latencies, including a very slow unresolved-calls query.
- The run retained CGC timeout/partial-artifact status instead of counting the timeout as a win.
- The final gate wording already avoided a superiority claim when CGC was incomplete.

## 3. What This Baseline Does Not Prove

- It does not prove graph fact correctness.
- It does not prove relation recall, relation precision, path recall, or source-span precision on hand-labeled truth.
- It does not prove that the 2,050,123 edges are proof-grade rather than heuristic, derived, duplicate, stale, or test/mock-contaminated.
- It does not prove CodeGraph beats CGC, because CGC did not complete the comparable Autoresearch run.
- It does not prove real agent coding superiority, because the agent-quality layer is a fake-agent dry run.
- It does not prove storage goals are met; the observed SQLite family size was about 803.4 MiB.
- It does not prove the retrieval funnel stages are separately useful.

## 4. Index Speed/Storage Metrics

Autoresearch CodeGraph run artifact: `target/codegraph-bench-report/sweep-20260509-231456/autoresearch-codegraph-attempt3/run.json`.

| Metric | Value |
| --- | ---: |
| Index status | `completed` |
| Index exit code | `0` |
| Index elapsed time | `169,950 ms` |
| Index elapsed time | `169.95 s` |
| Files | `4,975` |
| Entities | `865,896` |
| Edges | `2,050,123` |
| Source spans | `2,916,019` |
| Schema version | `4` |
| DB file size from run artifact | `803,495,936 bytes` |
| DB file size, binary MiB | `766.27 MiB` |
| DB file size, decimal MB | `803.50 MB` |
| SQLite family size from status artifact | `842,441,752 bytes` |
| SQLite family size, binary MiB | `803.42 MiB` |
| Current on-disk DB plus shm/wal observed in artifact directory | `803,528,704 bytes` |

The prompt's known value, about `803.4 MiB`, matches `status.json`'s recorded `db_size_bytes` value of `842,441,752` bytes.

## 5. Query Latency Metrics

Query latency artifact: `target/codegraph-bench-report/sweep-20260509-231456/autoresearch-codegraph-attempt3/query-latencies.json`.

| Query type | Elapsed time |
| --- | ---: |
| `symbols task` | `3,309 ms` |
| `text Codex` | `3,275 ms` |
| `files agent` | `3,284 ms` |
| `unresolved-calls` | `71,271 ms` |

Slowest observed query type: `unresolved-calls`, about `71.3 s`. This is too slow for agent planning and should remain a diagnostic weakness until bounded/paginated query behavior is proven.

## 6. Quality Metrics

Current quality verdict: `unknown`.

The internal benchmark/gate pass is not enough to prove graph correctness. `MVP.md` requires compact, verified, relation-aware context and exact graph verification. Passing a command-level or fixture-level gate does not establish that expected and forbidden graph facts, source spans, production/test/mock separation, stale updates, and proof paths are correct.

Relevant diagnostic signals:

| Signal | Value |
| --- | --- |
| Internal final gate | `pass` |
| CGC comparison state | `incomplete` |
| Current quality claim | `unknown` |
| Agent patch success | `not measured with a real model` |
| Fake-agent dry-run status | `present` |

Later strict audit reports under `reports/audit/` found that graph truth and context packet gates are not yet enough for production-readiness claims. Those later reports strengthen the conclusion that this frozen benchmark is diagnostic only.

## 7. Fixture/CGC Comparison Status

CGC Autoresearch artifact: `target/codegraph-bench-report/sweep-20260509-231456/autoresearch-cgc/run.json`.

| Field | Value |
| --- | --- |
| Competitor | `CodeGraphContext` / `cgc` |
| CGC package version | `0.4.7` |
| CGC executable path | `target/cgc-competitor/.venv312-pypi/Scripts/cgc.exe` |
| Backend | `kuzudb` |
| Python version | `Python 3.12.13` |
| Autoresearch status | `timeout` |
| Timeout | `180,000 ms` |
| Partial DB size | `2,696,673 bytes` |
| Final DB size | `unknown` |
| Comparison verdict | `incomplete`, not a CodeGraph win |

The final-gate fixture comparison also recorded CGC status as `incomplete`. The harness may still contain query/parsing mismatch risk, so fixture evidence recall should not be used as a quality victory without graph truth gates and fair output normalization.

## 8. Fake-Agent Warning

Agent-quality artifact: `target/codegraph-bench-report/sweep-20260509-231456/agent-quality-command.json`.

The agent-quality layer is a fake-agent dry run. It records trace events, patches, and final-answer artifacts, but it is not real model execution. It must be labeled as a dry-run diagnostic. It cannot support a claim of agent coding superiority, patch success, or SOTA behavior.

## 9. Top Storage Contributors

Top SQLite contributors from `target/codegraph-bench-report/sweep-20260509-231456/autoresearch-codegraph-attempt3/dbstat.json`:

| Rank | Table/index | Bytes |
| ---: | --- | ---: |
| 1 | `edges` | `165,593,088` |
| 2 | `sqlite_autoindex_qualified_name_dict_1` | `125,501,440` |
| 3 | `qualified_name_dict` | `109,445,120` |
| 4 | `entities` | `62,488,576` |
| 5 | `sqlite_autoindex_object_id_dict_1` | `48,812,032` |
| 6 | `object_id_dict` | `43,884,544` |
| 7 | `idx_edges_tail_relation` | `38,608,896` |
| 8 | `idx_edges_head_relation` | `38,608,896` |
| 9 | `idx_edges_span_path` | `32,833,536` |
| 10 | `sqlite_autoindex_symbol_dict_1` | `30,224,384` |
| 11 | `symbol_dict` | `26,824,704` |
| 12 | `sqlite_autoindex_qname_prefix_dict_1` | `26,296,320` |
| 13 | `qname_prefix_dict` | `23,056,384` |

This storage profile suggests real dictionary/index bloat risk, especially around qualified names, object IDs, symbol/prefix dictionaries, edge rows, and broad edge lookup indexes. No storage optimization is applied in this baseline.

## 10. Known Weaknesses

1. Current quality is unknown until strict graph truth gates prove expected and forbidden facts.
2. CGC timed out on Autoresearch, so the comparison is incomplete and cannot imply CodeGraph superiority.
3. The fake-agent layer is not real model patch success.
4. `unresolved-calls` took about `71.3 s`, too slow for agent planning.
5. SQLite family size was about `803.4 MiB`, likely violating compact-memory/storage goals.
6. Edge volume is large enough to require proof-grade edge classification, exactness, source spans, derived provenance, and test/mock context separation.
7. Internal baseline metrics appear inconsistent and may conflate file recall, symbol recall, path recall, graph proof, and vector/retrieval effects.
8. CGC evidence recall normalization may be contaminated by query or parsing mismatch.
9. Source-span correctness cannot be assumed from count totals.
10. Passing unit tests is not enough to validate graph correctness on adversarial fixtures.

## 11. Intended Performance Targets From MVP.md And This Prompt

`MVP.md` defines a local, Rust-first graph memory layer that provides compact, verified, relation-aware context for a single Codex-style coding agent. It requires exact graph verification, proof-oriented context packets, SQLite storage for MVP, relation/path evidence, source-span citations, and explicit unknown handling.

Target expectations for the next phases:

- Keep the system single-agent only; do not introduce subagents.
- Treat exact graph verification and proof-grade source spans as required for quality claims.
- Separate indexing, retrieval, proof, context-packet quality, and real agent coding.
- Keep CGC timeout/incomplete state as unknown rather than a win.
- Treat fake-agent dry-run output as diagnostic only.
- Make unresolved-call enumeration bounded enough for agent planning.
- Keep storage compact without deleting proof facts or benchmark evidence.
- Build adversarial hand-labeled graph truth cases before trusting aggregate metrics.

This prompt adds the diagnostic target that the frozen baseline must not claim quality. The current benchmark should be used as a starting measurement, not a proof point.

## 12. Next Phase: Adversarial Fixtures

The next phase should create strict adversarial graph truth fixtures with hand-labeled expected and forbidden facts. Those fixtures should include same-name symbols, import aliases, dynamic imports, stale deletes/renames, mocks/stubs, role checks, sanitizer distractors, derived provenance, and exact callsite source spans.

The runner should fail on missing required facts, forbidden facts, wrong direction, missing proof-grade spans, unresolved names labeled exact, mock/test leakage into production proof paths, derived facts without provenance, stale facts after update, same-name collisions, and missing critical context symbols.

Until those graph truth gates exist and pass, this baseline remains `diagnostic_unknown`.

