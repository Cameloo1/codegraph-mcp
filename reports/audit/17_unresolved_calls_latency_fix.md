# 17 Unresolved Calls Latency Fix

Verdict: fixed for agent-planning enumeration; still not optimized for full accounting.

`MVP.md` says unresolved calls must remain explicit, heuristic evidence unless a resolver proves the target, and exact graph/source verification must not fake proof. This phase therefore did not change extraction, edge exactness, benchmark scoring, or storage layout. It only bounded and instrumented the unresolved-call enumeration path.

## Sources Inspected

- `MVP.md`
- `reports/audit/03_storage_forensics.md`
- `target/codegraph-bench-report/sweep-20260509-231456/index.html`
- `target/codegraph-bench-report/sweep-20260509-231456/autoresearch-codegraph-attempt3/query-latencies.json`
- `target/codegraph-bench-report/sweep-20260509-231456/autoresearch-codegraph-attempt3/status.json`
- `crates/codegraph-cli/src/lib.rs`
- `crates/codegraph-store/src/sqlite.rs`
- `crates/codegraph-cli/tests/cli_smoke.rs`

## Root Cause

The 71.3s run was caused primarily by unbounded materialization, not snippet loading.

Before this phase, `codegraph-mcp query unresolved-calls` called `query_unresolved_calls(&current_repo_root()?, 100)`, then loaded up to `UNBOUNDED_STORE_READ_LIMIT` entities, loaded up to `UNBOUNDED_STORE_READ_LIMIT` edges, built an entity map, filtered unresolved `CALLS` in Rust, and optionally fell back to parsing indexed source files. On the latest Autoresearch DB this means walking very large joined dictionaries and edge rows before returning a small agent-planning list.

Storage audit 03 also showed there is no broad `idx_edges_relation` index. The new query plan still reports `SCAN e`, so missing relation/exactness indexing remains a measured storage/query risk. The important fix here is that the agent-planning command is now page-bounded and no longer loads the whole graph by default.

## What Changed

- Added bounded CLI flags:
  - `--limit <n>` with an effective cap of 500
  - `--offset <n>` / `--cursor <n>`
  - `--json`
  - `--no-snippets`
  - `--include-snippets`
  - `--db <path>` for audit measurements against artifact DBs
- Default behavior now avoids source snippets and source reparsing.
- Added SQL-level unresolved-call filtering over `CALLS`, `static_heuristic`, unresolved metadata, heuristic/static-reference tails, and unknown callee names.
- Added instrumentation:
  - SQL query text
  - `EXPLAIN QUERY PLAN`
  - returned row counts and optional audit-only total count via `--count-total`
  - elapsed times for DB open, explain, count, page query, and total command work
  - query-plan index/full-scan summary
- Left storage/indexes unchanged.

## Measurements

Baseline artifact:

| Source | Query | Time |
| --- | --- | ---: |
| `target/codegraph-bench-report/sweep-20260509-231456/autoresearch-codegraph-attempt3/query-latencies.json` | old `unresolved-calls` | 71,271 ms |

New large-DB page measurement:

| Artifact | Command | Returned | Instrumented total | Page query | Count |
| --- | --- | ---: | ---: | ---: | --- |
| `reports/audit/artifacts/17_unresolved_calls_large_after.json` | `cargo run -p codegraph-cli -- query unresolved-calls --db target\codegraph-bench-report\sweep-20260509-231456\autoresearch-codegraph-attempt3\codegraph.sqlite --limit 100 --json --no-snippets` | 100 | 14 ms | 13 ms | skipped |
| `reports/audit/artifacts/17_unresolved_calls_large_count.json` | same DB, `--limit 1 --count-total` | 1 | 11,564 ms | 0 ms | 11,557 ms |

The audit-only count found `250745` unresolved-call matches. That full count is intentionally not the default because it scans the large edge table.

## Query Plan

The page query uses dictionary primary-key lookups for returned rows, but the important plan detail is:

```text
SCAN e
```

This means broad relation-only unresolved-call accounting remains expensive without a relation/exactness-oriented index. The bounded page is usable because it stops after the requested page instead of materializing all entities/edges.

## Slowness Classification

| Suspected cause | Verdict |
| --- | --- |
| Full table scan | Still present in SQLite plan for `edges e`; bounded pages make it tolerable for first-page planning. |
| Missing relation/exactness index | Real risk; not changed in this phase because storage audit requires measurement before adding broad indexes. |
| Joining large text dictionaries | Previously severe through `list_entities`/`list_edges`; now limited to returned page rows and filter checks. |
| No pagination | Fixed. |
| Resolving too much at once | Fixed for default command. Full total count remains opt-in. |
| Source snippet loading | Not the 71.3s root cause, but now explicitly off by default. |
| Source reparsing fallback | Now disabled by default; available only through `--source-scan` for explicit audit use. |

## Safety Notes

- No production extraction behavior changed.
- No benchmark scoring changed.
- No storage optimization or index creation was applied.
- Heuristic/unresolved facts remain labeled as heuristic evidence; this patch does not upgrade them.

## Tests

- `cargo test -p codegraph-cli --test cli_smoke unresolved_calls_query_is_bounded_and_instrumented`
- `cargo test -p codegraph-cli --test cli_smoke index_status_and_query_commands_work_on_fixture_repo`
- `cargo test -p codegraph-cli --test cli_smoke`
- `cargo test --workspace`
- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`

## Next Work

1. Measure a copied DB with a candidate `(relation_id, exactness_id, id_key)` index and relation-specific partial indexes before changing storage.
2. Add a dedicated unresolved-call audit report that buckets results by language, file, exactness, and tail kind.
3. Decide whether agent planning needs a cursor over `id_key` instead of offset for deep pages on huge DBs.
4. Keep snippets opt-in and cap any source-scan mode by file count as well as row count.
