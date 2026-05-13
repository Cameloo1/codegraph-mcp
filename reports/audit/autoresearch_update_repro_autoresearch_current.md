# Autoresearch Update Repro Fix

Verdict: `passed`

| Repo | Status | Iterations | Repeat Hash Stable | Changed Hash | Restore Hash | Integrity |
| --- | --- | ---: | --- | --- | --- | --- |
| `autoresearch` | `passed` | 1 | `true` | `true` | `true` | `true` |

## Step Metrics

### `autoresearch`

Mutation file: `python/autoresearch_utils/metrics_tools.py`

| Step | ms | walked | read | hashed | parsed | entities inserted | edges inserted | duplicate edge upserts | integrity |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `cold_index` | 21951 | n/a | n/a | n/a | n/a | n/a | n/a | n/a | `ok` |
| `repeat_unchanged_index` | 12134 | 8561 | 0 | 0 | 0 | 0 | 0 | 0 | `ok` |
| `single_file_update` | 193636 | 1 | 1 | 1 | 1 | 18 | 35 | 0 | `ok` |
| `restore_update` | 188122 | 1 | 1 | 1 | 1 | 17 | 32 | 0 | `ok` |

