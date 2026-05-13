# Indexing Performance Gate

Timestamp: 2026-05-10 22:03:09 -05:00

## Verdict

Verdict: `fail`.

Storage optimization is **not safe to start** as a broad next phase. The fixture-scale indexer now shows deterministic worker output and healthy incremental skip/update behavior, but strict semantic correctness still fails 10/11 Graph Truth cases and Autoresearch-scale repeat/update benchmarks fail before producing valid summaries.

No speed win is claimed in this report because the Graph Truth Gate regressed/still fails one semantic fixture and the large-repo benchmark path is not clean.

## Tests Run

| Check | Result | Artifact |
| --- | --- | --- |
| Unit tests | Passed | `cargo test -q` |
| Graph Truth Gate | Failed: 10/11 passed | `reports/audit/artifacts/17_graph_truth.json` |
| Fixture cold/repeat/update benchmarks | Completed | `reports/audit/artifacts/17_fixture_benchmark_raw.json` |
| Autoresearch cold index | Failed: timed out after 180s window | `reports/audit/artifacts/17_autoresearch_cold_direct.sqlite` |
| Autoresearch repeat unchanged | Failed: duplicate edge insertion | `reports/audit/artifacts/17_autoresearch_cold_only.sqlite` |
| Autoresearch single-file update | Failed: SQLite write-path error | `reports/audit/artifacts/17_autoresearch_single_update_full.sqlite` |

Graph Truth failure:

- `derived_closure_edge_requires_provenance`
- Missing entity: `src/store.ordersTable`
- Missing edge: `src/store.saveOrder -WRITES-> src/store.ordersTable`
- Missing edge: `src/service.submitOrder -MAY_MUTATE-> src/store.ordersTable`
- Missing path: `path://derived/provenance`
- Forbidden edge/path hits: `0`
- Source-span failures: `0`
- Stale failures: `0`

## Baseline Comparison

Frozen baseline: `reports/baselines/frozen_baseline_20260510_171911.md`.

| Metric | Frozen baseline | Current accepted Autoresearch status DB | Current clean cold attempt |
| --- | ---: | ---: | ---: |
| Status | completed diagnostic | status-readable, no trusted timing | timed out |
| Wall time | 169,950 ms | unknown | >180,202 ms |
| Files | 4,975 | 1,540 | 1,975 partial/status-readable |
| Entities | 865,896 | 437,263 | 553,846 partial/status-readable |
| Edges | 2,050,123 | 1,044,611 | 1,320,810 partial/status-readable |
| Source spans | 2,916,019 | 1,481,874 | 1,874,656 partial/status-readable |
| Schema version | 4 | 5 | 5 |
| SQLite family size | 842,441,752 bytes | 500,717,264 bytes | 629,411,656 bytes |

The current large-repo counts are not apples-to-apples with the frozen baseline because the accepted current status DB covers fewer files. The clean current cold attempt exceeded the benchmark window and did not return a successful index summary, so it is not accepted as a completed timing run.

## Fixture Corpus

Root: `benchmarks/graph_truth/fixtures`.

| Mode | Status | Wall | CPU | Peak memory | Walked | Read | Hashed | Parsed | Entities | Edges | Source spans | DB after |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Cold, workers 1 | passed | 201 ms | 188 ms | 13,062,144 B | 49 | 28 | 28 | 28 | 429 | 833 | 429 | 630,784 B |
| Cold, workers 4 | passed | 201 ms | 156 ms | 13,443,072 B | 49 | 28 | 28 | 28 | 429 | 833 | 429 | 630,784 B |
| Repeat unchanged | passed | 104 ms | 78 ms | 10,108,928 B | 49 | 0 | 0 | 0 | 429 | 833 | 429 | 638,976 B |
| Single-file update | passed | 37 ms | 16 ms | 4,935,680 B | 1 | 1 | 1 | 1 | 41 | 94 | 41 | 274,432 B |
| Delete/rename update | passed | 34 ms | 31 ms | 4,931,584 B | 3 | 2 | 2 | 2 | 18 | 37 | 18 | 262,144 B |

Worker-count determinism:

| Workers | Graph fact hash |
| ---: | --- |
| 1 | `6016de052d15311ee040211d5712ddf7a29f8d60d3d6a4fe6ede210c23950a91` |
| 4 | `6016de052d15311ee040211d5712ddf7a29f8d60d3d6a4fe6ede210c23950a91` |

The fixture repeat run is the cleanest incremental signal: 28 metadata-unchanged source files, zero source reads, zero hashes, zero parses, and an unchanged graph hash.

## Autoresearch Scale

Repo detected: `<REPO_ROOT>/Desktop\development\autoresearch-codexlab`.

| Mode | Status | Wall | Target | Result |
| --- | --- | ---: | ---: | --- |
| Cold index, workers 4 | timed out | >180,202 ms | <=60,000 ms | fail |
| Repeat unchanged, workers 4 | failed | 7,634 ms / 7,673 ms | <=5,000 ms | fail |
| Single-file update | failed | 3,559 ms | <=750 ms | fail |

Autoresearch repeat unchanged failure:

```text
sqlite error: UNIQUE constraint failed: edges.id_key
```

Autoresearch single-file update failure:

```text
sqlite error: database disk image is malformed
```

The temporary single-file edit to `python/autoresearch_utils/metrics_tools.py` was restored after measurement.

## Target Pass/Fail

| Target | Verdict |
| --- | --- |
| Autoresearch cold <=60s | fail |
| Autoresearch cold <=45s stretch | fail |
| Autoresearch repeat unchanged <=5s | fail |
| Autoresearch repeat unchanged <=2s stretch | fail |
| Autoresearch single-file update <=750ms p95 | fail |
| Autoresearch single-file update <=300ms stretch | fail |
| 25k-file repo targets | not measured, repo unavailable |
| 50k-file repo targets | not measured, repo unavailable |

## Current Bottlenecks

1. Semantic correctness remains blocked by the derived/provenance fixture.
2. Repeat indexing at Autoresearch scale attempts duplicate edge insertion on `edges.id_key`.
3. Autoresearch single-file update fails in the SQLite write path before reporting manifest counters.
4. Cold Autoresearch indexing did not complete within a 180 second benchmark window.
5. Small fixture runs are dominated by SQLite write/index bookkeeping, not parsing.

## Storage Work Readiness

Do not proceed to broad storage optimization yet.

The next safe step is a correctness-preserving fix for the Autoresearch repeat/update write path, especially duplicate edge insertion during skipped repeats and the SQLite malformed-image failure in `watch`. After that, rerun this gate and only compare speed/storage where Graph Truth is not worse and the large-repo commands complete cleanly.
