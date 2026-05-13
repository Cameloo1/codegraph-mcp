# Autoresearch Update Repro Fix

Verdict: `passed`

## Remediation Notes

- The repeat-index duplicate insertion failure is fixed for the harnessed paths: unchanged repeat runs now insert `0` edges and report `0` duplicate edge upserts.
- Incremental writes remain transaction-scoped and all reported cold/repeat/update/restore steps finished with `integrity_status = ok`.
- Python-only Autoresearch updates no longer rebuild the whole TypeScript/JavaScript resolver plan before filtering. The update path now skips those global reducers unless the impacted paths include supported JS/TS resolver sources.
- Autoresearch was run from an integrity-clean current-code seed DB for the final pass because the older frozen seed predates the current extractor. That older seed also passed integrity, but its first restore hash intentionally drifted when the touched file was re-emitted by the current extractor.
- Timing is still not acceptable for intended performance: Autoresearch single-file update and restore are integrity-clean but each takes about 3 minutes, dominated by large-DB file-fact cleanup/update cost. This is a durability fix, not a storage/indexing optimization.

| Repo | Status | Iterations | Repeat Hash Stable | Changed Hash | Restore Hash | Integrity |
| --- | --- | ---: | --- | --- | --- | --- |
| `small_fixture` | `passed` | 20 | `true` | `true` | `true` | `true` |
| `medium_fixture` | `passed` | 20 | `true` | `true` | `true` | `true` |
| `autoresearch` | `passed` | 1 | `true` | `true` | `true` | `true` |

## Step Metrics

### `small_fixture`

Mutation file: `src/service.ts`

| Step | ms | walked | read | hashed | parsed | entities inserted | edges inserted | duplicate edge upserts | integrity |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `cold_index` | 191 | 2 | 2 | 2 | 2 | 21 | 43 | 3 | `ok` |
| `repeat_unchanged_index` | 28 | 2 | 0 | 0 | 0 | 0 | 0 | 0 | `ok` |
| `single_file_update` | 37 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 37 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 41 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 37 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 39 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 33 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 42 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 38 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 38 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 36 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 39 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 36 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 38 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 37 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 38 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 37 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 38 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 38 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 37 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 37 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 39 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 36 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 42 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 36 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 39 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 39 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 40 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 38 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 40 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 37 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 39 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 39 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 40 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 38 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 43 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 39 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 38 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 42 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |
| `single_file_update` | 41 | 1 | 1 | 1 | 1 | 11 | 31 | 0 | `ok` |
| `restore_update` | 36 | 1 | 1 | 1 | 1 | 7 | 20 | 0 | `ok` |

### `medium_fixture`

Mutation file: `src/module_000.ts`

