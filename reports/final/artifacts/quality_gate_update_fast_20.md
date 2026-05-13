# Autoresearch Update Repro Fix

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
| `cold_index` | 935 | n/a | n/a | n/a | n/a | n/a | n/a | n/a | `ok` |
| `repeat_unchanged_index` | 2242 | 8564 | 0 | 0 | 0 | 0 | 0 | 0 | `ok` |
| `single_file_update` | 481 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 315 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 503 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 317 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 486 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 336 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 487 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 303 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 467 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 309 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 511 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 314 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 512 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 318 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 353 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 223 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 379 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 231 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 335 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 239 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 361 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 241 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 325 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 676 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 382 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 249 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 348 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 225 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 366 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 243 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 336 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 221 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 331 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 234 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 390 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 232 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 342 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 250 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 358 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 230 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |

