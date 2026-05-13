# Autoresearch Update Repro Fix

Verdict: `passed`

Update mode: `update-fast`

| Repo | Status | Iterations | Repeat Hash Stable | Changed Hash | Restore Hash | Integrity |
| --- | --- | ---: | --- | --- | --- | --- |
| `autoresearch` | `passed` | 12 | `true` | `true` | `true` | `true` |

## Step Metrics

### `autoresearch`

Mutation file: `python/autoresearch_utils/metrics_tools.py`

| Step | ms | walked | read | hashed | parsed | entities inserted | edges inserted | duplicate edge upserts | integrity |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `cold_index` | 16702 | n/a | n/a | n/a | n/a | n/a | n/a | n/a | `ok` |
| `repeat_unchanged_index` | 2867 | 8561 | 1 | 1 | 0 | 0 | 0 | 0 | `ok` |
| `single_file_update` | 312 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 201 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 298 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 200 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 313 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 189 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 293 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 197 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 313 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 198 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 298 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 196 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 336 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 200 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 319 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 223 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 310 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 200 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 299 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 200 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 314 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 195 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |
| `single_file_update` | 296 | 1 | 1 | 1 | 1 | 15 | 29 | 0 | `ok` |
| `restore_update` | 198 | 1 | 1 | 1 | 1 | 14 | 26 | 0 | `ok` |

