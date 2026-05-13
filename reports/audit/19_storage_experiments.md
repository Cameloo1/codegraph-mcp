# Storage Experiments

Status: completed on copied DBs only.

This phase adds `codegraph-mcp audit storage-experiments` and runs the experiment suite against:

- Small fixture DB: `reports/audit/artifacts/17_fixture_workers4.sqlite`
- Frozen integrity-clean Autoresearch baseline DB: `target/codegraph-bench-report/sweep-20260509-231456/autoresearch-codegraph-attempt3/codegraph.sqlite`

Artifacts:

- Fixture JSON: `reports/audit/artifacts/storage_experiments.json`
- Fixture Markdown: `reports/audit/artifacts/storage_experiments.md`
- Frozen baseline JSON: `reports/audit/artifacts/storage_experiments_frozen_baseline.json`
- Frozen baseline Markdown: `reports/audit/artifacts/storage_experiments_frozen_baseline.md`
- Semantic reference run: `reports/audit/artifacts/19_graph_truth_after_storage_experiments.json`

## Safety Contract

The runner copies the SQLite DB family before every mutation and refuses to use the original DB path as an experiment copy path. Successful runs remove copied workdirs by default unless `--keep-copies` is passed.

Each experiment records:

- DB size before and after
- Size delta
- Core query latency before and after
- `PRAGMA integrity_check` per checkpoint
- Context-pack-shaped query status
- Graph Truth status
- Recommendation and notes

Graph Truth is marked `not_run` / `not applicable` inside each copied-DB experiment because Graph Truth cases reindex fixture repositories instead of consuming an already-mutated DB artifact. No correctness claim is made from copied-DB query measurements alone.

## Frozen Baseline Results

Original DB family bytes: `803,528,704`.

| Experiment | Size after | Delta | Delta MiB | Delta % | Context | Recommended |
| --- | ---: | ---: | ---: | ---: | --- | --- |
| `vacuum_analyze` | 744,718,336 | -58,810,368 | -56.09 | -7.32 | queried | yes |
| `drop_recreate_edge_indexes` | 744,718,336 | -58,810,368 | -56.09 | -7.32 | queried | yes |
| `drop_broad_unused_indexes` | 711,876,608 | -91,652,096 | -87.41 | -11.41 | queried | no |
| `replace_broad_with_partial_index` | 716,177,408 | -87,351,296 | -83.30 | -10.87 | queried | no |
| `simulate_compact_qualified_names` | 575,664,128 | -227,864,576 | -217.31 | -28.36 | queried | no |
| `simulate_exact_base_partition` | 718,004,224 | -85,524,480 | -81.56 | -10.64 | queried | no |
| `disable_fts_snippets_simulation` | 744,718,336 | -58,810,368 | -56.09 | -7.32 | queried | no |
| `bulk_load_secondary_indexes` | 744,718,336 | -58,810,368 | -56.09 | -7.32 | queried | yes |

## Strongest Safe Reductions

The strongest currently safe reduction is plain copied-DB maintenance:

- `VACUUM` / `ANALYZE`: `-56.09 MiB` on the frozen baseline, with measured core/context queries still `ok`.
- Drop and recreate the same edge indexes: final size matches the `VACUUM` result and final measured queries remain `ok`.
- Bulk-load secondary indexes after insertion: final size matches the `VACUUM` result and final measured queries remain `ok`; this supports implementing bulk-load ordering later, but this experiment did not measure insertion throughput.

These are safe candidates for the next implementation trial because they do not intentionally remove graph facts or user-visible fields.

## Rejected Or Not Ready

Larger copied-DB reductions are real but not safe production optimizations yet:

- Compact qualified-name simulation saves `217.31 MiB`, but it replaces human-readable qualified names with tuple surrogates and breaks output semantics.
- Dropping broad unused indexes saves `87.41 MiB`, but source-span and retrieval-trace workflows are not fully proven without replacement query plans and Graph Truth.
- Replacing a broad index with a partial CALLS heuristic index saves `83.30 MiB`, but unresolved/relation query plans still need proof on the target query shapes.
- Exact/base partition simulation saves `81.56 MiB`, but the frozen DB lacked edge class/context metadata and the final `unresolved_calls_paginated` latency regressed from `0 ms` to `403 ms`.
- Disabling FTS/snippet storage only produced the same `VACUUM` savings on the frozen baseline because FTS payload was already effectively empty; it is not recommended without an external text-index replacement.

## Semantic Reference

Separate Graph Truth reference after the storage experiment implementation:

- Cases passed: `7`
- Cases failed: `4`
- Total cases: `11`
- Verdict: `failed`

Failing fixtures:

- `derived_closure_edge_requires_provenance`
- `file_rename_prunes_old_path`
- `import_alias_change_updates_target`
- `stale_graph_cache_after_edit_delete`

This means storage/index optimization must remain conservative. The experiment runner is safe to use for copied-DB evidence, but production storage changes should be limited to maintenance or bulk-load ordering until the semantic gate is green again.

## Tests

Commands run:

- `cargo test -q -p codegraph-cli storage_experiment`
- `cargo test -q`
- `cargo run -q -p codegraph-cli --bin codegraph-mcp -- audit storage-experiments --db reports\audit\artifacts\17_fixture_workers4.sqlite --workdir reports\audit\artifacts\storage_experiments_work --json reports\audit\artifacts\storage_experiments.json --markdown reports\audit\artifacts\storage_experiments.md`
- `cargo run -q -p codegraph-cli --bin codegraph-mcp -- audit storage-experiments --db target\codegraph-bench-report\sweep-20260509-231456\autoresearch-codegraph-attempt3\codegraph.sqlite --workdir reports\audit\artifacts\storage_experiments_frozen_work_2 --json reports\audit\artifacts\storage_experiments_frozen_baseline.json --markdown reports\audit\artifacts\storage_experiments_frozen_baseline.md`
- `cargo run -q -p codegraph-cli --bin codegraph-mcp -- bench graph-truth --cases benchmarks\graph_truth\fixtures --fixture-root benchmarks\graph_truth\fixtures --out-json reports\audit\artifacts\19_graph_truth_after_storage_experiments.json --out-md reports\audit\artifacts\19_graph_truth_after_storage_experiments.md --fail-on-forbidden --fail-on-missing-source-span --fail-on-unresolved-exact --fail-on-derived-without-provenance --fail-on-test-mock-production-leak`

Test result:

- Full Rust test suite passed.
- Graph Truth reference run failed `4 / 11` fixtures, so copied-DB storage results are not a semantic readiness signal.
