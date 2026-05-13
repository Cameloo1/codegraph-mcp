# Autoresearch Update Repro Fix

Verdict: `failed`

| Repo | Status | Iterations | Repeat Hash Stable | Changed Hash | Restore Hash | Integrity |
| --- | --- | ---: | --- | --- | --- | --- |
| `autoresearch` | `failed` | 1 | `true` | `true` | `false` | `true` |

## Step Metrics

### `autoresearch`

Mutation file: `python/autoresearch_utils/metrics_tools.py`

| Step | ms | walked | read | hashed | parsed | entities inserted | edges inserted | duplicate edge upserts | integrity |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `cold_index` | 32890 | n/a | n/a | n/a | n/a | n/a | n/a | n/a | `ok` |
| `repeat_unchanged_index` | 23757 | 8561 | 4975 | 4975 | 0 | 0 | 0 | 0 | `ok` |
| `single_file_update` | 192891 | 1 | 1 | 1 | 1 | 18 | 35 | 0 | `ok` |
| `restore_update` | 196558 | 1 | 1 | 1 | 1 | 17 | 32 | 0 | `ok` |

