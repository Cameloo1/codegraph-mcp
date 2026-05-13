# Autoresearch Update Repro Fix

## Diagnosis

The previous fresh update loop timeout was a harness boundary bug plus a safety bug: the run could cold-build the Autoresearch proof DB before measuring update, and it attempted to mutate the sibling Autoresearch checkout directly. In this environment that direct mutation fails with access denial, and in a normal environment it would still be the wrong operator-grade boundary.

This report uses an explicit fresh seed artifact and `--loop-kind update-fast`. The mutation file is staged under `reports/audit/artifacts/fresh_update_loop_fix_work/repos/autoresearch_update_workspace`, so the real Autoresearch checkout is not edited. Seed copy/open/schema validation, graph digest setup, quick checks, markdown rendering, and JSON writing are reported separately from update-fast operation time.

Verdict: `passed`. Update-fast p95 is `475 ms` against the `<=750 ms` target; restore p95 is `322 ms`. The update-fast iterations report `global_hash_check_ran=false`, `graph_counts_ran=false`, and `storage_audit_ran=false`.

Verdict: `passed`

Update mode: `update-fast`

| Repo | Status | Iterations | Repeat Hash Stable | Changed Hash | Restore Hash | Integrity |
| --- | --- | ---: | --- | --- | --- | --- |
| `autoresearch` | `passed` | 20 | `true` | `true` | `true` | `true` |

## Step Metrics

### `autoresearch`

Mutation file: `python/autoresearch_utils/metrics_tools.py`

| Step | ms | walked | read | hashed | parsed | entities inserted | edges inserted | duplicate edge upserts | integrity |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `cold_index` | 980 | n/a | n/a | n/a | n/a | n/a | n/a | n/a | `ok` |
| `repeat_unchanged_index` | 2428 | 8564 | 0 | 0 | 0 | 0 | 0 | 0 | `ok` |
| `single_file_update` | 445 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 310 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 475 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 313 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 463 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 322 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 472 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 296 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 459 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 324 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 468 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 285 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 492 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 314 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 399 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 294 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 330 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 212 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 380 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 220 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 344 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 218 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 360 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 231 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 334 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 221 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 341 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 228 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 379 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 259 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 363 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 216 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 337 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 231 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 427 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 238 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 331 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 216 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 332 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 221 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
