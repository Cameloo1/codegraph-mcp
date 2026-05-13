# Autoresearch Update Repro Fix

Verdict: `passed`

| Repo | Status | Iterations | Repeat Hash Stable | Changed Hash | Restore Hash | Integrity |
| --- | --- | ---: | --- | --- | --- | --- |
| `autoresearch` | `passed` | 0 | `true` | `true` | `true` | `true` |

## Step Metrics

### `autoresearch`

Mutation file: `python/autoresearch_utils/metrics_tools.py`

| Step | ms | walked | read | hashed | parsed | entities inserted | edges inserted | duplicate edge upserts | integrity |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| `cold_index` | 40189 | n/a | n/a | n/a | n/a | n/a | n/a | n/a | `ok` |
| `repeat_unchanged_index` | 27311 | 8561 | 4975 | 4975 | 0 | 0 | 0 | 0 | `ok` |