| Step | ms | walked | read | hashed | parsed | entities inserted | edges inserted | duplicate edge upserts | integrity |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `cold_index` | 481 | 48 | 48 | 48 | 48 | 672 | 1440 | 144 | `ok` |
| `repeat_unchanged_index` | 37 | 48 | 0 | 0 | 0 | 0 | 0 | 0 | `ok` |
| `single_file_update` | 291 | 1 | 1 | 1 | 1 | 18 | 44 | 138 | `ok` |
| `restore_update` | 300 | 1 | 1 | 1 | 1 | 14 | 33 | 138 | `ok` |
| `single_file_update` | 268 | 1 | 1 | 1 | 1 | 18 | 44 | 138 | `ok` |
| `restore_update` | 274 | 1 | 1 | 1 | 1 | 14 | 33 | 138 | `ok` |
| `single_file_update` | 268 | 1 | 1 | 1 | 1 | 18 | 44 | 138 | `ok` |
| `restore_update` | 284 | 1 | 1 | 1 | 1 | 14 | 33 | 138 | `ok` |
| `single_file_update` | 308 | 1 | 1 | 1 | 1 | 18 | 44 | 138 | `ok` |
| `restore_update` | 270 | 1 | 1 | 1 | 1 | 14 | 33 | 138 | `ok` |
| `single_file_update` | 290 | 1 | 1 | 1 | 1 | 18 | 44 | 138 | `ok` |
| `restore_update` | 265 | 1 | 1 | 1 | 1 | 14 | 33 | 138 | `ok` |
| `single_file_update` | 268 | 1 | 1 | 1 | 1 | 18 | 44 | 138 | `ok` |
| `restore_update` | 271 | 1 | 1 | 1 | 1 | 14 | 33 | 138 | `ok` |
| `single_file_update` | 270 | 1 | 1 | 1 | 1 | 18 | 44 | 138 | `ok` |
| `restore_update` | 280 | 1 | 1 | 1 | 1 | 14 | 33 | 138 | `ok` |
| `single_file_update` | 266 | 1 | 1 | 1 | 1 | 18 | 44 | 138 | `ok` |
| `restore_update` | 277 | 1 | 1 | 1 | 1 | 14 | 33 | 138 | `ok` |
| `single_file_update` | 297 | 1 | 1 | 1 | 1 | 18 | 44 | 138 | `ok` |
| `restore_update` | 268 | 1 | 1 | 1 | 1 | 14 | 33 | 138 | `ok` |
| `single_file_update` | 260 | 1 | 1 | 1 | 1 | 18 | 44 | 138 | `ok` |
| `restore_update` | 272 | 1 | 1 | 1 | 1 | 14 | 33 | 138 | `ok` |
| `single_file_update` | 273 | 1 | 1 | 1 | 1 | 18 | 44 | 138 | `ok` |
| `restore_update` | 275 | 1 | 1 | 1 | 1 | 14 | 33 | 138 | `ok` |
| `single_file_update` | 280 | 1 | 1 | 1 | 1 | 18 | 44 | 138 | `ok` |
| `restore_update` | 279 | 1 | 1 | 1 | 1 | 14 | 33 | 138 | `ok` |
| `single_file_update` | 282 | 1 | 1 | 1 | 1 | 18 | 44 | 138 | `ok` |
| `restore_update` | 290 | 1 | 1 | 1 | 1 | 14 | 33 | 138 | `ok` |
| `single_file_update` | 263 | 1 | 1 | 1 | 1 | 18 | 44 | 138 | `ok` |
| `restore_update` | 274 | 1 | 1 | 1 | 1 | 14 | 33 | 138 | `ok` |
| `single_file_update` | 289 | 1 | 1 | 1 | 1 | 18 | 44 | 138 | `ok` |
| `restore_update` | 263 | 1 | 1 | 1 | 1 | 14 | 33 | 138 | `ok` |
| `single_file_update` | 269 | 1 | 1 | 1 | 1 | 18 | 44 | 138 | `ok` |
| `restore_update` | 277 | 1 | 1 | 1 | 1 | 14 | 33 | 138 | `ok` |
| `single_file_update` | 288 | 1 | 1 | 1 | 1 | 18 | 44 | 138 | `ok` |
| `restore_update` | 271 | 1 | 1 | 1 | 1 | 14 | 33 | 138 | `ok` |
| `single_file_update` | 284 | 1 | 1 | 1 | 1 | 18 | 44 | 138 | `ok` |
| `restore_update` | 322 | 1 | 1 | 1 | 1 | 14 | 33 | 138 | `ok` |
| `single_file_update` | 273 | 1 | 1 | 1 | 1 | 18 | 44 | 138 | `ok` |
| `restore_update` | 271 | 1 | 1 | 1 | 1 | 14 | 33 | 138 | `ok` |
| `single_file_update` | 260 | 1 | 1 | 1 | 1 | 18 | 44 | 138 | `ok` |
| `restore_update` | 283 | 1 | 1 | 1 | 1 | 14 | 33 | 138 | `ok` |

### `autoresearch`

Mutation file: `python/autoresearch_utils/metrics_tools.py`

| Step | ms | walked | read | hashed | parsed | entities inserted | edges inserted | duplicate edge upserts | integrity |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `cold_index` | 20942 | n/a | n/a | n/a | n/a | n/a | n/a | n/a | `ok` |
| `repeat_unchanged_index` | 11749 | 8561 | 0 | 0 | 0 | 0 | 0 | 0 | `ok` |
| `single_file_update` | 186143 | 1 | 1 | 1 | 1 | 18 | 35 | 0 | `ok` |
| `restore_update` | 189359 | 1 | 1 | 1 | 1 | 17 | 32 | 0 | `ok` |
